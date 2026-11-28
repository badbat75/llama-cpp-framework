#[cfg(test)]
use super::*;
use pai_core::client::pipe::CLIENT_ACCESS;

// ── Start at boot: the service's start type ─────────────────────────
// Values from windows-sys (Microsoft's metadata), meanings from
// CreateServiceW / ChangeServiceConfigW's dwStartType
// (learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-changeserviceconfigw).

/// `SERVICE_AUTO_START` (2): started by the SCM during system startup.
/// `SERVICE_DEMAND_START` (3): started only when asked, the start type
/// a new instance's service is created with, so it starts disabled.
#[test]
fn the_start_at_boot_is_the_automatic_start_type_and_its_absence_manual() {
    assert_eq!(boot_start_type(true), 2, "SERVICE_AUTO_START");
    assert_eq!(boot_start_type(false), 3, "SERVICE_DEMAND_START");
    assert_eq!(boot_start_type(false), SERVICE_DEMAND_START);
    let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
    assert_eq!(spec.start_type, boot_start_type(false), "created disabled");
}

/// Only an automatic start starts the service at boot (a delayed one is
/// the same start type with a flag on the side); manual, disabled
/// (`SERVICE_DISABLED`, 4) and the drivers' boot and system starts (0,
/// 1) do not.
#[test]
fn only_the_automatic_start_type_reads_as_started_at_boot() {
    for (start_type, enabled) in [(2, true), (3, false), (4, false), (0, false), (1, false)] {
        assert_eq!(boot_enabled_of(start_type), enabled, "{start_type}");
    }
    for enabled in [true, false] {
        assert_eq!(boot_enabled_of(boot_start_type(enabled)), enabled);
    }
}

/// The Win32 side of the start at boot (scm.rs, compiled on Windows
/// only, so pinned on its text): it reads the start type with
/// `QueryServiceConfigW` and changes it through the two functions
/// above. The spec applied again at every manager start (`sync`'s
/// `reconfigure`) passes `SERVICE_NO_CHANGE` as the start type, so it
/// never undoes the user's choice; only `CreateServiceW` takes the
/// spec's (manual) start type.
#[test]
fn the_scm_backend_reads_and_changes_the_start_type_and_sync_keeps_it() {
    let scm = include_str!("windows/scm.rs");
    for want in [
        "QueryServiceConfigW",
        "boot_start_type(",
        "boot_enabled_of(",
    ] {
        assert!(scm.contains(want), "scm.rs does not use {want}");
    }
    let start = scm
        .find("fn reconfigure(")
        .expect("scm.rs keeps `reconfigure`");
    let body = &scm[start..];
    let body = &body[..body.find("\n}\n").expect("the end of reconfigure")];
    assert!(body.contains("SERVICE_NO_CHANGE"), "{body}");
    assert!(!body.contains("spec.start_type"), "{body}");
}

// ── The pipe's security descriptor ──────────────────────────────────

/// One ACE of an SDDL DACL: (type, flags, rights, trustee).
fn aces(sddl: &str) -> Vec<(String, String, String, String)> {
    let dacl = sddl
        .strip_prefix("D:")
        .unwrap_or_else(|| panic!("{sddl:?}"));
    let body = dacl.trim_start_matches(|c: char| c.is_ascii_uppercase());
    assert!(body.starts_with('(') && body.ends_with(')'), "{sddl:?}");
    body[1..body.len() - 1]
        .split(")(")
        .map(|ace| {
            let f: Vec<&str> = ace.split(';').collect();
            assert_eq!(f.len(), 6, "ace {ace:?}");
            assert_eq!((f[3], f[4]), ("", ""), "no object GUIDs: {ace:?}");
            (f[0].into(), f[1].into(), f[2].into(), f[5].into())
        })
        .collect()
}

#[test]
fn the_pipe_denies_network_logons_first() {
    let aces = aces(PIPE_SDDL);
    assert_eq!(
        aces.first(),
        Some(&("D".into(), "".into(), "GA".into(), "NU".into())),
        "{PIPE_SDDL:?}"
    );
}

#[test]
fn the_pipe_gives_system_and_administrators_full_access() {
    let aces = aces(PIPE_SDDL);
    for who in ["SY", "BA"] {
        assert!(
            aces.contains(&("A".into(), "".into(), "GA".into(), who.into())),
            "{who}: {PIPE_SDDL:?}"
        );
    }
}

/// Interactive users read and write data and nothing else: the rights
/// pai-core's client asks for, without `FILE_CREATE_PIPE_INSTANCE`
/// (0x4, which `FILE_GENERIC_WRITE` and `GW` include), so no user can
/// create an instance of the manager's pipe.
#[test]
fn interactive_users_get_exactly_the_client_s_rights() {
    let aces = aces(PIPE_SDDL);
    let iu: Vec<_> = aces.iter().filter(|a| a.3 == "IU").collect();
    assert_eq!(iu.len(), 1, "{PIPE_SDDL:?}");
    let (kind, flags, rights, _) = iu[0];
    assert_eq!((kind.as_str(), flags.as_str()), ("A", ""));
    let mask = u32::from_str_radix(rights.trim_start_matches("0x"), 16)
        .unwrap_or_else(|_| panic!("rights as hex: {rights:?}"));
    assert_eq!(mask, CLIENT_ACCESS);
    assert_eq!(mask & 0x4, 0, "FILE_CREATE_PIPE_INSTANCE: {rights}");
}

#[test]
fn nobody_else_is_on_the_pipe() {
    let trustees: Vec<String> = aces(PIPE_SDDL).into_iter().map(|a| a.3).collect();
    let bff = service_sid(BFF_SERVICE);
    assert_eq!(
        trustees,
        ["NU", "SY", "BA", "IU", bff.as_str()],
        "{PIPE_SDDL:?}"
    );
}

/// The bff's Windows service, as the MSI installs it and `pai-bff
/// service` registers its control handler under.
const BFF_SERVICE: &str = "private-ai-bff";

/// The SID of service `name`, as Windows derives it ([MS-LSAT] 3.1.1.1.2,
/// "Configurable Translation Database and Corresponding View"): the name
/// upper-cased in UTF-16, its SHA-1 read as five unsigned 32-bit
/// integers, `S-1-5-80-h0-h1-h2-h3-h4`. A virtual account
/// `NT SERVICE\<name>` has it as its user SID.
fn service_sid(name: &str) -> String {
    use sha1::{Digest, Sha1};
    let utf16: Vec<u8> = name
        .to_uppercase()
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let hash = Sha1::digest(&utf16);
    let words: Vec<String> = hash
        .chunks(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]).to_string())
        .collect();
    format!("S-1-5-80-{}", words.join("-"))
}

/// The helper against known SIDs: the example in [MS-LSAT] 3.1.1.1.2
/// (ALG) and `NT SERVICE\TrustedInstaller`, whose SID owns the system's
/// files; the case of the name does not matter.
#[test]
fn the_service_sid_rule_gives_the_documented_sids() {
    assert_eq!(
        service_sid("ALG"),
        "S-1-5-80-2387347252-3645287876-2469496166-3824418187-3586569773"
    );
    assert_eq!(
        service_sid("TrustedInstaller"),
        "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
    );
    assert_eq!(service_sid("alg"), service_sid("ALG"));
}

/// The bff runs as the virtual account `NT SERVICE\private-ai-bff`,
/// which is not interactive: the pipe gives its SID (the service SID,
/// fixed by the service's name, so a constant in the descriptor) the
/// client's rights and nothing more, so it can connect and ask the calls
/// that need no authorization.
#[test]
fn the_bff_s_account_gets_exactly_the_client_s_rights() {
    let bff = service_sid(BFF_SERVICE);
    let aces = aces(PIPE_SDDL);
    let mine: Vec<_> = aces.iter().filter(|a| a.3 == bff).collect();
    assert_eq!(mine.len(), 1, "one ACE for {bff}: {PIPE_SDDL:?}");
    let (kind, flags, rights, _) = mine[0];
    assert_eq!((kind.as_str(), flags.as_str()), ("A", ""));
    let mask = u32::from_str_radix(rights.trim_start_matches("0x"), 16)
        .unwrap_or_else(|_| panic!("rights as hex: {rights:?}"));
    assert_eq!(mask, CLIENT_ACCESS);
    assert_eq!(mask & 0x4, 0, "FILE_CREATE_PIPE_INSTANCE: {rights}");
}

/// Connecting is all the bff gets: its token (the service SID as user,
/// SERVICE and ALL SERVICES, no INTERACTIVE, no Administrators) may
/// neither control nor configure, so through the pipe it reads the
/// instances and their states only.
#[test]
fn the_bff_s_token_may_neither_control_nor_configure() {
    let bff = service_sid(BFF_SERVICE);
    let mut groups = common();
    groups.push(group("S-1-5-6", 0x7)); // SERVICE
    groups.push(group("S-1-5-80-0", 0x7)); // NT SERVICE\ALL SERVICES
    let facts = TokenFacts::from_groups(&bff, &groups, Some(PAI));
    assert_eq!(
        facts,
        TokenFacts {
            sid: bff.clone(),
            interactive: false,
            admin_member: false,
            pai_member: false,
        }
    );
    for action in [Action::Control, Action::Configure] {
        assert!(!allowed(&facts, action), "{action:?}");
        let authorized =
            TokenAuthority::with_caller(facts.clone(), |c| TokenAuthority.is_authorized(c, action));
        assert!(!authorized, "{action:?} through the authority");
    }
}

/// The bff's token as the pipe sees it: its service SID, SERVICE and
/// ALL SERVICES, no INTERACTIVE, no Administrators.
fn bff_facts() -> TokenFacts {
    let mut groups = common();
    groups.push(group("S-1-5-6", 0x7));
    groups.push(group("S-1-5-80-0", 0x7));
    TokenFacts::from_groups(&service_sid(BFF_SERVICE), &groups, Some(PAI))
}

/// The connection of `NT SERVICE\private-ai-bff` is the bff, by its
/// service SID, the only caller `Authenticate` answers.
#[test]
fn the_bff_s_connection_is_the_bff() {
    let is_bff = TokenAuthority::with_caller(bff_facts(), |c| TokenAuthority.is_bff(c));
    assert!(is_bff);
}

/// Any other connection is not: a user, an administrator, SYSTEM, another
/// service's account; nor a caller other than the connection's (the
/// bff's SID named by another connection), nor any caller outside a
/// connection or on another thread.
#[test]
fn no_other_connection_is_the_bff() {
    let bff = Caller(service_sid(BFF_SERVICE));
    for f in [
        facts(true, false, false),
        facts(true, true, false),
        TokenFacts::from_groups("S-1-5-18", &common(), Some(PAI)),
        TokenFacts::from_groups(&service_sid("private-ai-manager"), &common(), Some(PAI)),
        TokenFacts::from_groups(&service_sid("llama.cpp_default"), &common(), Some(PAI)),
    ] {
        let sid = f.sid.clone();
        let (own, forged) = TokenAuthority::with_caller(f, |c| {
            (TokenAuthority.is_bff(c), TokenAuthority.is_bff(&bff))
        });
        assert!(!own, "{sid} taken for the bff");
        assert!(!forged, "{sid}'s connection naming the bff's SID");
    }
    assert!(!TokenAuthority.is_bff(&bff), "outside any connection");
    TokenAuthority::with_caller(bff_facts(), |caller| {
        let caller = caller.clone();
        let seen = std::thread::spawn(move || TokenAuthority.is_bff(&caller))
            .join()
            .expect("thread");
        assert!(!seen, "another thread has no connection");
        assert!(!TokenAuthority.is_bff(&Caller(String::new())));
    });
}

#[test]
fn the_pipe_is_a_local_byte_stream() {
    const PIPE_TYPE_MESSAGE: u32 = 0x4;
    const PIPE_READMODE_MESSAGE: u32 = 0x2;
    const PIPE_NOWAIT: u32 = 0x1;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
    assert_eq!(PIPE_MODE, PIPE_REJECT_REMOTE_CLIENTS, "{PIPE_MODE:#x}");
    assert_eq!(
        PIPE_MODE & (PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_NOWAIT),
        0
    );
}

// ── Who may do what ──────────────────────────────────────────────────

const USER: &str = "S-1-5-21-1004336348-1177238915-682003330-1001";

fn group(sid: &str, attributes: u32) -> TokenGroup {
    TokenGroup {
        sid: sid.into(),
        attributes,
    }
}

/// Groups as a real token holds them: Everyone, Users, Authenticated
/// Users, the logon SID, Local (attributes 0x7: mandatory, enabled by
/// default, enabled).
fn common() -> Vec<TokenGroup> {
    vec![
        group("S-1-1-0", 0x7),
        group("S-1-5-32-545", 0x7),
        group("S-1-5-11", 0x7),
        group("S-1-5-5-0-389150", 0xC000_0007),
        group("S-1-2-0", 0x7),
    ]
}

fn facts(interactive: bool, admin_member: bool, pai_member: bool) -> TokenFacts {
    TokenFacts {
        sid: USER.into(),
        interactive,
        admin_member,
        pai_member,
    }
}

#[test]
fn the_well_known_sids() {
    assert_eq!(SID_ADMINISTRATORS, "S-1-5-32-544");
    assert_eq!(SID_INTERACTIVE, "S-1-5-4");
}

#[test]
fn a_token_with_no_groups_is_nobody() {
    assert_eq!(
        TokenFacts::from_groups(USER, &[], Some(PAI)),
        facts(false, false, false)
    );
    assert_eq!(
        TokenFacts::from_groups(USER, &common(), Some(PAI)),
        facts(false, false, false)
    );
}

#[test]
fn a_network_logon_is_not_interactive() {
    let mut groups = common();
    groups.push(group("S-1-5-2", 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(false, false, false)
    );
}

#[test]
fn a_standard_user_at_the_console_is_interactive() {
    let mut groups = common();
    groups.push(group(SID_INTERACTIVE, 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(true, false, false)
    );
}

/// A deny-only INTERACTIVE (a restricted token) grants nothing.
#[test]
fn a_deny_only_interactive_group_does_not_count() {
    let mut groups = common();
    groups.push(group(SID_INTERACTIVE, SE_GROUP_USE_FOR_DENY_ONLY));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(false, false, false)
    );
}

/// The owner of a personal PC, not elevated: UAC's filtered token holds
/// Administrators deny-only (0x10); still a member.
#[test]
fn a_filtered_admin_token_is_an_admin_member() {
    let mut groups = common();
    groups.push(group(SID_INTERACTIVE, 0x7));
    groups.push(group(SID_ADMINISTRATORS, SE_GROUP_USE_FOR_DENY_ONLY));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(true, true, false)
    );
}

/// Elevated: Administrators enabled and owner (0xF).
#[test]
fn an_elevated_admin_token_is_an_admin_member() {
    let mut groups = common();
    groups.push(group(SID_ADMINISTRATORS, 0xF));
    groups.push(group(SID_INTERACTIVE, 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(true, true, false)
    );
}

/// The SID string is compared whole: a domain group whose SID merely
/// starts like the alias is not it.
#[test]
fn a_look_alike_sid_is_not_administrators() {
    let mut groups = common();
    groups.push(group("S-1-5-32-5440", 0x7));
    groups.push(group("S-1-5-41", 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(false, false, false)
    );
}

#[test]
fn the_user_sid_is_kept() {
    assert_eq!(
        TokenFacts::from_groups("S-1-5-18", &[], Some(PAI)).sid,
        "S-1-5-18"
    );
}

/// The SID of a local group `pai` as `LookupAccountNameW` gives it: the
/// machine's domain SID and a RID of its own (1000 and up).
const PAI: &str = "S-1-5-21-1004336348-1177238915-682003330-1009";

/// The computer name `GetComputerNameW` answers for the fixture machine.
const COMPUTER: &str = "WORKSTATION-7";

#[test]
fn the_account_use_values_are_the_windows_ones() {
    assert_eq!(SID_TYPE_USER, 1);
    assert_eq!(SID_TYPE_GROUP, 2);
    assert_eq!(SID_TYPE_DOMAIN, 3);
    assert_eq!(SID_TYPE_ALIAS, 4);
    assert_eq!(SID_TYPE_WELL_KNOWN_GROUP, 5);
    assert_eq!(SID_TYPE_DELETED_ACCOUNT, 6);
    assert_eq!(SID_TYPE_INVALID, 7);
    assert_eq!(SID_TYPE_UNKNOWN, 8);
    assert_eq!(SID_TYPE_COMPUTER, 9);
    assert_eq!(SID_TYPE_LABEL, 10);
    assert_eq!(SID_TYPE_LOGON_SESSION, 11);
}

/// The looked-up SID is this machine's `pai` only when the account is a
/// local group (an alias) of this computer: everything the name search
/// finds beyond the local computer (its order: local, primary domain,
/// trusted domains) is refused.
#[test]
fn the_pai_sid_is_accepted_only_for_a_local_alias_of_this_computer() {
    // The local group `pai`: an alias, whose domain is the computer.
    assert_eq!(
        local_alias_sid(SID_TYPE_ALIAS, COMPUTER, COMPUTER, PAI),
        Some(PAI)
    );
    // Windows compares names case-insensitively: `LookupAccountNameW`
    // reports the domain upper-case, `GetComputerNameW` answers with
    // the name as it was set.
    assert_eq!(
        local_alias_sid(SID_TYPE_ALIAS, "workstation-7", COMPUTER, PAI),
        Some(PAI)
    );
    assert_eq!(
        local_alias_sid(SID_TYPE_ALIAS, COMPUTER, "workstation-7", PAI),
        Some(PAI)
    );
    // An alias of another computer or domain (a trusted domain's local
    // group): its members are not this machine's.
    assert_eq!(
        local_alias_sid(SID_TYPE_ALIAS, "CORP", COMPUTER, PAI),
        None,
        "alias of another domain"
    );
    // A domain group named `pai` (SidTypeGroup), in the domain, and as
    // an account type on this computer's name: not an alias.
    assert_eq!(
        local_alias_sid(SID_TYPE_GROUP, "CORP", COMPUTER, PAI),
        None,
        "domain global group"
    );
    assert_eq!(
        local_alias_sid(SID_TYPE_GROUP, COMPUTER, COMPUTER, PAI),
        None,
        "a global group is not an alias"
    );
    // A user named `pai`, local or the domain's (SidTypeUser).
    assert_eq!(
        local_alias_sid(SID_TYPE_USER, COMPUTER, COMPUTER, PAI),
        None,
        "local user"
    );
    assert_eq!(
        local_alias_sid(SID_TYPE_USER, "CORP", COMPUTER, PAI),
        None,
        "domain user"
    );
    // Every other account type, and an account with no domain reported.
    for use_for in [
        SID_TYPE_DOMAIN,
        SID_TYPE_WELL_KNOWN_GROUP,
        SID_TYPE_DELETED_ACCOUNT,
        SID_TYPE_INVALID,
        SID_TYPE_UNKNOWN,
        SID_TYPE_COMPUTER,
        SID_TYPE_LABEL,
        SID_TYPE_LOGON_SESSION,
        0,
    ] {
        assert_eq!(
            local_alias_sid(use_for, COMPUTER, COMPUTER, PAI),
            None,
            "account type {use_for}"
        );
    }
    assert_eq!(local_alias_sid(SID_TYPE_ALIAS, "", COMPUTER, PAI), None);
}

/// End to end through the token's facts: with no local `pai`, the
/// domain's group (or a user) named `pai` that the name search finds is
/// refused, so a token holding its SID grants no `configure`; the same
/// token with the local alias accepted does configure.
#[test]
fn a_domain_account_named_pai_makes_nobody_a_member() {
    let mut groups = common();
    groups.push(group(SID_INTERACTIVE, 0x7));
    groups.push(group(PAI, 0x7));
    for (use_for, domain) in [
        (SID_TYPE_GROUP, "CORP"),
        (SID_TYPE_ALIAS, "CORP"),
        (SID_TYPE_USER, "CORP"),
        (SID_TYPE_USER, COMPUTER),
    ] {
        let pai = local_alias_sid(use_for, domain, COMPUTER, PAI);
        let f = TokenFacts::from_groups(USER, &groups, pai);
        assert!(!f.pai_member, "{use_for} in {domain:?}: {f:?}");
        assert!(
            !allowed(&f, Action::Configure),
            "{use_for} in {domain:?}: {f:?}"
        );
    }
    let pai = local_alias_sid(SID_TYPE_ALIAS, &COMPUTER.to_lowercase(), COMPUTER, PAI);
    let f = TokenFacts::from_groups(USER, &groups, pai);
    assert!(f.pai_member, "{f:?}");
    assert!(allowed(&f, Action::Configure), "{f:?}");
}

/// The group's SID is looked up from its name (`LookupAccountNameW`) for
/// each connection, not cached at the manager's start: a group created
/// after the start, or deleted and created again (a new SID), counts at
/// once. The name is the manager's one (`ADMIN_GROUP`, `pai`), the same
/// `Authenticate` checks. The answer counts only through
/// [`local_alias_sid`]: a local alias of this computer.
#[test]
fn the_pipe_looks_the_pai_group_up_by_name_for_each_connection() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/manager/windows/pipe_server.rs"),
    )
    .expect("pipe_server.rs");
    let start = src.find("fn client_facts(").expect("fn client_facts");
    let body = &src[start..];
    let end = body.find("\n}\n").expect("end of client_facts");
    let body = &body[..end];
    assert!(
        src.contains("LookupAccountNameW"),
        "pipe_server.rs never calls LookupAccountNameW"
    );
    assert!(
        body.contains("ADMIN_GROUP"),
        "client_facts does not name the group by ADMIN_GROUP:\n{body}"
    );
    assert!(
        !body.contains("from_groups(&sid, &groups, None)"),
        "client_facts passes no pai SID:\n{body}"
    );
    assert!(
        !src.contains("OnceLock") && !src.contains("LazyLock") && !src.contains("static PAI"),
        "the pai SID is cached"
    );
    let gs = src.find("fn group_sid(").expect("fn group_sid");
    let rest = &src[gs..];
    let gs = &rest[..rest.find("\n}\n").expect("end of group_sid")];
    assert!(
        gs.contains("local_alias_sid("),
        "group_sid hands over the looked-up SID without filtering it \
             through local_alias_sid:\n{gs}"
    );
}

/// A token holding `pai` enabled (a local group in a user's token:
/// mandatory, enabled by default, enabled) is a member, at the console
/// or not; nothing else changes.
#[test]
fn a_token_holding_pai_is_a_pai_member() {
    let mut groups = common();
    groups.push(group(PAI, 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(false, false, true)
    );
    groups.push(group(SID_INTERACTIVE, 0x7));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(true, false, true)
    );
}

/// A deny-only `pai` (a restricted token) grants nothing, as a deny-only
/// INTERACTIVE grants nothing.
#[test]
fn a_deny_only_pai_group_does_not_count() {
    let mut groups = common();
    groups.push(group(PAI, SE_GROUP_USE_FOR_DENY_ONLY));
    assert_eq!(
        TokenFacts::from_groups(USER, &groups, Some(PAI)),
        facts(false, false, false)
    );
    let mut groups = common();
    groups.push(group(PAI, 0x7 | SE_GROUP_USE_FOR_DENY_ONLY));
    assert!(!TokenFacts::from_groups(USER, &groups, Some(PAI)).pai_member);
}

/// No group `pai` on the machine (deleted, or the lookup failed): nobody
/// is a member, whatever the token holds.
#[test]
fn no_pai_group_makes_nobody_a_member() {
    let mut groups = common();
    groups.push(group(PAI, 0x7));
    groups.push(group(SID_ADMINISTRATORS, 0xF));
    let f = TokenFacts::from_groups(USER, &groups, None);
    assert!(!f.pai_member, "{f:?}");
    assert!(!allowed(&f, Action::Configure), "{f:?}");
}

/// The SID is compared whole: a group whose SID starts like `pai`'s, or
/// another local group of the same machine, is not `pai`; nor is the
/// Administrators alias.
#[test]
fn a_look_alike_sid_is_not_pai() {
    let mut groups = common();
    groups.push(group(&format!("{PAI}0"), 0x7));
    groups.push(group("S-1-5-21-1004336348-1177238915-682003330-1010", 0x7));
    groups.push(group(SID_ADMINISTRATORS, 0xF));
    let f = TokenFacts::from_groups(USER, &groups, Some(PAI));
    assert!(!f.pai_member, "{f:?}");
    assert!(!allowed(&f, Action::Configure), "{f:?}");
}

#[test]
fn nobody_may_do_anything() {
    for action in [Action::Control, Action::Configure] {
        assert!(!allowed(&facts(false, false, false), action), "{action:?}");
    }
}

#[test]
fn a_standard_user_controls_but_never_configures() {
    assert!(allowed(&facts(true, false, false), Action::Control));
    assert!(!allowed(&facts(true, false, false), Action::Configure));
}

/// An administrator still controls, but configures only as a member of
/// `pai` (step 10 slice 6b part 2b): Administrators alone, filtered or
/// elevated, no longer configures.
#[test]
fn an_admin_member_controls_but_configures_only_in_pai() {
    for interactive in [true, false] {
        let f = facts(interactive, true, false);
        assert!(allowed(&f, Action::Control), "{f:?}");
        assert!(!allowed(&f, Action::Configure), "{f:?}");
        let f = facts(interactive, true, true);
        assert!(allowed(&f, Action::Configure), "{f:?}");
    }
}

/// A member of `pai` configures, at the console or not, administrator
/// or not; configuring includes controlling.
#[test]
fn a_pai_member_configures_and_controls() {
    for (interactive, admin) in [(true, false), (false, false), (true, true), (false, true)] {
        let f = facts(interactive, admin, true);
        assert!(allowed(&f, Action::Configure), "{f:?}");
        assert!(allowed(&f, Action::Control), "{f:?}");
    }
}

/// The real tokens end to end: a standard user at the console added to
/// `pai` configures; the owner of the PC (filtered or elevated) who is
/// not in `pai` does not.
#[test]
fn configure_follows_the_pai_group_in_the_token() {
    let mut member = common();
    member.push(group(SID_INTERACTIVE, 0x7));
    member.push(group(PAI, 0x7));
    let member = TokenFacts::from_groups(USER, &member, Some(PAI));
    assert!(allowed(&member, Action::Configure), "{member:?}");
    for admin in [SE_GROUP_USE_FOR_DENY_ONLY, 0xF] {
        let mut owner = common();
        owner.push(group(SID_INTERACTIVE, 0x7));
        owner.push(group(SID_ADMINISTRATORS, admin));
        let owner = TokenFacts::from_groups(USER, &owner, Some(PAI));
        assert!(allowed(&owner, Action::Control), "{owner:?}");
        assert!(!allowed(&owner, Action::Configure), "{owner:?}");
    }
}

/// LocalSystem (the manager's own account, the MSI's deferred custom
/// actions) configures as it did through Administrators, like root on
/// Linux; its token is not in `pai`.
#[test]
fn local_system_still_configures() {
    let mut groups = common();
    groups.push(group(SID_ADMINISTRATORS, 0xE)); // enabled by default, enabled, owner
    let system = TokenFacts::from_groups(SID_LOCAL_SYSTEM, &groups, Some(PAI));
    assert!(!system.pai_member, "{system:?}");
    assert!(allowed(&system, Action::Configure), "{system:?}");
    assert!(allowed(&system, Action::Control), "{system:?}");
    let system = TokenFacts::from_groups(SID_LOCAL_SYSTEM, &groups, None);
    assert!(
        allowed(&system, Action::Configure),
        "no pai group: {system:?}"
    );
}

/// Only the SID itself is LocalSystem: a user whose token merely holds
/// SYSTEM as a group (or a look-alike SID) is not.
#[test]
fn only_the_local_system_user_sid_is_local_system() {
    assert_eq!(SID_LOCAL_SYSTEM, "S-1-5-18");
    let mut groups = common();
    groups.push(group(SID_LOCAL_SYSTEM, 0x7));
    let f = TokenFacts::from_groups(USER, &groups, Some(PAI));
    assert!(!allowed(&f, Action::Configure), "{f:?}");
    let f = TokenFacts::from_groups("S-1-5-180", &common(), Some(PAI));
    assert!(!allowed(&f, Action::Configure), "{f:?}");
}

#[test]
fn no_connection_on_this_thread_is_refused() {
    for action in [Action::Control, Action::Configure] {
        assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), action));
    }
}

#[test]
fn the_connection_s_caller_is_its_sid_and_decides() {
    let (control, configure) = TokenAuthority::with_caller(facts(true, false, false), |caller| {
        assert_eq!(caller, &Caller(USER.into()));
        (
            TokenAuthority.is_authorized(caller, Action::Control),
            TokenAuthority.is_authorized(caller, Action::Configure),
        )
    });
    assert_eq!((control, configure), (true, false));
}

/// Windows never prompts: the quiet question answers as the plain one,
/// from the token (configure is a member of `pai`, not of
/// Administrators), and a caller other than the connection's, or no
/// connection, is nobody.
#[test]
fn the_quiet_question_answers_from_the_token_as_the_plain_one() {
    for (interactive, admin, pai) in [
        (true, false, false),
        (true, true, false),
        (false, true, false),
        (false, false, false),
        (true, false, true),
        (false, false, true),
        (true, true, true),
    ] {
        let f = facts(interactive, admin, pai);
        let configure = TokenAuthority::with_caller(f.clone(), |caller| {
            TokenAuthority.is_authorized_quietly(caller, Action::Configure)
        });
        assert_eq!(configure, pai, "quiet configure is pai membership: {f:?}");
        let (quiet, plain) = TokenAuthority::with_caller(f.clone(), |caller| {
            let q = [Action::Control, Action::Configure]
                .map(|a| TokenAuthority.is_authorized_quietly(caller, a));
            let p = [Action::Control, Action::Configure]
                .map(|a| TokenAuthority.is_authorized(caller, a));
            (q, p)
        });
        assert_eq!(quiet, plain, "{f:?}");
    }
    let other = Caller("S-1-5-21-1-2-3-500".into());
    let refused = TokenAuthority::with_caller(facts(true, true, true), |_| {
        !TokenAuthority.is_authorized_quietly(&other, Action::Configure)
    });
    assert!(refused, "another caller than the connection's");
    assert!(
        !TokenAuthority.is_authorized_quietly(&Caller(USER.into()), Action::Configure),
        "no connection on this thread"
    );
}

/// Another caller than the connection's (a stale or forged one) is
/// nobody.
#[test]
fn another_caller_than_the_connection_s_is_refused() {
    let other = Caller("S-1-5-21-1-2-3-500".into());
    let refused = TokenAuthority::with_caller(facts(true, true, false), |_| {
        !TokenAuthority.is_authorized(&other, Action::Control)
            && !TokenAuthority.is_authorized(&Caller(String::new()), Action::Control)
    });
    assert!(refused);
}

#[test]
fn the_connection_is_cleared_when_it_ends() {
    TokenAuthority::with_caller(facts(true, true, false), |_| {});
    assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Control));
    let panicked = std::panic::catch_unwind(|| {
        TokenAuthority::with_caller(facts(true, true, false), |_| panic!("in the call"))
    });
    assert!(panicked.is_err());
    assert!(!TokenAuthority.is_authorized(&Caller(USER.into()), Action::Configure));
}

/// Each connection thread sees its own caller only.
#[test]
fn connections_on_other_threads_are_apart() {
    TokenAuthority::with_caller(facts(true, true, false), |caller| {
        let caller = caller.clone();
        let seen =
            std::thread::spawn(move || TokenAuthority.is_authorized(&caller, Action::Configure))
                .join()
                .expect("thread");
        assert!(!seen, "another thread has no connection");
    });
}

// ── The SCM service of an instance ──────────────────────────────────

#[test]
fn the_manager_s_own_service() {
    assert_eq!(MANAGER_SERVICE, "private-ai-manager");
}

#[test]
fn invalid_ids_have_no_service() {
    for bad in ["", "Default", "a/b", "a\\b", "../x", "a@b", "a b"] {
        assert_eq!(service_name(bad), None, "{bad:?}");
        assert_eq!(service_spec(bad, Path::new("pai-manager.exe")), None);
    }
}

#[test]
fn the_service_is_named_after_the_instance_with_an_underscore() {
    assert_eq!(
        service_name("default").as_deref(),
        Some("llama.cpp_default")
    );
    assert_eq!(service_name("lan-2").as_deref(), Some("llama.cpp_lan-2"));
}

#[test]
fn foreign_services_are_no_instance() {
    for name in [
        "",
        "llama.cpp",
        "llama.cpp_",
        "llama.cpp__",
        "llama.cpp__default",
        "llama.cpp_a_b",
        "llama.cpp_a@b",
        "llama.cpp-default",
        "xllama.cpp_default",
        // The Linux unit's name is not the Windows service's.
        "llama.cpp@default",
        "llama.cpp@lan-2",
        "private-ai-manager",
        "Spooler",
    ] {
        assert_eq!(instance_of_service(name), None, "{name:?}");
    }
}

/// No id holds a `_` (`paths::valid_instance_id`) and the prefix is
/// fixed, so every service name reads back as the one id it was made of.
#[test]
fn the_service_name_reads_back_as_its_id() {
    for id in ["default", "lan-2", "8080", "a", "0-1-2", "x-"] {
        let name = service_name(id).expect("a valid id");
        assert_eq!(instance_of_service(&name).as_deref(), Some(id), "{name}");
    }
}

#[test]
fn our_services_name_their_instance_in_any_case() {
    assert_eq!(
        instance_of_service("llama.cpp_default").as_deref(),
        Some("default")
    );
    // The SCM compares names case-insensitively: a service a user
    // created as LLAMA.CPP_LAN-2 is the one of instance lan-2.
    assert_eq!(
        instance_of_service("LLAMA.CPP_LAN-2").as_deref(),
        Some("lan-2")
    );
}

const EXE: &str = r"C:\Program Files\private-ai\bin\pai-manager.exe";

#[test]
fn the_service_runs_run_instance_on_demand_as_its_virtual_account() {
    let spec = service_spec("lan-2", Path::new(EXE)).expect("a valid id");
    assert_eq!(
        spec,
        ServiceSpec {
            name: "llama.cpp_lan-2".into(),
            display_name: "llama.cpp server lan-2 (Private AI Framework)".into(),
            executable: PathBuf::from(EXE),
            arguments: vec!["run-instance".into(), "lan-2".into()],
            account: r"NT SERVICE\llama.cpp_lan-2".into(),
            service_type: SERVICE_WIN32_OWN_PROCESS,
            start_type: SERVICE_DEMAND_START,
            preshutdown_timeout_ms: PRESHUTDOWN_TIMEOUT.as_millis() as u32,
        }
    );
}

/// The arguments are the ones `parse_args` reads back.
#[test]
fn the_service_s_arguments_parse_as_run_instance() {
    let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
    assert_eq!(
        crate::manager::parse_args(&spec.arguments),
        Ok(crate::manager::ManagerCommand::RunInstance(
            "default".into()
        ))
    );
}

/// CreateServiceW: a path with a space must be quoted.
#[test]
fn the_image_path_quotes_the_executable() {
    let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
    assert_eq!(
        image_path(&spec),
        r#""C:\Program Files\private-ai\bin\pai-manager.exe" run-instance default"#
    );
    let spec = service_spec("lan-2", Path::new(r"D:\pai\pai-manager.exe")).expect("id");
    assert_eq!(
        image_path(&spec),
        r#""D:\pai\pai-manager.exe" run-instance lan-2"#
    );
}

/// The preshutdown timeout carried into the spec, in milliseconds:
/// `ChangeServiceConfig2` with `SERVICE_CONFIG_PRESHUTDOWN_INFO` is
/// given `dwPreshutdownTimeout` in milliseconds
/// (learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_preshutdown_info),
/// the design's 300 s (ARCHITECTURE.md, "Budget"). The delegation to
/// `PRESHUTDOWN_TIMEOUT` is pinned by the full spec comparison above.
#[test]
fn the_service_spec_carries_the_preshutdown_timeout() {
    let spec = service_spec("default", Path::new(EXE)).expect("a valid id");
    assert_eq!(spec.preshutdown_timeout_ms, 300_000);
}

// ── SCM states and errors ────────────────────────────────────────────

const STOPPED: u32 = 1;
const START_PENDING: u32 = 2;
const STOP_PENDING: u32 = 3;
const RUNNING: u32 = 4;
const CONTINUE_PENDING: u32 = 5;
const PAUSE_PENDING: u32 = 6;
const PAUSED: u32 = 7;
const ERROR_SERVICE_SPECIFIC_ERROR: u32 = 1066;

#[test]
fn a_service_that_stopped_with_an_error_failed() {
    assert_eq!(unit_state(STOPPED, 1067, 0), UnitState::Failed);
    assert_eq!(
        unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 1),
        UnitState::Failed
    );
    assert_eq!(
        unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 0xC000_0005),
        UnitState::Failed
    );
}

#[test]
fn a_service_that_stopped_cleanly_is_inactive() {
    assert_eq!(unit_state(STOPPED, 0, 0), UnitState::Inactive);
    assert_eq!(
        unit_state(STOPPED, 0, 7),
        UnitState::Inactive,
        "ignored without 1066"
    );
    assert_eq!(
        unit_state(STOPPED, ERROR_SERVICE_SPECIFIC_ERROR, 0),
        UnitState::Inactive
    );
}

/// The manager creates an instance's service at its start without
/// starting it; until a start since the last boot, the SCM reports it
/// stopped with ERROR_SERVICE_NEVER_STARTED (1077, "No attempts to start
/// the service have been made since the last boot"): nothing failed.
#[test]
fn a_service_never_started_is_inactive() {
    const ERROR_SERVICE_NEVER_STARTED: u32 = 1077;
    assert_eq!(
        unit_state(STOPPED, ERROR_SERVICE_NEVER_STARTED, 0),
        UnitState::Inactive
    );
}

#[test]
fn scm_states_map_to_unit_states() {
    assert_eq!(unit_state(START_PENDING, 0, 0), UnitState::Activating);
    assert_eq!(unit_state(CONTINUE_PENDING, 0, 0), UnitState::Activating);
    assert_eq!(unit_state(RUNNING, 0, 0), UnitState::Active);
    assert_eq!(unit_state(PAUSED, 0, 0), UnitState::Active);
    assert_eq!(unit_state(STOP_PENDING, 0, 0), UnitState::Deactivating);
    assert_eq!(unit_state(PAUSE_PENDING, 0, 0), UnitState::Deactivating);
    // The exit code is only read once stopped.
    assert_eq!(unit_state(RUNNING, 1067, 3), UnitState::Active);
}

#[test]
fn an_unknown_scm_state_reads_as_inactive() {
    for state in [0, 8, u32::MAX] {
        assert_eq!(unit_state(state, 0, 0), UnitState::Inactive, "{state}");
    }
}

#[test]
fn a_requested_stop_is_a_clean_stop() {
    for child in [Some(0), Some(1), Some(-1), None] {
        assert_eq!(exit_report(true, child), (0, 0), "{child:?}");
    }
}

#[test]
fn llama_server_exiting_by_itself_reports_its_code() {
    assert_eq!(exit_report(false, Some(0)), (0, 0));
    assert_eq!(
        exit_report(false, Some(1)),
        (ERROR_SERVICE_SPECIFIC_ERROR, 1)
    );
    // An NTSTATUS crash code (access violation) is kept bit for bit.
    assert_eq!(
        exit_report(false, Some(0xC000_0005_u32 as i32)),
        (ERROR_SERVICE_SPECIFIC_ERROR, 0xC000_0005)
    );
    // It never ran: a failure, with no code of its own.
    assert_eq!(exit_report(false, None), (ERROR_SERVICE_SPECIFIC_ERROR, 1));
}

/// What run-instance reports is what Status reads back.
#[test]
fn the_exit_report_reads_back_as_the_unit_state() {
    let state = |requested, child| {
        let (win32, specific) = exit_report(requested, child);
        unit_state(STOPPED, win32, specific)
    };
    assert_eq!(state(false, Some(1)), UnitState::Failed);
    assert_eq!(state(false, None), UnitState::Failed);
    assert_eq!(state(false, Some(0)), UnitState::Inactive);
    assert_eq!(state(true, Some(1)), UnitState::Inactive);
}

/// A failure of the SCM itself names no instance's service: no
/// `llama.cpp_` with an empty id, the Win32 error as `scm_error` names it.
#[test]
fn an_scm_wide_error_names_no_instance_service() {
    for (code, name) in [
        (5, "ERROR_ACCESS_DENIED"),
        (1053, "ERROR_SERVICE_REQUEST_TIMEOUT"),
        (1072, "ERROR_SERVICE_MARKED_FOR_DELETE"),
    ] {
        assert_eq!(
            scm_wide_error(code),
            ServiceError(format!("service control manager: {name} ({code})"))
        );
    }
    assert_eq!(
        scm_wide_error(87),
        ServiceError("service control manager: Win32 error 87".into())
    );
    assert!(!scm_wide_error(5).0.contains(SERVICE_PREFIX));
}

/// The code of `scm.rs` (`cfg(windows)`, only type-checked on Linux),
/// whole-line comments left out.
fn scm_code() -> String {
    include_str!("windows/scm.rs")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `sync` reports a failure of the SCM itself as its own kind, never as
/// the failure of an instance with an empty id ("instance ``").
#[test]
fn sync_reports_an_scm_failure_apart_from_the_instances() {
    let code = scm_code();
    assert!(
        !code.contains("scm_error(\"\""),
        "an SCM failure as an instance with an empty id"
    );
    assert!(!code.contains("(String::new(), "), "an empty instance id");
    assert!(
        code.contains("-> Vec<SyncFailure>"),
        "sync returns its failures as SyncFailure"
    );
    assert!(
        code.matches("SyncFailure::Scm(scm_wide_error(").count() >= 2,
        "opening the SCM and listing its services are SCM-wide failures"
    );
    assert!(code.contains("SyncFailure::Instance("));
}

/// `serve`'s doc says what the manager reports at its start: the
/// services it could not set up; it starts no instance (the SCM starts
/// the enabled ones).
#[test]
fn serve_s_doc_says_set_up_not_started() {
    let entry = include_str!("windows/entry.rs");
    let at = entry.find("pub fn serve(").expect("serve");
    let doc: Vec<&str> = entry[..at]
        .lines()
        .rev()
        .take_while(|l| l.trim_start().starts_with("///"))
        .collect();
    assert!(!doc.is_empty(), "serve has no doc comment");
    let doc = doc.into_iter().rev().collect::<Vec<_>>().join("\n");
    assert!(!doc.contains("started"), "{doc}");
    assert!(doc.contains("set up"), "{doc}");
}

#[test]
fn scm_errors_name_the_service_and_the_error() {
    for (code, name) in [
        (5, "ERROR_ACCESS_DENIED"),
        (1053, "ERROR_SERVICE_REQUEST_TIMEOUT"),
        (1058, "ERROR_SERVICE_DISABLED"),
        (1060, "ERROR_SERVICE_DOES_NOT_EXIST"),
        (1064, "ERROR_EXCEPTION_IN_SERVICE"),
        (1069, "ERROR_SERVICE_LOGON_FAILED"),
        (1072, "ERROR_SERVICE_MARKED_FOR_DELETE"),
    ] {
        assert_eq!(
            scm_error("lan-2", code),
            ServiceError(format!("llama.cpp_lan-2: {name} ({code})"))
        );
    }
    assert_eq!(
        scm_error("default", 87),
        ServiceError("llama.cpp_default: Win32 error 87".into())
    );
}

#[test]
fn start_failures_are_errors_but_already_running() {
    for code in [5, 1053, 1058, 1060, 1069, 1072] {
        assert_eq!(
            start_failure("default", code),
            Err(scm_error("default", code))
        );
    }
    assert_eq!(start_failure("default", 1056), Ok(()));
}

#[test]
fn stop_failures_are_errors_but_not_running_or_no_service() {
    for code in [5, 1053, 1061] {
        assert_eq!(
            stop_failure("default", code),
            Err(scm_error("default", code))
        );
    }
    assert_eq!(stop_failure("default", 1062), Ok(()));
    assert_eq!(stop_failure("default", 1060), Ok(()));
}

#[test]
fn status_failures_are_errors_but_no_service() {
    for code in [5, 6, 1072] {
        assert_eq!(
            status_failure("default", code),
            Err(scm_error("default", code))
        );
    }
    assert_eq!(status_failure("default", 1060), Ok(UnitState::Inactive));
}

// ── The services for the instances on disk ──────────────────────────

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn nothing_on_disk_and_no_service_plans_nothing() {
    assert_eq!(service_plan(&[], &[]), ServicePlan::default());
    let foreign = ids(&[
        "Spooler",
        "private-ai-manager",
        "llama.cpp",
        "llama.cpp_Bad_id",
    ]);
    assert_eq!(service_plan(&[], &foreign), ServicePlan::default());
}

/// What `uninstall-services` applies (`ScmService::sync` with no
/// instance): every instance's service goes, whatever its case, and
/// nobody else's, the manager's own included (the MSI removes that one).
#[test]
fn with_no_instance_every_instance_service_is_deleted() {
    let services = ids(&[
        "Spooler",
        "private-ai-manager",
        "llama.cpp_default",
        "LLAMA.CPP_LAN-2",
        "llama.cpp",
        "llama.cpp_Bad_id",
    ]);
    assert_eq!(
        service_plan(&[], &services),
        ServicePlan {
            delete: ids(&["default", "lan-2"]),
            ..ServicePlan::default()
        }
    );
}

#[test]
fn a_new_instance_gets_its_service() {
    let plan = service_plan(&ids(&["default", "lan-2"]), &ids(&["Spooler"]));
    assert_eq!(
        plan,
        ServicePlan {
            create: ids(&["default", "lan-2"]),
            ..ServicePlan::default()
        }
    );
}

#[test]
fn a_removed_instance_loses_its_service() {
    let plan = service_plan(
        &ids(&["default"]),
        &ids(&["llama.cpp_default", "llama.cpp_old", "Spooler"]),
    );
    assert_eq!(
        plan,
        ServicePlan {
            keep: ids(&["default"]),
            delete: ids(&["old"]),
            ..ServicePlan::default()
        }
    );
}

#[test]
fn a_service_in_another_case_is_the_instance_s() {
    let plan = service_plan(&ids(&["lan-2"]), &ids(&["LLAMA.CPP_LAN-2"]));
    assert_eq!(
        plan,
        ServicePlan {
            keep: ids(&["lan-2"]),
            ..ServicePlan::default()
        }
    );
}

/// A crafted id that reached the list is never turned into a service.
#[test]
fn an_invalid_id_on_disk_is_never_created() {
    let plan = service_plan(&ids(&["default", "../x", "Upper"]), &[]);
    assert_eq!(plan.create, ids(&["default"]));
}

// ── The instance's log ───────────────────────────────────────────────

#[test]
fn the_previous_log_sits_beside_the_log() {
    let log = Path::new("logs").join("llama-server@lan-2.log");
    assert_eq!(
        previous_log(&log),
        Path::new("logs").join("llama-server@lan-2.previous.log")
    );
}

#[test]
fn a_log_whose_directory_cannot_exist_fails() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("logs");
    std::fs::write(&file, "a file, not a directory").expect("fixture");
    let log = file.join("llama-server@default.log");
    assert!(open_instance_log(&log).is_err());
    assert_eq!(
        std::fs::read_to_string(&file).expect("still there"),
        "a file, not a directory"
    );
}

/// A directory the service cannot write: the error, and the logs of the
/// runs before untouched.
#[cfg(unix)]
#[test]
fn an_unwritable_log_directory_fails_and_keeps_both_logs() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let logs = dir.path().join("logs");
    std::fs::create_dir(&logs).expect("logs");
    let log = logs.join("llama-server@default.log");
    std::fs::write(&log, "last run\n").expect("log");
    std::fs::write(previous_log(&log), "run before\n").expect("previous");
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    let probe = std::fs::write(logs.join("probe"), "");
    let result = open_instance_log(&log);
    std::fs::set_permissions(&logs, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    if probe.is_ok() {
        eprintln!("skipped: running as root, the mode does not stop a write");
        return;
    }
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&log).expect("log"), "last run\n");
    assert_eq!(
        std::fs::read_to_string(previous_log(&log)).expect("previous"),
        "run before\n"
    );
    let mut names: Vec<_> = std::fs::read_dir(&logs)
        .expect("list")
        .map(|e| e.expect("entry").file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "llama-server@default.log",
            "llama-server@default.previous.log"
        ]
    );
}

#[test]
fn the_first_run_creates_the_directory_and_an_empty_log() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("logs").join("llama-server@default.log");
    let mut file = open_instance_log(&log).expect("opened");
    assert_eq!(std::fs::read(&log).expect("created"), b"");
    assert!(!previous_log(&log).exists());
    file.write_all(b"build: 1234\n").expect("write");
    drop(file);
    assert_eq!(std::fs::read_to_string(&log).expect("log"), "build: 1234\n");
}

#[test]
fn the_last_run_becomes_the_previous_one() {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("llama-server@default.log");
    std::fs::write(&log, "last run\n").expect("log");
    std::fs::write(previous_log(&log), "run before\n").expect("previous");
    let mut file = open_instance_log(&log).expect("opened");
    file.write_all(b"this run\n").expect("write");
    drop(file);
    assert_eq!(std::fs::read_to_string(&log).expect("log"), "this run\n");
    assert_eq!(
        std::fs::read_to_string(previous_log(&log)).expect("previous"),
        "last run\n"
    );
}

/// A run that wrote nothing (a start that failed at once) does not push
/// the useful previous log out.
#[test]
fn an_empty_last_run_keeps_the_previous_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let log = dir.path().join("llama-server@default.log");
    std::fs::write(&log, "").expect("log");
    std::fs::write(previous_log(&log), "the crash\n").expect("previous");
    drop(open_instance_log(&log).expect("opened"));
    assert_eq!(std::fs::read(&log).expect("log"), b"");
    assert_eq!(
        std::fs::read_to_string(previous_log(&log)).expect("previous"),
        "the crash\n"
    );
}

// ── Saving snapshots at the instance's own stop ─────────────────────
// The instance service (`run-instance`) writes the slot snapshots at
// its own stop, on a Stop and on a Preshutdown, before killing
// llama-server. The decisions are OS-neutral here; the Win32 glue
// (windows/entry.rs) is type-checked only, so its call order is pinned
// from its text, narrowly.

/// Arguments as `Command::get_args` yields them, collected.
fn os_args(v: &[&str]) -> Vec<std::ffi::OsString> {
    v.iter().map(std::ffi::OsString::from).collect()
}

/// The arguments the instance spawned llama-server with are the save's
/// target, through the core's `slot_target` (lossy UTF-8): the port and
/// the folder the running engine uses, never a port or folder read out
/// of `server.ini`.
#[test]
fn the_spawned_arguments_give_the_snapshot_target() {
    let a = os_args(&[
        "--model",
        "/models/qwen.gguf",
        "--slot-save-path",
        r"C:\state\default",
        "--port",
        "8081",
    ]);
    assert_eq!(
        instance_save_target(&a),
        Some((8081, PathBuf::from(r"C:\state\default")))
    );
}

/// No `--slot-save-path` means the snapshots were off at the launch,
/// whatever `server.ini` says now: no target, no save. A flag with
/// nothing usable after it (end of the list, empty, a flag) names no
/// folder either.
#[test]
fn a_launch_without_a_snapshot_folder_gives_no_target() {
    for a in [
        os_args(&[]),
        os_args(&["--model", "/models/qwen.gguf", "--port", "8081"]),
        os_args(&["--slot-save-path"]),
        os_args(&["--slot-save-path", ""]),
        os_args(&["--slot-save-path", "--model"]),
    ] {
        assert_eq!(instance_save_target(&a), None, "{a:?}");
    }
}

/// No `--port`: the engine's own default, the same reading
/// `slot_target` gives, never the file's.
#[test]
fn a_launch_without_a_port_gives_the_default_port() {
    let a = os_args(&["--slot-save-path", r"C:\state\default"]);
    assert_eq!(
        instance_save_target(&a),
        Some((
            pai_core::server_cfg::DEFAULT_PORT as u16,
            PathBuf::from(r"C:\state\default")
        ))
    );
}

/// An argument that is not valid UTF-8 (an `OsString` is what the
/// spawn's command line carries): the target is taken through lossy
/// UTF-8, the replacement character standing for the unreadable bytes,
/// so the folder the engine writes to is read as close as Rust gets.
#[cfg(unix)]
#[test]
fn the_folder_of_a_lossy_argument_survives_the_conversion() {
    use std::os::unix::ffi::OsStringExt;
    let dir = OsStringExt::from_vec(b"C:\\state\\bin\xff".to_vec());
    let a = vec![std::ffi::OsString::from("--slot-save-path"), dir];
    assert_eq!(
        instance_save_target(&a),
        Some((
            pai_core::server_cfg::DEFAULT_PORT as u16,
            PathBuf::from("C:\\state\\bin\u{FFFD}")
        ))
    );
}

/// The `dwControlsAccepted` bits and the control codes, from
/// SERVICE_STATUS and ServiceCtrlAccepted
/// (learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status):
/// stop is 0x1, shutdown 0x4 and 0x5, interrogate 0x4, preshutdown
/// 0x100 and 0xF.
#[test]
fn the_control_accept_and_control_codes_are_the_windows_ones() {
    assert_eq!(SERVICE_ACCEPT_STOP, 0x1);
    assert_eq!(SERVICE_ACCEPT_SHUTDOWN, 0x4);
    assert_eq!(SERVICE_ACCEPT_PRESHUTDOWN, 0x100);
    assert_eq!(SERVICE_CONTROL_STOP, 0x1);
    assert_eq!(SERVICE_CONTROL_INTERROGATE, 0x4);
    assert_eq!(SERVICE_CONTROL_SHUTDOWN, 0x5);
    assert_eq!(SERVICE_CONTROL_PRESHUTDOWN, 0xF);
}

/// A running instance answers a Stop and a Preshutdown, and nothing
/// else. Plain shutdown (`SERVICE_ACCEPT_SHUTDOWN`) is not accepted:
/// the SCM gives a shutdown about 20 s, too short for a snapshot write,
/// so the reboot path is the preshutdown, bounded by the service's own
/// timeout (HandlerEx Remarks,
/// learn.microsoft.com/en-us/windows/win32/api/winsvc/nc-winsvc-lphandler_function_ex).
#[test]
fn a_running_instance_accepts_stop_and_preshutdown_only() {
    assert_eq!(controls_accepted(true), 0x1 | 0x100);
    assert_eq!(
        controls_accepted(true),
        SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_PRESHUTDOWN
    );
    assert_eq!(
        controls_accepted(true) & SERVICE_ACCEPT_SHUTDOWN,
        0,
        "shutdown is not the save path"
    );
}

/// A service that is not running accepts no control: a stop asked of a
/// stopped service is nothing to save.
#[test]
fn an_instance_that_is_not_running_accepts_nothing() {
    assert_eq!(controls_accepted(false), 0);
}

/// The two controls that end the instance's run, and so must save
/// first: the SCM's Stop, and the Preshutdown of a system shutdown.
#[test]
fn stop_and_preshutdown_end_the_instance() {
    for control in [SERVICE_CONTROL_STOP, SERVICE_CONTROL_PRESHUTDOWN] {
        assert!(ends_instance(control), "{control}");
    }
}

/// Nothing else: Interrogate is a status poll, Shutdown is not the save
/// path, and the rest the service does not implement.
#[test]
fn no_other_control_ends_the_instance() {
    for control in [
        SERVICE_CONTROL_INTERROGATE,
        SERVICE_CONTROL_SHUTDOWN,
        0x0,
        0x2,
        0x3,
        0x6,
        0x7,
        0x8,
        0x9,
        0xA,
        0xB,
        0xC,
        0x10,
        0x20,
        u32::MAX,
    ] {
        assert!(!ends_instance(control), "{control}");
    }
}

/// The manager's own service is not the save path: running, it accepts
/// the Stop (0x1) and nothing else. The masks are pinned as literals:
/// SERVICE_ACCEPT_PRESHUTDOWN is 0x100 and SERVICE_ACCEPT_SHUTDOWN 0x4
/// (SERVICE_STATUS,
/// learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_status),
/// so this test does not lean on the constants of this file.
#[test]
fn the_manager_service_accepts_the_stop_only() {
    assert_eq!(manager_controls_accepted(true), 0x1, "stop only");
    assert_eq!(
        manager_controls_accepted(true) & 0x100,
        0,
        "the preshutdown is the instances' path"
    );
    assert_eq!(
        manager_controls_accepted(true) & 0x4,
        0,
        "the shutdown notification is not the save path"
    );
    assert_eq!(
        manager_controls_accepted(true) & !controls_accepted(true),
        0,
        "the manager accepts no control an instance does not accept"
    );
}

/// A manager service that is not running accepts no control: a stop
/// asked of a stopped service is nothing to save.
#[test]
fn a_manager_that_is_not_running_accepts_nothing() {
    assert_eq!(manager_controls_accepted(false), 0);
}

/// Only the Stop ends the manager's run. The Preshutdown is never asked
/// of it, so a reboot cannot take the manager down while the instances
/// are saving; the Shutdown notification and the Interrogate poll are
/// not a stop either.
#[test]
fn only_the_stop_ends_the_manager() {
    assert!(ends_manager(SERVICE_CONTROL_STOP), "stop");
    for control in [
        SERVICE_CONTROL_PRESHUTDOWN,
        SERVICE_CONTROL_SHUTDOWN,
        SERVICE_CONTROL_INTERROGATE,
        0x0,
        0x2,
        0x3,
        0x6,
        0x7,
        0x8,
        0x9,
        0xA,
        0xB,
        0xC,
        0x10,
        0x20,
        u32::MAX,
    ] {
        assert!(!ends_manager(control), "{control}");
    }
}

/// The two services part at exactly the Preshutdown: both end on a Stop
/// and accept it; an instance also ends on a Preshutdown and accepts it,
/// the manager does neither.
#[test]
fn the_preshutdown_belongs_to_the_instances_not_the_manager() {
    assert!(ends_manager(SERVICE_CONTROL_STOP), "the manager stops");
    assert!(ends_instance(SERVICE_CONTROL_STOP), "the instance stops");
    assert!(
        ends_instance(SERVICE_CONTROL_PRESHUTDOWN),
        "an instance saves at a reboot"
    );
    assert!(
        !ends_manager(SERVICE_CONTROL_PRESHUTDOWN),
        "the manager must stay up while the instances save"
    );
    assert_eq!(
        controls_accepted(true) & SERVICE_ACCEPT_PRESHUTDOWN,
        SERVICE_ACCEPT_PRESHUTDOWN,
        "an instance accepts the preshutdown"
    );
    assert_eq!(
        manager_controls_accepted(true) & SERVICE_ACCEPT_PRESHUTDOWN,
        0,
        "the manager does not accept it"
    );
}

/// While the save runs the service reports `StopPending` with a wait
/// hint and raises `dwCheckPoint` within every hint it announced, or
/// the SCM may take the service for failed (SERVICE_STATUS docs). So
/// the tick, the moment of a new checkpoint, sits well inside the hint.
#[test]
fn the_stop_wait_hint_gives_time_for_a_checkpoint_every_tick() {
    assert!(
        STOP_TICK > std::time::Duration::ZERO,
        "a zero tick is no progress"
    );
    assert!(
        STOP_TICK * 2 < STOP_WAIT_HINT,
        "a checkpoint every tick must land inside the announced hint: \
             {STOP_TICK:?} vs {STOP_WAIT_HINT:?}"
    );
}

/// The checkpoint starts at 1 and strictly increases with every tick:
/// a value that stands still reads as a service that stopped making
/// progress.
#[test]
fn the_stop_checkpoint_starts_at_one_and_only_grows() {
    assert_eq!(stop_checkpoint(0), 1);
    let mut last = stop_checkpoint(0);
    for tick in 1..64u32 {
        let cp = stop_checkpoint(tick);
        assert!(cp > last, "tick {tick}: {cp} after {last}");
        last = cp;
    }
}

/// The preshutdown timeout the instance services are created with, the
/// value of `SERVICE_CONFIG_PRESHUTDOWN_INFO`: the design's 300 s, so
/// it outlasts the save's one budget by the minute llama-server needs
/// to exit after it (ARCHITECTURE.md, "Budget").
#[test]
fn the_preshutdown_timeout_leaves_the_server_time_to_exit() {
    assert_eq!(PRESHUTDOWN_TIMEOUT, std::time::Duration::from_secs(300));
    let margin = PRESHUTDOWN_TIMEOUT.checked_sub(pai_core::slot_save::SAVE_BUDGET);
    assert!(
        margin.is_some_and(|m| m >= std::time::Duration::from_secs(60)),
        "PRESHUTDOWN_TIMEOUT {} must exceed SAVE_BUDGET {} by at least 60 s",
        PRESHUTDOWN_TIMEOUT.as_secs(),
        pai_core::slot_save::SAVE_BUDGET.as_secs()
    );
}

/// A restart's delayed start waits for the stop before starting the
/// service again: the stop now saves the snapshots first, so the wait
/// must outlast the save's one budget, and the preshutdown timeout that
/// bounds the same save on the reboot path is its floor
/// (ARCHITECTURE.md, "Budget").
#[test]
fn the_restart_stop_wait_outlasts_the_save_and_the_preshutdown() {
    assert!(
        RESTART_STOP_WAIT > pai_core::slot_save::SAVE_BUDGET,
        "the stop saves first: RESTART_STOP_WAIT {RESTART_STOP_WAIT:?} \
             must exceed SAVE_BUDGET {} s",
        pai_core::slot_save::SAVE_BUDGET.as_secs()
    );
    assert!(
        RESTART_STOP_WAIT >= PRESHUTDOWN_TIMEOUT,
        "the same save is bounded on both paths: RESTART_STOP_WAIT \
             {RESTART_STOP_WAIT:?} must not be shorter than \
             PRESHUTDOWN_TIMEOUT {} s",
        PRESHUTDOWN_TIMEOUT.as_secs()
    );
}

/// The record line of a delayed start that failed: the manager's record
/// shape (`manager/service.rs`), action, target and outcome, every name
/// `{:?}`-quoted. Its own action name, `restart_start`: the manager has
/// already recorded the `restart` call it answered, so a delayed start
/// that fails must not read as a second restart of the same instance.
#[test]
fn the_restart_failure_line_is_one_quoted_record() {
    assert_eq!(
        restart_failure_line("default", "Service"),
        r#"action="restart_start" target="default" outcome="Service""#,
        "the line follows the manager's record shape, values {{:?}}-quoted"
    );
}

/// The ids and failure names this line carries come from the caller and
/// from the SCM: a quote, a newline or a NUL forges no second record.
/// The expectation is Rust's own quoting, formatted in the test.
#[test]
fn the_restart_failure_line_forges_no_second_record() {
    for (id, detail) in [
        (r#"lan"2"#, r#"Io("C:\tmp")"#),
        ("a\nb", "start failed\nagain"),
        ("nul\0id", "NUL\0detail"),
    ] {
        let line = restart_failure_line(id, detail);
        assert_eq!(
            line,
            format!(r#"action="restart_start" target={id:?} outcome={detail:?}"#),
            "the values are Rust-quoted, id {id:?}"
        );
        assert_eq!(line.lines().count(), 1, "one record per line, id {id:?}");
    }
}

// ── A restart a stop cancels ─────────────────────────────────────────
// `restart` answers once the stop is accepted and starts the service
// again from a thread, after the save that stop makes (the wait,
// [`RESTART_STOP_WAIT`]). A Stop of the same instance, or a newer
// Restart, asked while that thread waits must stand: the gate keeps one
// generation per instance, and the delayed start starts only while the
// generation its restart took is still the current one.

/// The gate is asked by the manager's calls and by the thread of a
/// delayed start, on different threads, so it must cross one.
fn gate_is_send_sync<T: Send + Sync>() {}

/// A restart is current until a stop of that instance makes it stale:
/// the user's Stop stands, the delayed start must not start the service
/// again.
#[test]
fn a_restart_is_current_until_a_stop_makes_it_stale() {
    gate_is_send_sync::<RestartGate>();
    let gate = RestartGate::default();
    let generation = gate.begin("default");
    assert!(
        gate.is_current("default", generation),
        "a restart that began, and was not cancelled, is current"
    );
    gate.cancel("default");
    assert!(
        !gate.is_current("default", generation),
        "a stop of the instance makes the restart that is still waiting \
         stale"
    );
}

/// A newer Restart of the same instance replaces the pending one: only
/// the newest generation is current, so at most one delayed start runs
/// for an instance.
#[test]
fn a_newer_restart_makes_the_one_it_replaces_stale() {
    let gate = RestartGate::default();
    let first = gate.begin("default");
    let second = gate.begin("default");
    assert!(
        !gate.is_current("default", first),
        "the restart a newer one replaces is stale"
    );
    assert!(
        gate.is_current("default", second),
        "the newest restart of the instance is current"
    );
}

/// One instance's stop cancels that instance's restarts only: another
/// instance's pending restart stays current until its own stop.
#[test]
fn each_instance_has_its_own_generation() {
    let gate = RestartGate::default();
    let (default, lan2) = (gate.begin("default"), gate.begin("lan-2"));
    gate.cancel("default");
    assert!(!gate.is_current("default", default), "its own stop");
    assert!(
        gate.is_current("lan-2", lan2),
        "another instance's stop cancels nothing"
    );
    gate.cancel("lan-2");
    assert!(!gate.is_current("lan-2", lan2), "until its own stop");
}

/// The gate travels as an `Arc`: the thread of a delayed start asks the
/// same gate the manager's calls answer, so a stop asked on another
/// thread (a caller served on its own connection) is seen by the thread
/// that is waiting.
#[test]
fn the_gate_shared_across_threads_answers_the_same() {
    let gate = std::sync::Arc::new(RestartGate::default());
    let generation = gate.begin("default");
    let seen = {
        let gate = std::sync::Arc::clone(&gate);
        std::thread::spawn(move || gate.is_current("default", generation))
            .join()
            .expect("thread")
    };
    assert!(
        seen,
        "the thread of a delayed start sees its own restart as current"
    );
    {
        let gate = std::sync::Arc::clone(&gate);
        std::thread::spawn(move || gate.cancel("default"))
            .join()
            .expect("thread");
    }
    assert!(
        !gate.is_current("default", generation),
        "a stop asked on another thread reaches the thread that is waiting"
    );
}

/// The glue is type-checked only, so pin its decisions from its text,
/// narrowly: `instance_main` takes the target from the arguments it
/// spawned llama-server with, saves within the core's one budget on the
/// stop the SCM asked for, and only then kills the child. The save is
/// pinned inside the arm for `End::Stop`: the same needles moved into
/// the arm that watches the child exit on its own would keep this test
/// green while a stop never reaches them. The worker thread, the kill
/// itself and the checkpoint reporting are the glue's, not pinned.
#[test]
fn instance_main_saves_the_snapshots_before_it_kills_the_child() {
    let body = entry_fn("instance_main");
    for call in ["instance_save_target(", "save_all(", "SAVE_BUDGET"] {
        assert!(body.contains(call), "instance_main never uses {call}");
    }
    let kill = body
        .find("child.kill()")
        .expect("instance_main keeps killing the child");
    for call in ["instance_save_target(", "save_all(", "SAVE_BUDGET"] {
        assert!(
            body.find(call).expect("the call") < kill,
            "{call} must come before child.kill(): the snapshots first"
        );
    }
    let stop = body
        .find("End::Stop")
        .expect("instance_main keeps the arm for the SCM's stop");
    for call in ["save_all(", "SAVE_BUDGET"] {
        assert!(
            body.find(call).expect("the call") > stop,
            "{call} must sit in the arm for the SCM's stop, not in the arm \
             that watches the child exit: a stop would skip the save"
        );
    }
}

// ── Which service takes which control ───────────────────────────────
// `register` (the control handler) and `set` (the status report) are
// shared by the instance's service and the manager's own, so the rule
// of what ends a run and of what is accepted must arrive from the
// caller: an instance saves the snapshots on a Preshutdown, the manager
// service must not take it at all. The glue is type-checked only, so the
// routing is pinned from its text, whole-line comments left out: a
// comment naming a needle never greens it. It pins names and wiring in
// the source, not behaviour: it cannot catch a parameter the glue
// ignores once it has it, a call through a path these needles do not
// name, or what the SCM really sends.

/// entry.rs with the whole-line comments left out.
fn entry_code() -> String {
    include_str!("windows/entry.rs")
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of `fn <name>` in entry.rs, from its signature to the brace
/// that closes it at column 0, comments left out. The search starts the
/// line, so a `define_windows_service!` of the same name is not it.
fn entry_fn(name: &str) -> String {
    let entry = entry_code();
    let open = format!("\nfn {name}(");
    let at = entry
        .find(&open)
        .unwrap_or_else(|| panic!("entry.rs has no `{open}`"));
    let body = &entry[at..];
    let end = body
        .find("\n}\n")
        .unwrap_or_else(|| panic!("the end of `fn {name}`"));
    body[..end].to_string()
}

/// The arguments of the first call of `name(...)` in `hay`, up to that
/// call's own closing paren: a nested call is counted, not followed.
fn call_arguments(hay: &str, name: &str) -> String {
    let open = format!("{name}(");
    let at = hay
        .find(&open)
        .unwrap_or_else(|| panic!("`{name}(` is never called:\n{hay}"));
    let rest = &hay[at + open.len()..];
    let mut depth = 1usize;
    for (i, c) in rest.char_indices() {
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                return rest[..i].to_string();
            }
        }
    }
    panic!("`{name}(` is never closed:\n{hay}");
}

/// How often `hay` names the instance's accept mask. The manager's own
/// mask is a different name that ends with the instance's, so each of
/// its namings is taken back out.
fn instance_mask_asks(hay: &str) -> usize {
    hay.matches("controls_accepted").count() - hay.matches("manager_controls_accepted").count()
}

/// The shared control handler decides with a function it is given, so
/// the two services can route apart; it must not pick one service's rule
/// itself, and it keeps reading the control code it is handed.
#[test]
fn the_control_handler_decides_with_the_function_it_is_given() {
    let body = entry_fn("register");
    let sig = &body[..body.find('{').expect("the body of register")];
    assert!(
        sig.contains("fn(u32)") || sig.contains("Fn(u32)"),
        "register takes no control -> bool function, so one rule is fixed \
             inside it:\n{sig}"
    );
    assert!(
        sig.contains("-> bool"),
        "register's deciding parameter is not control -> bool:\n{sig}"
    );
    assert!(
        !body.contains("ends_instance(") && !body.contains("ends_manager("),
        "register names a service's rule itself, so the manager's handler \
             takes the instance's preshutdown:\n{body}"
    );
    assert!(
        body.contains("raw_service_control_type()"),
        "the handler no longer reads the control code it is sent:\n{body}"
    );
}

/// Each service main hands the handler its own rule: the instance the
/// one that ends on a Preshutdown, the manager the one that ends on a
/// Stop only. entry.rs imports both names, so the pin is the call, not
/// a naming anywhere in the file.
#[test]
fn each_service_main_registers_its_own_rule() {
    let args = call_arguments(&entry_fn("instance_main"), "register");
    assert!(
        args.contains("ends_instance"),
        "instance_main does not register ends_instance: {args}"
    );
    assert!(
        !args.contains("ends_manager"),
        "instance_main registers the manager's rule, so a reboot skips the \
             snapshots: {args}"
    );
    let args = call_arguments(&entry_fn("serve_main"), "register");
    assert!(
        args.contains("ends_manager"),
        "serve_main does not register ends_manager: {args}"
    );
    assert!(
        !args.contains("ends_instance"),
        "serve_main registers the instance's rule, so the manager stops \
             while the instances save: {args}"
    );
}

/// The accept mask a running service reports is its caller's: `set`
/// takes it (the mask, or the function that computes it) and never
/// chooses one, `serve_main` hands over the manager's and `instance_main`
/// the instance's. The mask parameter of `set` must not be named
/// `controls_accepted`: this test reads that name as the instance's rule.
#[test]
fn the_accept_mask_is_the_caller_s() {
    let set = entry_fn("set");
    let sig = &set[..set.find('{').expect("the body of set")];
    let params = call_arguments(sig, "fn set");
    assert!(
        params.split(',').filter(|p| !p.trim().is_empty()).count() > 4,
        "set takes no accept mask, so it reports one rule for both \
             services: {params}"
    );
    assert!(
        !set.contains("controls_accepted("),
        "set chooses the accept mask itself:\n{set}"
    );
    assert!(
        set.contains("controls_accepted:"),
        "set no longer reports a mask to the SCM:\n{set}"
    );
    let manager = entry_fn("serve_main");
    assert!(
        manager.contains("manager_controls_accepted"),
        "serve_main never names the manager's accept mask"
    );
    assert_eq!(
        instance_mask_asks(&manager),
        0,
        "serve_main reports the instance's accept mask"
    );
    let instance = entry_fn("instance_main");
    assert!(
        instance_mask_asks(&instance) > 0,
        "instance_main never names the instance's accept mask"
    );
    assert!(
        !instance.contains("manager_controls_accepted"),
        "instance_main reports the manager's accept mask"
    );
}

// ── The SCM glue (scm.rs) ────────────────────────────────────────────
// scm.rs is cfg(windows): cross-compiled and checked, never run here, so
// its wiring is pinned from its text, whole-line comments left out by
// `scm_code` (as the entry.rs pins do). Trailing comments are kept:
// a required needle spelled in one greens falsely, and a forbidden one
// named in any comment reds, so the glue must not name these needles in
// comments. The pins name code shapes in the source: they cannot catch
// a value that lands in the wrong argument slot, a closure that borrows
// what it may not (the cross target's own check), the record reaching
// the real log file, or what the SCM does.

/// The text of `fn <name>` in scm.rs, from its signature to the brace
/// that closes it (at column 0 for a top-level fn, at column 4 for an
/// impl method), comments left out.
fn scm_fn(name: &str) -> String {
    let code = scm_code();
    let at = [format!("\nfn {name}("), format!("\n    fn {name}(")]
        .into_iter()
        .find_map(|open| code.find(&open))
        .unwrap_or_else(|| panic!("scm.rs has no `fn {name}`"));
    let body = &code[at..];
    let end = [body.find("\n}\n"), body.find("\n    }\n")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or_else(|| panic!("the end of `fn {name}` in scm.rs"));
    body[..end].to_string()
}

/// Where `fn <name>` in scm.rs sets the preshutdown timeout: the text
/// that carries the `ChangeServiceConfig2W` call (the body when it calls
/// it directly, else the body of the scm.rs helper it calls for it) and
/// the call needle that sits in the body.
fn preshutdown_set(name: &str) -> (String, String) {
    let body = scm_fn(name);
    if body.contains("ChangeServiceConfig2W(") {
        return (body, "ChangeServiceConfig2W(".to_string());
    }
    let helpers: Vec<String> = scm_code()
        .lines()
        .filter_map(|l| l.strip_prefix("fn ").and_then(|r| r.split('(').next()))
        .filter(|n| *n != name)
        .map(|n| n.to_string())
        .collect();
    for helper in helpers {
        let call = format!("{helper}(");
        if body.contains(&call) && scm_fn(&helper).contains("ChangeServiceConfig2W(") {
            return (scm_fn(&helper), call);
        }
    }
    panic!(
        "`fn {name}` in scm.rs calls neither ChangeServiceConfig2W \
                nor a helper that does"
    );
}

/// A service the manager creates (start, boot, sync) must carry the
/// preshutdown timeout: the SCM's default is 10 s
/// (SERVICE_PRESHUTDOWN_INFO,
/// learn.microsoft.com/en-us/windows/win32/api/winsvc/ns-winsvc-service_preshutdown_info),
/// far too short for the reboot save. `CreateServiceW`'s `dwAccess` is
/// the access of the handle it returns, so the handle the create hands
/// on needs `SERVICE_CHANGE_CONFIG` for the `ChangeServiceConfig2W` call
/// (learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-createservicew).
#[test]
fn create_sets_the_preshutdown_timeout_on_the_new_service() {
    let body = scm_fn("create");
    let (set, call) = preshutdown_set("create");
    for needle in [
        "ChangeServiceConfig2W(",
        "SERVICE_CONFIG_PRESHUTDOWN_INFO",
        "dwPreshutdownTimeout",
        "preshutdown_timeout_ms",
    ] {
        assert!(
            set.contains(needle),
            "create never sets {needle}: a new instance service keeps \
                 the SCM's short preshutdown default"
        );
    }
    let created = body
        .find("CreateServiceW(")
        .expect("create keeps calling CreateServiceW");
    let set_at = body
        .find(&call)
        .expect("create keeps setting the preshutdown timeout");
    assert!(
        created < set_at,
        "the timeout is set on the handle CreateServiceW returned, \
             after it"
    );
    let args = call_arguments(&body, "CreateServiceW");
    assert!(
        args.contains("access | SERVICE_CHANGE_CONFIG")
            || args.contains("SERVICE_CHANGE_CONFIG | access"),
        "create must OR SERVICE_CHANGE_CONFIG into the access it opens \
             with, so the handle it hands on may call ChangeServiceConfig2W: \
             {args}"
    );
}

/// The sync applies the spec again to every kept service: the
/// preshutdown timeout too, so a service created by an earlier build
/// gets the longer window.
#[test]
fn reconfigure_sets_the_preshutdown_timeout() {
    let body = scm_fn("reconfigure");
    let (set, call) = preshutdown_set("reconfigure");
    for needle in [
        "ChangeServiceConfig2W(",
        "SERVICE_CONFIG_PRESHUTDOWN_INFO",
        "dwPreshutdownTimeout",
        "preshutdown_timeout_ms",
    ] {
        assert!(
            set.contains(needle),
            "reconfigure never sets {needle}: a kept service keeps the \
                 timeout it was created with"
        );
    }
    let config = body
        .find("ChangeServiceConfigW(")
        .expect("reconfigure keeps calling ChangeServiceConfigW");
    let set_at = body
        .find(&call)
        .expect("reconfigure keeps setting the preshutdown timeout");
    assert!(
        config < set_at,
        "the timeout is set on the handle ChangeServiceConfigW retook, \
             after it"
    );
}

/// `start` opens the service, or creates it when it is missing, so it
/// sets the preshutdown timeout on the handle it got: a service created
/// earlier, whose timeout was never set (a create that could not set it,
/// or an install before the timeout existed), would otherwise keep the
/// SCM's short default until the next `sync`. Two parts:
///
/// - the open's access carries `SERVICE_CHANGE_CONFIG`, the access
///   `ChangeServiceConfig2W` needs on the handle (`create` adds it to
///   the handle it returns on its own),
/// - a service that cannot carry the window is still started: starting
///   the instance is what the caller asked for, the window is the reboot
///   path's, so the result is ignored (`let _ =`),
/// - the write names `SERVICE_PRESHUTDOWN_INFO` and lands on the handle
///   the open returned, not on a handle opened for something else.
#[test]
fn start_sets_the_preshutdown_timeout_on_the_service_it_opened() {
    let body = scm_fn("start");
    let (set, call) = preshutdown_set("start");
    for needle in [
        "ChangeServiceConfig2W(",
        "SERVICE_CONFIG_PRESHUTDOWN_INFO",
        "dwPreshutdownTimeout",
    ] {
        assert!(
            set.contains(needle),
            "start never sets {needle}: a service it opens keeps the \
                 SCM's short preshutdown default"
        );
    }
    assert!(
        body.contains("preshutdown_timeout_ms"),
        "start sets no timeout of the spec, so an instance can be left \
             with a window that is not the save's: {body}"
    );
    let set_at = body
        .find(&call)
        .expect("start keeps setting the preshutdown timeout");
    let started = body
        .find("StartServiceW(")
        .expect("start keeps calling StartServiceW");
    assert!(
        set_at < started,
        "the window is set on the handle the open returned, before the \
             service starts"
    );
    let before = body[..set_at].trim_end();
    assert!(
        before.ends_with("let _ ="),
        "a service that cannot carry the window must still start: ignore \
             the result, `let _ = {call}(..)`"
    );
    let args = call_arguments(&body, "open_service");
    let access = body
        .lines()
        .find(|l| l.contains("let access"))
        .unwrap_or_default();
    assert!(
        args.contains("SERVICE_CHANGE_CONFIG") || access.contains("SERVICE_CHANGE_CONFIG"),
        "start opens the service without SERVICE_CHANGE_CONFIG, so the \
             preshutdown change is refused on a service it did not create: \
             {args}"
    );
    assert!(
        set.contains("SERVICE_PRESHUTDOWN_INFO"),
        "the write names no SERVICE_PRESHUTDOWN_INFO: the preshutdown \
             window is set as SERVICE_CONFIG_PRESHUTDOWN_INFO carrying that \
             structure"
    );
    let open_line = body
        .lines()
        .find(|l| l.contains("open_service("))
        .unwrap_or_default();
    let handle = open_line
        .split('=')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches("let ")
        .trim()
        .to_string();
    assert!(
        !handle.is_empty(),
        "start opens the service to no variable, so the window cannot be \
             pinned to the handle it opened"
    );
    let set_args = call_arguments(&body, call.trim_end_matches('('));
    assert!(
        set_args.contains(&format!("&{handle}")),
        "the window is set on something other than the handle the open \
             returned (`{handle}`): {set_args}"
    );
}

/// The restart answers once the stop is accepted: `stop(id)` stays at
/// the top and its `?` stays, the wait and the start move to a spawned
/// thread (the stop's save can run to the whole budget, ARCHITECTURE.md
/// "Budget"). The delayed start waits `RESTART_STOP_WAIT`, not the old
/// 30 s, and a failed start records one line, built by
/// `restart_failure_line`, on the log of who acted.
#[test]
fn restart_answers_once_the_stop_is_accepted() {
    let body = scm_fn("restart");
    let stop = body
        .find("stop(id)")
        .expect("restart keeps asking for the stop first");
    let spawn = body.find("thread::spawn").expect(
        "the wait and the start must move to a spawned thread: \
                     restart must answer once the stop is accepted",
    );
    assert!(
        stop < spawn,
        "the stop must be asked, and accepted, before restart answers"
    );
    for call in [
        "RESTART_STOP_WAIT",
        ".start(",
        ".record(",
        "restart_failure_line(",
    ] {
        assert!(
            body.find(call).is_some_and(|at| at > spawn),
            "{call} must be used on the spawned side of restart, after \
                 thread::spawn"
        );
    }
    let no_old_wait = body.replace("RESTART_STOP_WAIT", "");
    assert!(
        !no_old_wait.contains("STOP_WAIT"),
        "restart still waits with the old 30 s STOP_WAIT"
    );
}

/// The delayed start's failure must reach the log of who acted:
/// `ScmService` is given that log as an `Arc` the spawned closure can
/// clone and record through (`ActionLog`, `manager/service.rs`).
#[test]
fn the_scm_service_is_given_the_action_log() {
    let code = scm_code();
    assert!(
        code.contains("fn with_log("),
        "ScmService has no with_log setter: the delayed start's failure \
             has no log to record on"
    );
    assert!(
        code.contains("dyn ActionLog"),
        "the log is not an Arc<dyn ActionLog + Send + Sync> the closure \
             can own across the thread"
    );
}

/// The gate lives in the glue: `ScmService` holds it as an `Arc`, so
/// every clone of it (the one the delayed start owns, the one the
/// manager calls) asks and raises the same generations.
#[test]
fn the_scm_service_holds_the_gate_the_delayed_start_asks() {
    let code = scm_code();
    assert!(
        code.contains("Arc<RestartGate>"),
        "ScmService holds no Arc<RestartGate>: the thread of a delayed \
             start has no gate to ask, and a stop on another clone of the \
             service would cancel nothing"
    );
}

/// A stop raises the instance's generation, and does it before the stop
/// is asked of the SCM: the thread of a delayed start starts the service
/// as soon as the status is no longer active, so a generation raised
/// after `ControlService` returns can arrive after that start.
#[test]
fn stop_cancels_the_pending_restart_of_its_instance() {
    let body = scm_fn("stop");
    let cancel = body
        .find(".cancel(")
        .expect("stop cancels the restart of its instance that is waiting");
    let control = body
        .find("ControlService(")
        .expect("stop keeps asking the SCM for the stop");
    assert!(
        cancel < control,
        "the generation goes up before the stop is asked, so the thread \
             that is waiting is stale by the time it sees the service stop"
    );
    let args = call_arguments(&body, ".cancel");
    assert!(
        args.contains("id"),
        "stop cancels the pending restart of an id that is not its own: \
             {args}"
    );
}

/// The text before a call, with the call's receiver (`scm.gate`) taken
/// off, so what is left is the construct the call sits in.
fn without_receiver(text: &str) -> &str {
    let mut rest = text.trim_end();
    loop {
        let no_ident = rest.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
        if no_ident.len() < rest.len() {
            rest = no_ident.trim_end();
        }
        match rest.strip_suffix('.') {
            Some(before_dot) => rest = before_dot.trim_end(),
            None => break,
        }
    }
    rest
}

/// The restart takes its generation from the gate after the stop (the
/// stop cancels, so a generation taken before it would be cancelled by
/// the restart's own stop) and before the thread is spawned; the delayed
/// start asks the gate, as the condition of the start, before it starts.
/// The generation it asks about is the one that restart took.
#[test]
fn the_delayed_start_checks_its_generation_before_it_starts() {
    let body = scm_fn("restart");
    let stop = body
        .find("stop(id)")
        .expect("restart keeps asking for the stop first");
    let begin = body
        .find(".begin(")
        .expect("restart takes a generation from the gate");
    let spawn = body
        .find("thread::spawn")
        .expect("the wait and the start stay on a spawned thread");
    assert!(
        stop < begin,
        "`stop` cancels, so restart must take its generation after the \
             stop: one taken before it is made stale by the restart's own stop"
    );
    assert!(
        begin < spawn,
        "the generation is taken before the thread is spawned, and owned \
             by it"
    );
    let current = body
        .find(".is_current(")
        .expect("the delayed start asks the gate before it starts");
    let start = body
        .find(".start(")
        .expect("the delayed start keeps starting the service");
    assert!(
        current < start,
        "the gate is asked after the start: a stop asked during the wait \
             would be undone"
    );
    let guard = without_receiver(&body[..current]);
    assert!(
        guard.ends_with("if") || guard.ends_with("if !"),
        "the gate's answer must be the condition of the start, not a \
             value computed and left: {guard}"
    );
    let between = &body[current..start];
    assert!(
        between.matches(';').count() <= 1 && !between.contains(".record("),
        "nothing but the guard's own return sits between the gate's \
             answer and the start: {between}"
    );
    let line = body
        .lines()
        .find(|l| l.contains(".begin("))
        .expect("the begin call");
    let names: Vec<String> = line
        .split('=')
        .next()
        .unwrap_or_default()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty() && *w != "let")
        .map(|w| w.to_string())
        .collect();
    let begin_args = call_arguments(&body, ".begin");
    let asked = call_arguments(&body, ".is_current");
    assert!(
        names
            .iter()
            .any(|n| asked.contains(n) && !begin_args.contains(n)),
        "the generation the delayed start asks about is not the one this \
             restart took: begin's result must be bound and carried into the \
             thread (begin {begin_args:?}, asked {asked:?})"
    );
}
