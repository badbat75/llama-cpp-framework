use super::{render, render_profile, rows, AssignmentRow};
use crate::ini;

/// Both ending styles a pool file can have.
const EOLS: [&str; 2] = ["\n", "\r\n"];

/// A file's text: `lines` joined by `eol`, the file ending with one.
fn file_of(lines: &[&str], eol: &str) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push_str(eol);
    }
    out
}

/// The pool that is switched on. Its comments, key order, compact
/// spellings and indentation are what the render must keep byte for byte.
fn pool_on(eol: &str) -> String {
    file_of(
        &[
            "; models.ini, the pool's switched-on presets",
            "[*]",
            "temp = 0.7",
            "n-gpu-layers = 99",
            "",
            "; the alpha preset",
            "[alpha]",
            "model = /models/alpha.gguf",
            "ctx-size=8192",
            "    temp = 0.2 ; an inline note",
            "",
            "[beta]",
            "model = /models/beta.gguf",
        ],
        eol,
    )
}

/// The pool that is switched off.
fn pool_off(eol: &str) -> String {
    file_of(
        &[
            "; models.ini.disabled, the pool's switched-off presets",
            "[gamma]",
            "model = /models/gamma.gguf",
            "load-on-startup = false",
        ],
        eol,
    )
}

/// A pool with `beta` before `alpha` and no `[*]`: what the render orders
/// by, and the shape with no global section.
fn pool_no_global(eol: &str) -> String {
    file_of(
        &[
            "[beta]",
            "model = /models/beta.gguf",
            "",
            "[alpha]",
            "model = /models/alpha.gguf",
        ],
        eol,
    )
}

/// `models.ini` of a pool where `alpha` sits in BOTH files.
fn pool_both_on(eol: &str) -> String {
    file_of(
        &[
            "; models.ini, the pool's switched-on presets",
            "[*]",
            "temp = 0.7",
            "",
            "[alpha]",
            "model = /models/alpha.gguf",
            "ctx-size = 8192",
            "",
            "[beta]",
            "model = /models/beta.gguf",
        ],
        eol,
    )
}

/// `models.ini.disabled` of the same pool: the same `alpha`, switched off,
/// with its own text so the render's copy is told apart from the one in
/// `models.ini`.
fn pool_both_off(eol: &str) -> String {
    file_of(
        &[
            "; models.ini.disabled, the pool's switched-off presets",
            "[alpha]",
            "; the alpha preset, switched off",
            "model = /models/alpha-off.gguf",
            "",
            "[gamma]",
            "model = /models/gamma.gguf",
        ],
        eol,
    )
}

/// `file_of` with the last line's ending removed: a file that ends in the
/// middle of a line, as a hand-edited one does.
fn file_open_end(lines: &[&str], eol: &str) -> String {
    file_of(lines, eol)
        .trim_end_matches(['\r', '\n'])
        .to_string()
}

/// A section's verbatim text in `pool` (comments, key order, spacing), the
/// way the render is required to copy it.
fn section(pool: &str, id: &str) -> String {
    ini::text::section_text(pool, id).unwrap_or_default()
}

/// A render split into its header (every line before the first section
/// header) and the sections that follow.
fn parts(render: &str) -> (String, String) {
    let mut head = String::new();
    for line in render.split_inclusive('\n') {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            return (head.clone(), render[head.len()..].to_string());
        }
        head.push_str(line);
    }
    (render.to_string(), String::new())
}

/// The ids of the section headers of `text`, in order.
fn headers(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let t = line.trim();
            t.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .map(|name| name.trim().to_string())
        })
        .collect()
}

/// Pins `body` to be exactly the sections `want` (each a pool text and a
/// section id), in this order: every one of them sits in `body` byte for
/// byte (its line breaks included, its trailing ones not), in order, and
/// `body` holds no other section. Blank lines the render puts between two
/// sections are its own.
fn assert_sections(body: &str, want: &[(&str, &str)], eol: &str) {
    let mut ids: Vec<String> = Vec::new();
    let mut from = 0;
    for (pool, id) in want {
        let text = section(pool, id);
        assert!(
            !text.is_empty(),
            "the fixture pool holds no [{id}] ({eol:?})"
        );
        let kept = text.trim_end_matches(['\r', '\n']);
        let at = body[from..].find(kept).unwrap_or_else(|| {
            panic!("the section [{id}] is not in the render byte for byte ({eol:?}): {body:?}")
        });
        from += at + kept.len();
        ids.push((*id).to_string());
    }
    assert_eq!(
        headers(body),
        ids,
        "the render's sections are not these, in this order ({eol:?})"
    );
}

/// The render's header: comments on a line of their own, saying the file is
/// generated.
fn assert_generated_header(head: &str, eol: &str) {
    assert!(
        head.ends_with('\n'),
        "the header is not its own line ({eol:?}): {head:?}"
    );
    assert!(
        head.to_lowercase().contains("generated"),
        "the header does not say the file is generated ({eol:?}): {head:?}"
    );
    for line in head.lines() {
        let t = line.trim();
        assert!(
            t.is_empty() || t.starts_with(';'),
            "the header is not a comment ({eol:?}): {line:?}"
        );
    }
}

/// The header's ending style: the render has one style, the pool's, no CR
/// in an LF render and no bare LF in a CRLF one.
fn assert_one_ending_style(render: &str, eol: &str) {
    if eol == "\n" {
        assert!(
            !render.contains('\r'),
            "an LF pool rendered with a CR: {render:?}"
        );
    } else {
        assert_eq!(
            render.matches("\r\n").count(),
            render.matches('\n').count(),
            "a CRLF pool rendered with a bare LF line ending: {render:?}"
        );
    }
}

/// Every section header of `body` opens a line of its own: the character in
/// front of the `[` that opens it is a line break (or the body starts).
fn assert_headers_own_lines(body: &str, ids: &[&str], eol: &str) {
    for id in ids {
        let needle = format!("[{id}]");
        let at = body
            .find(&needle)
            .unwrap_or_else(|| panic!("the render holds no {needle} header ({eol:?}): {body:?}"));
        assert!(
            at == 0 || body[..at].ends_with('\n'),
            "the {needle} header does not start a line ({eol:?}): {body:?}"
        );
    }
}

/// The section's text sits in `body` and its last line ENDS there: it is
/// not glued to the section that follows it, so the section's last value
/// stays one value.
fn assert_section_ends_at_break(body: &str, pool: &str, id: &str, eol: &str) {
    let text = section(pool, id);
    let kept = text.trim_end_matches(['\r', '\n']);
    let at = body.find(kept).unwrap_or_else(|| {
        panic!("the section [{id}] is not in the render byte for byte ({eol:?}): {body:?}")
    });
    let after = &body[at + kept.len()..];
    assert!(
        after.is_empty() || after.starts_with("\r\n") || after.starts_with('\n'),
        "the [{id}] section is glued to what follows it ({eol:?}): {body:?}"
    );
}

/// The render of a pool where one id sits in BOTH files: the sections
/// `want` in order, with that id rendered ONCE, from `models.ini` (never
/// the copy switched off in `models.ini.disabled`).
fn assert_alpha_once(body: &str, want: &[(&str, &str)], off: &str, eol: &str) {
    assert_sections(body, want, eol);
    assert_eq!(
        headers(body).iter().filter(|id| *id == "alpha").count(),
        1,
        "the id in both files of the pool is not rendered exactly once ({eol:?}): {:?}",
        headers(body)
    );
    let off_alpha = section(off, "alpha");
    let off_alpha = off_alpha.trim_end_matches(['\r', '\n']);
    assert!(
        !body.contains(off_alpha),
        "the copy switched off in models.ini.disabled is in the render ({eol:?}): {body:?}"
    );
}

/// `* = inherit`: the pool's `[*]` section and every model profile the global
/// switch has on, nothing switched off.
#[test]
fn the_render_star_inherit_follows_the_global_switch() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = inherit"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "alpha"), (&on, "beta")], eol);
        assert_generated_header(&head, eol);
    }
}

/// `* = off` drops every globally-on model profile; the explicit `on` survives.
#[test]
fn the_render_star_off_keeps_only_the_explicit_on() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = off", "beta = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "beta")], eol);
        assert_generated_header(&head, eol);
    }
}

/// An explicit `off` wins over `* = on`, which reaches into the
/// switched-off file.
#[test]
fn the_render_explicit_off_wins_over_the_star() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on", "beta = off"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "alpha"), (&off, "gamma")], eol);
        assert_generated_header(&head, eol);
    }
}

/// `on` renders a model profile that sits only in `models.ini.disabled`, copied
/// byte for byte from that file.
#[test]
fn the_render_override_on_renders_a_model_profile_only_in_the_disabled_file() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = inherit", "gamma = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(
            &body,
            &[(&on, "*"), (&on, "alpha"), (&on, "beta"), (&off, "gamma")],
            eol,
        );
        assert_generated_header(&head, eol);
    }
}

/// `inherit` on a named key follows the global switch: on from
/// `models.ini`, absent from `models.ini.disabled`.
#[test]
fn the_render_inherit_of_a_named_key_follows_the_global_switch() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["alpha = inherit", "gamma = inherit"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "alpha")], eol);
        assert_generated_header(&head, eol);
    }
}

/// The section text arrives as `ini::text::section_text` hands it: its
/// `[<id>]` header line, comment lines and inline comments carry no
/// assignment.
#[test]
fn the_render_assignment_comments_and_inline_comments_are_ignored() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(
            &[
                "[default]",
                "; the switch of alpha is explicit",
                "alpha = on ; set by hand",
                "",
                "gamma = off",
            ],
            eol,
        );
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "alpha")], eol);
        assert_generated_header(&head, eol);
    }
}

/// No assignment section, an empty one, one holding only a comment, one
/// switching a model profile off: the instance is given `[*]` alone.
#[test]
fn the_render_no_assignment_section_renders_only_the_global_section() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        for case in [
            None,
            Some(""),
            Some("[default]\n; nothing assigned\n"),
            Some("[default]\nbeta = off\n"),
        ] {
            let (head, body) = parts(&render(&on, &off, case));
            assert_sections(&body, &[(&on, "*")], eol);
            assert_generated_header(&head, eol);
        }
    }
}

/// An assigned id the pool does not hold renders nothing: alone it leaves
/// the render with no section at all, beside a `*` it adds none.
#[test]
fn the_render_assigned_id_missing_from_the_pool_renders_nothing() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on", "zeta = inherit"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(
            &body,
            &[(&on, "*"), (&on, "alpha"), (&on, "beta"), (&off, "gamma")],
            eol,
        );
        assert_generated_header(&head, eol);

        let assignment = file_of(&["zeta = on", "alpha = inherit"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "alpha")], eol);
        assert_generated_header(&head, eol);
    }
}

/// `[*]` is first whatever the pool looks like: last in the file, or only
/// in the switched-off one.
#[test]
fn the_render_global_section_is_first() {
    for eol in EOLS {
        let late = file_of(
            &[
                "[alpha]",
                "model = /models/alpha.gguf",
                "",
                "[*]",
                "temp = 0.7",
            ],
            eol,
        );
        let empty = String::new();
        let assignment = file_of(&["* = inherit"], eol);
        let (head, body) = parts(&render(&late, &empty, Some(&assignment)));
        assert_eq!(
            headers(&body).first().map(String::as_str),
            Some("*"),
            "the global section is not first ({eol:?}): {:?}",
            headers(&body)
        );
        assert_sections(&body, &[(&late, "*"), (&late, "alpha")], eol);
        assert_generated_header(&head, eol);

        let on = file_of(&["[alpha]", "model = /models/alpha.gguf"], eol);
        let off = file_of(
            &[
                "[*]",
                "temp = 0.7",
                "",
                "[gamma]",
                "model = /models/gamma.gguf",
            ],
            eol,
        );
        let assignment = file_of(&["alpha = inherit", "gamma = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&off, "*"), (&on, "alpha"), (&off, "gamma")], eol);
        assert_generated_header(&head, eol);
    }
}

/// A pool with no `[*]` section: the render is the assigned model profiles, in the
/// pool's order (`enabled` first, then `disabled`), not sorted.
#[test]
fn the_render_follows_the_pool_order_not_the_key_order() {
    for eol in EOLS {
        let on = pool_no_global(eol);
        let off = pool_off(eol);
        let empty = String::new();
        let assignment = file_of(&["* = inherit"], eol);
        let (head, body) = parts(&render(&on, &empty, Some(&assignment)));
        assert_sections(&body, &[(&on, "beta"), (&on, "alpha")], eol);
        assert_generated_header(&head, eol);

        let assignment = file_of(&["beta = inherit", "gamma = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "beta"), (&off, "gamma")], eol);
        assert_generated_header(&head, eol);
    }
}

/// `*` is never a model profile id, so it never becomes a section: on a pool that
/// holds no model profile it renders no section at all, and on a pool that has a
/// `[*]` section it renders that one, once.
#[test]
fn the_render_star_never_becomes_a_model_profile_section() {
    for eol in EOLS {
        let off = pool_off(eol);
        let empty = String::new();
        let bare = file_of(&["; models.ini, no preset in the pool yet"], eol);
        let assignment = file_of(&["* = on"], eol);
        let (head, body) = parts(&render(&bare, &empty, Some(&assignment)));
        assert_eq!(
            headers(&body),
            Vec::<String>::new(),
            "the key `*` rendered a section of its own ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);

        let on = pool_on(eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_eq!(
            headers(&body).iter().filter(|id| *id == "*").count(),
            1,
            "the global section rendered more than once ({eol:?}): {:?}",
            headers(&body)
        );
        assert!(
            body.contains(section(&on, "*").trim_end_matches(['\r', '\n'])),
            "the global section is not in the render ({eol:?})"
        );
        assert_generated_header(&head, eol);
    }
}

/// The pool's ending style is the render's: no CR in an LF render, no bare
/// LF in a CRLF one.
#[test]
fn the_render_keeps_the_pools_ending_style() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on"], eol);
        let got = render(&on, &off, Some(&assignment));
        assert!(
            got.contains(section(&on, "alpha").trim_end_matches(['\r', '\n'])),
            "the alpha section is not in the render ({eol:?})"
        );
        if eol == "\n" {
            assert!(
                !got.contains('\r'),
                "an LF pool rendered with a CR: {got:?}"
            );
        } else {
            assert_eq!(
                got.matches("\r\n").count(),
                got.matches('\n').count(),
                "a CRLF pool rendered with a bare LF line ending: {got:?}"
            );
        }
    }
}

/// One id in both files of the pool is ONE model profile: when it is on it is
/// rendered once, from `models.ini` (the file that holds it on), under its
/// own `on`, under `* = on` and under `inherit`.
#[test]
fn the_render_an_id_in_both_pool_files_is_rendered_once_from_the_enabled_one() {
    for eol in EOLS {
        let (on, off) = (pool_both_on(eol), pool_both_off(eol));

        let assignment = file_of(&["alpha = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_alpha_once(&body, &[(&on, "*"), (&on, "alpha")], &off, eol);
        assert_generated_header(&head, eol);

        let assignment = file_of(&["* = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_alpha_once(
            &body,
            &[(&on, "*"), (&on, "alpha"), (&on, "beta"), (&off, "gamma")],
            &off,
            eol,
        );
        assert_generated_header(&head, eol);

        let assignment = file_of(&["alpha = inherit"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_alpha_once(&body, &[(&on, "*"), (&on, "alpha")], &off, eol);
        assert_generated_header(&head, eol);
    }
}

/// The same id in both files, switched off: the model profile is off, so it is
/// rendered not at all, from neither file, and `* = on` does not reach it.
#[test]
fn the_render_an_id_in_both_pool_files_switched_off_renders_not_at_all() {
    for eol in EOLS {
        let (on, off) = (pool_both_on(eol), pool_both_off(eol));
        let on_alpha = section(&on, "alpha");
        let on_alpha = on_alpha.trim_end_matches(['\r', '\n']);
        let off_alpha = section(&off, "alpha");
        let off_alpha = off_alpha.trim_end_matches(['\r', '\n']);

        let assignment = file_of(&["alpha = off"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*")], eol);
        assert_eq!(
            headers(&body).iter().filter(|id| *id == "alpha").count(),
            0,
            "a preset switched off in both files is rendered ({eol:?}): {:?}",
            headers(&body)
        );
        assert!(
            !body.contains(on_alpha) && !body.contains(off_alpha),
            "a preset switched off in both files is in the render ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);

        let assignment = file_of(&["* = on", "alpha = off"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "beta"), (&off, "gamma")], eol);
        assert_eq!(
            headers(&body).iter().filter(|id| *id == "alpha").count(),
            0,
            "the explicit off does not win over `* = on` ({eol:?}): {:?}",
            headers(&body)
        );
        assert!(
            !body.contains(on_alpha) && !body.contains(off_alpha),
            "the preset the explicit off switched off is in the render ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);
    }
}

/// The last section of a pool file with NO final line break: its text ends
/// in the middle of a line, so the render opens a line of its own for the
/// section that follows, and the section's last value stays one value.
#[test]
fn the_render_a_pool_section_with_no_final_break_does_not_glue_the_next_one() {
    for eol in EOLS {
        let on = file_open_end(
            &[
                "[*]",
                "temp = 0.7",
                "",
                "[beta]",
                "model = /models/beta.gguf",
            ],
            eol,
        );
        let off = file_of(&["[gamma]", "model = /models/gamma.gguf"], eol);
        let assignment = file_of(&["* = on"], eol);
        let got = render(&on, &off, Some(&assignment));
        let (head, body) = parts(&got);
        assert_sections(&body, &[(&on, "*"), (&on, "beta"), (&off, "gamma")], eol);
        assert_headers_own_lines(&body, &["*", "beta", "gamma"], eol);
        assert_section_ends_at_break(&body, &on, "beta", eol);
        assert_generated_header(&head, eol);
        assert_one_ending_style(&got, eol);
    }
}

/// `[*]` as the LAST section of its file, with no final line break: it is
/// rendered first, before the model profile that follows it in the file, so the
/// render breaks the line the global section ends in.
#[test]
fn the_render_the_global_section_last_with_no_final_break_does_not_glue_the_next_one() {
    for eol in EOLS {
        let on = file_open_end(
            &[
                "[alpha]",
                "model = /models/alpha.gguf",
                "",
                "[*]",
                "temp = 0.7",
            ],
            eol,
        );
        let empty = String::new();
        let assignment = file_of(&["alpha = inherit"], eol);
        let got = render(&on, &empty, Some(&assignment));
        let (head, body) = parts(&got);
        assert_sections(&body, &[(&on, "*"), (&on, "alpha")], eol);
        assert_headers_own_lines(&body, &["*", "alpha"], eol);
        assert_section_ends_at_break(&body, &on, "*", eol);
        assert_generated_header(&head, eol);
        assert_one_ending_style(&got, eol);
    }
}

/// One pool file holding the same section twice: llama-server knows one
/// model profile with that name, so the render gives one section, the first, once.
#[test]
fn the_render_a_duplicate_section_in_one_pool_file_renders_once() {
    for eol in EOLS {
        let on = file_of(
            &[
                "[*]",
                "temp = 0.7",
                "",
                "[beta]",
                "model = /models/beta.gguf",
                "",
                "[beta]",
                "model = /models/beta-second.gguf",
            ],
            eol,
        );
        let empty = String::new();
        let assignment = file_of(&["* = on"], eol);
        let (head, body) = parts(&render(&on, &empty, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "beta")], eol);
        assert_eq!(
            headers(&body).iter().filter(|id| *id == "beta").count(),
            1,
            "the section written twice in one file is not rendered exactly once ({eol:?}): {:?}",
            headers(&body)
        );
        assert_eq!(
            body.matches("/models/beta.gguf").count(),
            1,
            "the first copy of the section is not rendered exactly once ({eol:?}): {body:?}"
        );
        assert!(
            !body.contains("/models/beta-second.gguf"),
            "the second copy of the section is in the render ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);
    }
}

/// `[Alpha]` and `[alpha]` in one pool file: the reader knows one section
/// with that name (its lookup ignores the case), so the render gives one
/// section, the first, once.
#[test]
fn the_render_ids_differing_only_in_case_render_one_section_each() {
    for eol in EOLS {
        let on = file_of(
            &[
                "[*]",
                "temp = 0.7",
                "",
                "[Alpha]",
                "model = /models/Alpha.gguf",
                "",
                "[alpha]",
                "model = /models/alpha.gguf",
            ],
            eol,
        );
        let empty = String::new();
        for assignment in [file_of(&["* = on"], eol), file_of(&["Alpha = on"], eol)] {
            let (head, body) = parts(&render(&on, &empty, Some(&assignment)));
            assert_sections(&body, &[(&on, "*"), (&on, "Alpha")], eol);
            assert_eq!(
                headers(&body)
                    .iter()
                    .filter(|id| id.eq_ignore_ascii_case("alpha"))
                    .count(),
                1,
                "the two spellings of one id are not rendered as one section ({eol:?}): {:?}",
                headers(&body)
            );
            assert_eq!(
                body.matches("/models/Alpha.gguf").count(),
                1,
                "the first section of the pair is not rendered exactly once ({eol:?}): {body:?}"
            );
            assert!(
                !body.contains("/models/alpha.gguf"),
                "the second section of the pair is in the render ({eol:?}): {body:?}"
            );
            assert_generated_header(&head, eol);
        }
    }
}

/// An assignment value that is not `inherit`, `on` or `off` (a hand-edited
/// `none`) is read as an absent key: only the pool's `[*]` and the model profiles
/// the assignment really assigns are rendered.
#[test]
fn the_render_an_assignment_value_that_is_not_inherit_on_or_off_reads_as_absent() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));

        for assignment in [file_of(&["alpha = none"], eol), file_of(&["* = none"], eol)] {
            let (head, body) = parts(&render(&on, &off, Some(&assignment)));
            assert_sections(&body, &[(&on, "*")], eol);
            assert!(
                !body.contains("model = /models/alpha.gguf"),
                "the preset of the nonsense value is in the render ({eol:?}): {body:?}"
            );
            assert_generated_header(&head, eol);
        }

        // The nonsense key does not assign alpha; the real `on` next to it
        // still assigns beta.
        let assignment = file_of(&["alpha = none", "beta = on"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(&body, &[(&on, "*"), (&on, "beta")], eol);
        assert!(
            !body.contains("model = /models/alpha.gguf"),
            "the preset of the nonsense value is in the render ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);

        // Under a real `* = on` the model profile has no key of its own to fall
        // back on, so the `*` assigns it: the nonsense value changes nothing.
        let assignment = file_of(&["* = on", "alpha = none"], eol);
        let (head, body) = parts(&render(&on, &off, Some(&assignment)));
        assert_sections(
            &body,
            &[(&on, "*"), (&on, "alpha"), (&on, "beta"), (&off, "gamma")],
            eol,
        );
        assert_generated_header(&head, eol);
    }
}

/// An empty `models.ini` beside a non-empty `models.ini.disabled`: the
/// render is the pool it has, in the ending style of the file that has one.
#[test]
fn the_render_an_empty_enabled_file_renders_the_disabled_pool() {
    for eol in EOLS {
        let empty = String::new();
        let off = file_of(
            &[
                "; models.ini.disabled, the pool's switched-off presets",
                "[*]",
                "temp = 0.7",
                "",
                "[gamma]",
                "model = /models/gamma.gguf",
                "",
                "[delta]",
                "model = /models/delta.gguf",
            ],
            eol,
        );

        let assignment = file_of(&["* = on"], eol);
        let (head, body) = parts(&render(&empty, &off, Some(&assignment)));
        assert_sections(&body, &[(&off, "*"), (&off, "gamma"), (&off, "delta")], eol);
        assert_generated_header(&head, eol);

        // No `*` in the assignment: only the model profile the keys assign.
        let assignment = file_of(&["gamma = on"], eol);
        let (head, body) = parts(&render(&empty, &off, Some(&assignment)));
        assert_sections(&body, &[(&off, "*"), (&off, "gamma")], eol);
        assert!(
            !body.contains("model = /models/delta.gguf"),
            "an unassigned preset of the switched-off file is in the render ({eol:?}): {body:?}"
        );
        assert_generated_header(&head, eol);

        let got = render(&empty, &off, Some(&file_of(&["* = on"], eol)));
        assert_one_ending_style(&got, eol);
    }
}

/// The header line of every generated `models@<id>.ini`, byte for byte: the
/// pool it is cut from is the model profile pool, the name those files carry.
#[test]
fn the_generated_header_names_the_model_profile_pool() {
    const WANT: &str = "; Generated by pai-manager from the model profile pool: do not edit.";
    assert_eq!(super::HEADER, WANT);
    for eol in EOLS {
        let enabled = file_of(&["[*]", "* = on"], eol);
        let got = render(&enabled, "", None);
        assert_eq!(
            parts(&got).0,
            format!("{WANT}{eol}"),
            "the render opens with that line ({eol:?}): {got:?}"
        );
    }
}

// ── rows: the pool and one assignment, read as the web UI's rows ──────

/// `rows` as a table of `(id, value, via_all, effective)`, in the order
/// the call answers them.
fn table(rows: &[AssignmentRow]) -> Vec<(String, String, bool, bool)> {
    rows.iter()
        .map(|row| {
            (
                row.id.clone(),
                row.value.clone(),
                row.via_all,
                row.effective,
            )
        })
        .collect()
}

/// One row of an expected table.
fn row(id: &str, value: &str, via_all: bool, effective: bool) -> (String, String, bool, bool) {
    (id.to_string(), value.to_string(), via_all, effective)
}

/// One call's `*` and its rows, the rows as a table.
fn rows_of(
    enabled: &str,
    disabled: &str,
    assignment: Option<&str>,
) -> (String, Vec<(String, String, bool, bool)>) {
    let (all, rows) = rows(enabled, disabled, assignment);
    (all, table(&rows))
}

/// The ids of the table's rows that are effective.
fn effective(rows: &[(String, String, bool, bool)]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.3)
        .map(|row| row.0.clone())
        .collect()
}

/// `* = inherit`, no other key: the pool in its order, every model profile
/// following the global switch of the file that holds it, and the `*` read
/// as `inherit`.
#[test]
fn the_rows_of_the_star_alone_follow_the_global_switch() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = inherit"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "the key `* = inherit` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", true, true),
                row("beta", "none", true, true),
                row("gamma", "none", true, false),
            ],
            "the rows of `* = inherit` ({eol:?})"
        );
    }
}

/// `* = on`: the key is honoured, so the model profiles of the switched-off
/// file are effective too, and the `*` the page shows is `inherit`.
#[test]
fn the_rows_of_the_star_on_reach_the_disabled_file() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "a honoured `*` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", true, true),
                row("beta", "none", true, true),
                row("gamma", "none", true, true),
            ],
            "the rows of `* = on` ({eol:?})"
        );
    }
}

/// The keys a page writes: `value` is the key the file holds, and with no
/// `*` the rows are not `via_all` and the `*` the page shows is `none`.
#[test]
fn the_rows_value_is_the_explicit_key() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["alpha = inherit", "gamma = on"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "none", "no key `*` in the section ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "inherit", false, true),
                row("beta", "none", false, false),
                row("gamma", "on", false, true),
            ],
            "the rows of two explicit keys ({eol:?})"
        );
    }
}

/// An explicit `off` under `* = on`: the row is not effective, carries the
/// key as its value, and is not covered by the `*`.
#[test]
fn the_rows_explicit_off_wins_over_the_star() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on", "alpha = off"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "the `*` is honoured ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "off", false, false),
                row("beta", "none", true, true),
                row("gamma", "none", true, true),
            ],
            "the explicit off and the `*` ({eol:?})"
        );
    }
}

/// An explicit `inherit` follows the `*` it sits under: with `* = off` the
/// row is not effective, and it is not covered by the `*` either.
#[test]
fn the_rows_explicit_inherit_follows_the_star_it_sits_under() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = off", "alpha = inherit"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "the key `* = off` is honoured ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "inherit", false, false),
                row("beta", "none", true, false),
                row("gamma", "none", true, false),
            ],
            "the rows of `* = off` with an explicit inherit ({eol:?})"
        );
    }
}

/// `on` on a model profile switched off in the pool: the row is effective,
/// and with no `*` in the section the page's `*` is `none`.
#[test]
fn the_rows_on_renders_a_model_profile_switched_off() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["gamma = on"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "none", "no key `*` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", false, false),
                row("beta", "none", false, false),
                row("gamma", "on", false, true),
            ],
            "the rows of one `on` over a switched-off model profile ({eol:?})"
        );
    }
}

/// A hand-edited `* = none`, a word the render ignores: the key is not
/// honoured, so the `*` the page shows is `none`, no row is `via_all`, and
/// the pool's own switch assigns nothing.
#[test]
fn the_rows_a_hand_edited_star_none_is_not_honoured() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = none", "beta = on"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "none", "the ignored `*` is not honoured ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", false, false),
                row("beta", "on", false, true),
                row("gamma", "none", false, false),
            ],
            "the rows of an ignored `*` ({eol:?})"
        );
    }
}

/// A hand-edited `* = on`, a value the page never writes, is honoured all
/// the same: the `*` is `inherit` and every model profile of the pool is
/// effective.
#[test]
fn the_rows_a_hand_edited_star_on_is_honoured() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = on", "gamma = off"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "the honoured `* = on` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", true, true),
                row("beta", "none", true, true),
                row("gamma", "off", false, false),
            ],
            "the rows of a hand-edited `* = on` ({eol:?})"
        );
    }
}

/// A hand-edited value that is none of the three words is no key at all:
/// the row is `none` and, under a honoured `*`, `via_all`.
#[test]
fn the_rows_a_hand_edited_value_reads_as_no_key() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        let assignment = file_of(&["* = off", "alpha = none", "beta = yes"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "inherit", "the `*` is honoured ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", true, false),
                row("beta", "none", true, false),
                row("gamma", "none", true, false),
            ],
            "the rows of two ignored values ({eol:?})"
        );
    }
}

/// One id in both files of the pool is one model profile: one row, in the
/// place the file that holds it on has, never a second row for the copy
/// switched off.
#[test]
fn the_rows_an_id_in_both_pool_files_is_one_row() {
    for eol in EOLS {
        let (on, off) = (pool_both_on(eol), pool_both_off(eol));
        let assignment = file_of(&["alpha = on"], eol);
        let (all, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(all, "none", "no key `*` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("alpha", "on", false, true),
                row("beta", "none", false, false),
                row("gamma", "none", false, false),
            ],
            "the rows of the id in both files ({eol:?})"
        );

        let assignment = file_of(&["* = on"], eol);
        let (_, rows) = rows_of(&on, &off, Some(&assignment));
        assert_eq!(
            rows.iter().filter(|row| row.0 == "alpha").count(),
            1,
            "the id in both files is not one row ({eol:?}): {rows:?}"
        );
        assert_eq!(
            rows,
            vec![
                row("alpha", "none", true, true),
                row("beta", "none", true, true),
                row("gamma", "none", true, true),
            ],
            "the rows of the id in both files under `* = on` ({eol:?})"
        );
    }
}

/// No section at all, and an empty one: every model profile of the pool is
/// a row with no value, not covered by anything, effective nowhere, and the
/// `*` the page shows is `none`.
#[test]
fn the_rows_no_section_is_no_assignment() {
    for eol in EOLS {
        let (on, off) = (pool_on(eol), pool_off(eol));
        for assignment in [None, Some("")] {
            let (all, rows) = rows_of(&on, &off, assignment);
            assert_eq!(all, "none", "no assignment ({eol:?})");
            assert_eq!(
                rows,
                vec![
                    row("alpha", "none", false, false),
                    row("beta", "none", false, false),
                    row("gamma", "none", false, false),
                ],
                "the rows with no assignment section ({eol:?})"
            );
        }
    }
}

/// The rows are the pool's model profiles, in the pool's order (`models.ini`
/// first), once each: never the `*`, never an id only the section names, and
/// ids that differ only in letter case are one row.
#[test]
fn the_rows_are_the_pool_in_its_order_once_each() {
    for eol in EOLS {
        let on = pool_no_global(eol);
        let empty = String::new();
        let assignment = file_of(&["* = inherit", "zeta = on"], eol);
        let (all, rows) = rows_of(&on, &empty, Some(&assignment));
        assert_eq!(all, "inherit", "the key `* = inherit` ({eol:?})");
        assert_eq!(
            rows,
            vec![
                row("beta", "none", true, true),
                row("alpha", "none", true, true)
            ],
            "the rows follow the pool's order, not the sorted one ({eol:?})"
        );

        let twice = file_of(
            &[
                "[*]",
                "temp = 0.7",
                "",
                "[Alpha]",
                "model = /models/Alpha.gguf",
                "",
                "[alpha]",
                "model = /models/alpha.gguf",
            ],
            eol,
        );
        let assignment = file_of(&["* = on"], eol);
        let (_, rows) = rows_of(&twice, &empty, Some(&assignment));
        assert_eq!(
            rows,
            vec![row("Alpha", "none", true, true)],
            "the two spellings of one id are one row ({eol:?})"
        );
        assert!(
            !rows.iter().any(|row| row.0 == "*"),
            "the `*` is a row of its own ({eol:?}): {rows:?}"
        );
    }
}

/// A pool with no model profile: no row, in either ending style, with a
/// honoured `*` and without one.
#[test]
fn the_rows_of_an_empty_pool_are_no_rows() {
    for eol in EOLS {
        let empty = String::new();
        let bare = file_of(&["; models.ini, no preset in the pool yet"], eol);
        assert_eq!(
            rows_of(&empty, &empty, None),
            ("none".to_string(), Vec::new()),
            "no pool, no rows ({eol:?})"
        );
        let (all, rows) = rows_of(&bare, &empty, Some(&file_of(&["* = on"], eol)));
        assert_eq!(all, "inherit", "the `*` is honoured ({eol:?})");
        assert!(
            rows.is_empty(),
            "the key `*` is a row of its own ({eol:?}): {rows:?}"
        );
    }
}

/// The rows that are effective are exactly the sections the render emits
/// after `[*]`, in the render's order: the bff reads one rule, never a
/// second copy of the render.
#[test]
fn the_rows_effective_ids_are_the_sections_the_render_emits() {
    for eol in EOLS {
        let case_pool = file_of(
            &[
                "[Alpha]",
                "model = /models/Alpha.gguf",
                "",
                "[alpha]",
                "model = /models/alpha.gguf",
            ],
            eol,
        );
        let pools: Vec<(String, String)> = vec![
            (pool_on(eol), pool_off(eol)),
            (pool_both_on(eol), pool_both_off(eol)),
            (pool_no_global(eol), String::new()),
            (pool_no_global(eol), pool_off(eol)),
            (case_pool, String::new()),
        ];
        let assignments: [Option<&str>; 13] = [
            None,
            Some("* = inherit"),
            Some("* = on"),
            Some("* = off"),
            Some("* = none"),
            Some("alpha = on"),
            Some("alpha = off"),
            Some("* = on\nalpha = off"),
            Some("* = off\nbeta = on"),
            Some("gamma = on"),
            Some("alpha = inherit\ngamma = inherit"),
            Some("alpha = none\nbeta = on"),
            Some("zeta = on\nalpha = inherit"),
        ];
        for (on, off) in pools.iter() {
            for assignment in assignments {
                let (_, rows) = rows_of(on.as_str(), off.as_str(), assignment);
                let rendered: Vec<String> = headers(&render(on.as_str(), off.as_str(), assignment))
                    .into_iter()
                    .filter(|id| id != "*")
                    .collect();
                assert_eq!(
                    effective(&rows),
                    rendered,
                    "the effective rows are not the rendered sections: assignment \
                         {assignment:?}, pool ({eol:?})"
                );
            }
        }
    }
}

/// The IP1 render's pool: `[*]` and three model profiles, `alpha` with a
/// comment and two keys of its own.
fn ip1_pool(eol: &str) -> String {
    file_of(
        &[
            "[*]",
            "temp = 0.7",
            "",
            "[alpha]",
            "; alpha's note",
            "model = /models/alpha.gguf",
            "temp = 0.2",
            "",
            "[beta]",
            "model = /models/beta.gguf",
            "",
            "[gamma]",
            "model = /models/gamma.gguf",
        ],
        eol,
    )
}

/// The IP1 render's `instance-profiles.ini`: `default` with an entry for
/// `alpha` (two keys) and an empty entry for `beta`, `other` with an entry
/// for `gamma`, and `empty` with none.
fn ip1_profiles(eol: &str) -> String {
    file_of(
        &[
            "[default]",
            "ServerProfile = default",
            "",
            "[default/alpha]",
            "device = cuda0",
            "ctx-size = 8192",
            "",
            "[default/beta]",
            "",
            "[other]",
            "ServerProfile = other",
            "",
            "[other/gamma]",
            "device = cpu",
            "",
            "[empty]",
            "ServerProfile = other",
        ],
        eol,
    )
}

/// A render as sections: the id and the keys and values the INI reader
/// reads from it, in the file's section order.
fn ip1_sections(render: &str) -> Vec<(String, std::collections::BTreeMap<String, String>)> {
    ini::parse_sections(render)
        .into_iter()
        .map(|s| (s.id, s.keys))
        .collect()
}

/// A map of `pairs`, for a readable compare with a section's keys.
fn ip1_map(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// `ip1` criterion: the section of a model profile with an entry is the
/// model profile's own bytes, followed by the entry's key lines; the
/// profile's `[*]` section is first; a model profile with an entry of no
/// keys is its own bytes alone; a model profile with no entry of this
/// profile is not rendered.
#[test]
fn ip1_the_render_is_the_model_section_followed_by_the_entry_keys() {
    for eol in EOLS {
        let (pool, profiles) = (ip1_pool(eol), ip1_profiles(eol));
        let render = render_profile(&pool, &profiles, "default");
        let (head, body) = parts(&render);
        assert_generated_header(&head, eol);
        assert_one_ending_style(&render, eol);
        assert_sections(
            &body,
            &[(&pool, "*"), (&pool, "alpha"), (&pool, "beta")],
            eol,
        );
        let sections = ip1_sections(&render);
        let ids: Vec<String> = sections.iter().map(|(id, _)| id.clone()).collect();
        assert_eq!(
            ids,
            ["*", "alpha", "beta"],
            "the sections are not these, in this order ({eol:?}): {render:?}"
        );
        assert_eq!(
            sections[1].1,
            ip1_map(&[
                ("model", "/models/alpha.gguf"),
                ("temp", "0.2"),
                ("device", "cuda0"),
                ("ctx-size", "8192")
            ]),
            "the [default/alpha] entry's keys are not added to the model profile's ({eol:?})"
        );
        // The entry's lines sit after the model profile's own lines.
        let alpha = section(&pool, "alpha");
        let alpha = alpha.trim_end_matches(['\r', '\n']).to_string();
        let at = body.find(&alpha).unwrap_or_else(|| {
            panic!("the [alpha] section is not in the render byte for byte ({eol:?}): {body:?}")
        });
        let after = &body[at..];
        let own = after
            .find("temp = 0.2")
            .expect("the model profile's own last key");
        let entry = after
            .find("device = cuda0")
            .expect("the entry's key line is not in the render");
        assert!(
            entry > own,
            "the entry's keys come before the model profile's own ({eol:?}): {after:?}"
        );
        assert_eq!(
            sections[2].1,
            ip1_map(&[("model", "/models/beta.gguf")]),
            "the entry of no keys adds a key to [beta] ({eol:?})"
        );
        assert!(
            !body.contains("[gamma]"),
            "a model profile with no entry of this profile is rendered ({eol:?}): {body:?}"
        );
        assert!(
            !body.contains("[default/alpha]"),
            "the entry section itself is written into the render ({eol:?}): {body:?}"
        );
    }
}

/// `ip1` criterion: an instance profile with no entry is rendered as the
/// pool's `[*]` section alone.
#[test]
fn ip1_a_profile_with_no_entry_renders_the_global_section_alone() {
    for eol in EOLS {
        let (pool, profiles) = (ip1_pool(eol), ip1_profiles(eol));
        let render = render_profile(&pool, &profiles, "empty");
        let (head, body) = parts(&render);
        assert_generated_header(&head, eol);
        assert_eq!(
            headers(&body),
            ["*"],
            "a profile with no entry renders more than [*] ({eol:?}): {render:?}"
        );
        assert_sections(&body, &[(&pool, "*")], eol);
    }
}

/// The same entry names read under another profile: the render of `other`
/// is `gamma` alone beside `[*]`, so one profile's entries never reach
/// another's render.
#[test]
fn ip1_one_profile_entries_do_not_reach_another_render() {
    for eol in EOLS {
        let (pool, profiles) = (ip1_pool(eol), ip1_profiles(eol));
        let render = render_profile(&pool, &profiles, "other");
        let ids: Vec<String> = ip1_sections(&render)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            ids,
            ["*", "gamma"],
            "the render of [other] is not its own entries ({eol:?}): {render:?}"
        );
        let sections = ip1_sections(&render);
        assert_eq!(
            sections[1].1,
            ip1_map(&[("model", "/models/gamma.gguf"), ("device", "cpu")]),
            "the [other/gamma] entry's keys are not in its section ({eol:?})"
        );
    }
}

/// The pool of the clashing key: `alpha` holds `ctx-size`, a placement key,
/// so after the migration only the entry can hold it. A pool that still
/// carries it (a file from before the migration, a hand edit) is what the
/// render must not double.
fn ip1_clash_pool(eol: &str) -> String {
    file_of(
        &[
            "[*]",
            "temp = 0.7",
            "",
            "[alpha]",
            "model = /models/alpha.gguf",
            "ctx-size = 8192",
        ],
        eol,
    )
}

/// The profile of the same: `[default/alpha]` names the SAME key, with the
/// value the instance's entry sets for it.
fn ip1_clash_profiles(eol: &str) -> String {
    file_of(
        &[
            "[default]",
            "ServerProfile = default",
            "",
            "[default/alpha]",
            "ctx-size = 4096",
        ],
        eol,
    )
}

/// `ip1` criterion (review D6a): a key the model profile's section and the
/// entry both name is written ONCE, with the entry's value: the rendered
/// section carries no duplicate key line.
#[test]
fn ip1_a_key_the_section_and_the_entry_both_name_is_written_once_with_the_entrys_value() {
    for eol in EOLS {
        let (pool, profiles) = (ip1_clash_pool(eol), ip1_clash_profiles(eol));
        let render = render_profile(&pool, &profiles, "default");
        let (head, body) = parts(&render);
        assert_generated_header(&head, eol);
        let sections = ip1_sections(&render);
        assert_eq!(
            sections
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
            ["*", "alpha"],
            "the render is not the global section and alpha ({eol:?}): {render:?}"
        );
        assert_eq!(
            sections[1].1,
            ip1_map(&[("model", "/models/alpha.gguf"), ("ctx-size", "4096")]),
            "the entry's value does not win over the model profile's own ({eol:?}): {render:?}"
        );
        let lines: Vec<&str> = body
            .lines()
            .filter(|line| line.trim_start().starts_with("ctx-size"))
            .collect();
        assert_eq!(
            lines,
            ["ctx-size = 4096"],
            "the key both the model profile's section and the entry name is written \
                 twice, so the rendered section carries a duplicate key line ({eol:?}): {body:?}"
        );
    }
}

/// `ip1` criterion (review D6b): a pool of one ending style and an
/// `instance-profiles.ini` of the other: the render keeps the POOL's style,
/// so the entry's lines arrive in the pool's endings, and its keys and
/// values read unchanged.
#[test]
fn ip1_a_pool_of_one_ending_style_and_a_profile_file_of_the_other_keep_the_pools_style() {
    for (pool_eol, profiles_eol) in [("\r\n", "\n"), ("\n", "\r\n")] {
        let (pool, profiles) = (ip1_pool(pool_eol), ip1_profiles(profiles_eol));
        let render = render_profile(&pool, &profiles, "default");
        assert_one_ending_style(&render, pool_eol);
        let sections = ip1_sections(&render);
        assert_eq!(
            sections
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>(),
            ["*", "alpha", "beta"],
            "the sections are not these, in this order ({pool_eol:?} pool, \
                 {profiles_eol:?} profiles): {render:?}"
        );
        assert_eq!(
            sections[1].1,
            ip1_map(&[
                ("model", "/models/alpha.gguf"),
                ("temp", "0.2"),
                ("device", "cuda0"),
                ("ctx-size", "8192"),
            ]),
            "the entry's keys are not in the render, read as the reader reads them \
                 ({pool_eol:?} pool, {profiles_eol:?} profiles): {render:?}"
        );
    }
}

/// `ip1` criterion (review D6c): a model profile the pool holds and this
/// instance profile has no entry for is NOT served: its section is nowhere
/// in the render, and the profile that does have the entry serves it.
#[test]
fn ip1_a_model_of_the_pool_with_no_entry_of_the_profile_is_not_served() {
    for eol in EOLS {
        let (pool, profiles) = (ip1_pool(eol), ip1_profiles(eol));
        let render = render_profile(&pool, &profiles, "default");
        let (_, body) = parts(&render);
        assert_eq!(
            headers(&body),
            ["*", "alpha", "beta"],
            "a model profile of the pool with no [default/gamma] entry is served \
                 ({eol:?}): {render:?}"
        );
        let gamma = section(&pool, "gamma")
            .trim_end_matches(['\r', '\n'])
            .to_string();
        assert!(
            !body.contains(&gamma),
            "the section of the model profile with no entry of this profile is in the \
                 render ({eol:?}): {body:?}"
        );
        let other = render_profile(&pool, &profiles, "other");
        assert_eq!(
            headers(&parts(&other).1),
            ["*", "gamma"],
            "the profile that has the entry does not serve it ({eol:?}): {other:?}"
        );
    }
}
