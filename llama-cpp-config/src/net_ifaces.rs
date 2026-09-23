//! Network interface enumeration and the bind-address list for the "Bind to"
//! checkboxes in the Server tab.
//!
//! `Hostname` in server.ini is llama-server's `--host`, which since llama.cpp
//! v0.5.0 (#28690) takes a COMMA-SEPARATED list of addresses, each one bound on
//! its own. Upstream parses it with `parse_csv_row` + trim and drops empty
//! entries (`common/arg.cpp`), so [`parse_hosts`] does the same and
//! [`join_hosts`] writes the canonical `a,b` form back. Its help text adds that
//! "overlapping addresses result in undefined behavior", which is why
//! [`ALL_INTERFACES`] is exclusive here: it already covers every IPv4 address,
//! so [`toggle`] never lets it share the list with anything else.

use std::net::Ipv4Addr;

/// The all-interfaces IPv4 bind. Exclusive in the list (see the module header).
pub const ALL_INTERFACES: &str = "0.0.0.0";

#[derive(Debug, Clone)]
pub struct BindOption {
    pub label: String,
    pub value: String,
}

/// One "Bind to" checkbox row, built here and rendered one-way by the Server tab
/// (the `BindRow` Slint struct mirrors it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindRow {
    pub label: String,
    pub value: String,
    pub checked: bool,
    /// False for every other row while [`ALL_INTERFACES`] is checked: they
    /// would overlap it.
    pub enabled: bool,
}

/// Split a `Hostname` value into its addresses, exactly as llama.cpp's `--host`
/// handler does: on `,`, trimmed, empty entries dropped.
pub fn parse_hosts(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map(str::to_string)
        .collect()
}

/// The canonical `Hostname` value for a list of addresses.
pub fn join_hosts(hosts: &[String]) -> String {
    hosts.join(",")
}

/// Check or uncheck `value` in the `current` list. Checking [`ALL_INTERFACES`]
/// replaces the whole list, and checking anything else drops it, so the two
/// never overlap. Unchecking the last address falls back to localhost, the
/// default an unset `Hostname` means and the value `server_form::config_to_form`
/// shows for it, so the form does not read as dirty over nothing.
pub fn toggle(current: &str, value: &str) -> String {
    let mut hosts = parse_hosts(current);
    if let Some(pos) = hosts.iter().position(|h| h.eq_ignore_ascii_case(value)) {
        hosts.remove(pos);
    } else if value == ALL_INTERFACES {
        hosts = vec![ALL_INTERFACES.to_string()];
    } else {
        hosts.retain(|h| h != ALL_INTERFACES);
        hosts.push(value.to_string());
    }
    if hosts.is_empty() {
        return fixed_rows()[0].value.clone();
    }
    join_hosts(&hosts)
}

/// The two fixed bind choices that always lead the list, before any detected
/// interface: loopback-only, then all-interfaces. `build_rows` inserts stale
/// saved values right after these, so their count is the single source for that slot.
fn fixed_rows() -> Vec<BindOption> {
    vec![
        BindOption {
            label: "localhost (only this machine)".into(),
            value: "localhost".into(),
        },
        BindOption {
            label: "0.0.0.0 (all interfaces, LAN-reachable)".into(),
            value: ALL_INTERFACES.into(),
        },
    ]
}

/// Enumerate the machine's usable IPv4 interfaces (loopback / link-local
/// filtered out), sorted by name then address, each as an `ip (name, net/prefix)`
/// row. Empty when enumeration fails.
pub fn interfaces() -> Vec<BindOption> {
    let mut ifaces = match if_addrs::get_if_addrs() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    ifaces.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.ip().to_string().cmp(&b.ip().to_string()))
    });

    let mut out = Vec::new();
    for iface in ifaces {
        let if_addrs::IfAddr::V4(v4) = iface.addr else {
            continue;
        };
        if v4.ip.is_loopback() || v4.ip.is_link_local() {
            continue;
        }
        let prefix = netmask_to_prefix(v4.netmask);
        let network = network_of(v4.ip, v4.netmask);
        let label = format!(
            "{ip} ({name}, {net}/{prefix})",
            ip = v4.ip,
            name = iface.name,
            net = network,
            prefix = prefix,
        );
        out.push(BindOption {
            label,
            value: v4.ip.to_string(),
        });
    }

    out
}

/// Build the "Bind to" checkbox rows for the saved `current` list: the two fixed
/// rows lead, `ifaces` follow, and each address of `current` checks its row. An
/// address matching no row is preserved as a checked "(no longer present)" row
/// right after the fixed ones, so a stale saved address never silently vanishes
/// from the launch line. An empty `current` checks localhost, the default it
/// falls back to.
pub fn build_rows(ifaces: &[BindOption], current: &str) -> Vec<BindRow> {
    let mut opts = fixed_rows();
    let fixed = opts.len();
    opts.extend_from_slice(ifaces);

    let mut hosts = parse_hosts(current);
    if hosts.is_empty() {
        hosts.push(fixed_rows()[0].value.clone());
    }

    // Case-insensitive, like the sibling builders (devices matches ids
    // case-insensitively, model_scan uses paths_eq): a hand-edited
    // `Hostname = Localhost` must check the localhost row, not spawn a
    // spurious "(no longer present)" twin.
    let stale: Vec<BindOption> = hosts
        .iter()
        .filter(|h| !opts.iter().any(|o| o.value.eq_ignore_ascii_case(h)))
        .map(|h| BindOption {
            label: format!("{h} (no longer present)"),
            value: h.clone(),
        })
        .collect();
    opts.splice(fixed..fixed, stale);

    let all = hosts.iter().any(|h| h == ALL_INTERFACES);
    opts.into_iter()
        .map(|o| {
            let checked = hosts.iter().any(|h| h.eq_ignore_ascii_case(&o.value));
            BindRow {
                enabled: !all || o.value == ALL_INTERFACES,
                checked,
                label: o.label,
                value: o.value,
            }
        })
        .collect()
}

fn netmask_to_prefix(mask: Ipv4Addr) -> u32 {
    mask.octets().iter().map(|b| b.count_ones()).sum()
}

fn network_of(ip: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    let ip_u = u32::from(ip);
    let mk_u = u32::from(mask);
    Ipv4Addr::from(ip_u & mk_u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(value: &str) -> BindOption {
        BindOption {
            label: format!("{value} (eth0, {value}/24)"),
            value: value.to_string(),
        }
    }

    fn checked(rows: &[BindRow]) -> Vec<&str> {
        rows.iter()
            .filter(|r| r.checked)
            .map(|r| r.value.as_str())
            .collect()
    }

    #[test]
    fn parse_hosts_mirrors_llama_cpp_csv_handling() {
        assert_eq!(parse_hosts(" a , ,b,"), vec!["a", "b"]);
        assert!(parse_hosts("  ").is_empty());
        assert_eq!(join_hosts(&parse_hosts("a, b")), "a,b");
    }

    #[test]
    fn build_rows_defaults_to_localhost_when_empty() {
        let rows = build_rows(&[], "");
        assert_eq!(checked(&rows), vec!["localhost"]);
        assert!(rows.iter().all(|r| r.enabled));
    }

    #[test]
    fn build_rows_checks_every_listed_address() {
        let ifaces = [iface("192.168.1.10"), iface("10.0.0.2")];
        let rows = build_rows(&ifaces, "localhost,10.0.0.2");
        assert_eq!(rows.len(), 4); // 2 fixed rows + 2 interfaces
        assert_eq!(checked(&rows), vec!["localhost", "10.0.0.2"]);
    }

    #[test]
    fn build_rows_all_interfaces_disables_the_rest() {
        let rows = build_rows(&[iface("192.168.1.10")], "0.0.0.0");
        assert_eq!(checked(&rows), vec!["0.0.0.0"]);
        for r in &rows {
            assert_eq!(r.enabled, r.value == ALL_INTERFACES, "{}", r.value);
        }
    }

    #[test]
    fn build_rows_matches_current_case_insensitively() {
        let rows = build_rows(&[], "Localhost");
        assert_eq!(checked(&rows), vec!["localhost"]);
        assert!(!rows.iter().any(|r| r.label.contains("no longer present")));
    }

    #[test]
    fn build_rows_preserves_stale_values_after_fixed_rows() {
        let rows = build_rows(&[iface("192.168.1.10")], "192.168.1.10,10.0.0.5,10.0.0.6");
        assert_eq!(rows[2].value, "10.0.0.5");
        assert_eq!(rows[3].value, "10.0.0.6");
        assert!(rows[2].label.contains("no longer present") && rows[2].checked);
        assert_eq!(rows[4].value, "192.168.1.10");
        assert!(rows[4].checked);
    }

    #[test]
    fn toggle_adds_and_removes_addresses() {
        assert_eq!(toggle("localhost", "192.168.1.10"), "localhost,192.168.1.10");
        assert_eq!(toggle("localhost,192.168.1.10", "localhost"), "192.168.1.10");
        // Unchecking the last address falls back to the default.
        assert_eq!(toggle("192.168.1.10", "192.168.1.10"), "localhost");
        assert_eq!(toggle("Localhost", "localhost"), "localhost");
    }

    #[test]
    fn toggle_keeps_all_interfaces_exclusive() {
        assert_eq!(toggle("localhost,192.168.1.10", ALL_INTERFACES), ALL_INTERFACES);
        assert_eq!(toggle(ALL_INTERFACES, "localhost"), "localhost");
        assert_eq!(toggle(ALL_INTERFACES, ALL_INTERFACES), "localhost");
    }
}
