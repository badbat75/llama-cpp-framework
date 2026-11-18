//! The body of `PUT /api/profiles/model/{id}` on its own: the new text of the preset
//! file, a 400, or a 422, out of the request body, the text of the file and the
//! preset's id. No manager, no session, no route and no write: those belong to
//! the rest of "Saving a preset from the web" in `ARCHITECTURE.md`. The web page
//! that sends the body is `fe/briefs/02h-preset-save.md`, its contract
//! `fe/briefs/api-contract.md`.
//!
//! The rules these tests pin, the design's "Done when" for this step and its
//! rules for the body: a body that changes nothing gives the file text back byte
//! for byte; a key that is set replaces only its line, the comment above it
//! kept, and the rewritten line is spelled `key = value` (`set_key`'s rule), so
//! the fixture spells its key lines compactly (`key=value`) to show the
//! untouched lines keeping their bytes while an edited line is rewritten whole;
//! a key the section does not hold is appended after its last real line, and
//! several such keys land in key order whatever the order of the body and of the
//! `values` map, because the bff applies the edits sorted by key (a
//! `serde_json::Map` is a `BTreeMap` sorted by key in this crate's build and
//! insertion-ordered once the workspace turns `preserve_order` on through
//! `pai-app`, so the bytes written must not depend on it); a key that
//! is null removes every line of that key in the section named
//! by `id`; a key the web list does not hold is 400 and nothing else of the body
//! is applied; that list is `pai_core::checks::WEB_PRESET_KEYS` (this crate
//! keeps no copy of it), matched in the exact spelling, so a key of the list in
//! another letter case is 400 like an unknown one, and the keys of
//! `checks::PRESET_KEYS_NOT_ON_WEB` are 400 with it; a value with a control
//! character other than a tab, an empty value after trimming spaces and tabs, a
//! value whose first or last character is a Unicode space other than a space or
//! a tab (the reader trims every Unicode space, so it would come back shorter
//! than it went), a value that is neither a string nor `null` and a body with no
//! string `revision` are 400; a value with a `;` or a `#` is 422
//! `comment-markers` with the field the key.
//!
//! What the tests assume, because the design leaves it to the implementation:
//! the trimmed value is what lands in the file; the line a set writes spells the
//! key as the body does (a key of the file in another letter case counts as the
//! key's occurrence and goes); when more than one value holds a comment marker
//! every such key is listed, in no order of its own; a `revision` that is not a
//! revision of the file is still the text answer, the save that reads the file
//! back refusing the stale edit; and the checks of one value are not pinned in
//! their order, so a value that two checks would refuse is not tested.

use crate::presets::{body_to_text, PresetBodyAnswer};
use pai_core::checks::{PRESET_KEYS_NOT_ON_WEB, WEB_PRESET_KEYS};

/// The revision every test body carries. A real revision is a keyed hash the
/// read model computes over the file's name and the section's text; the body
/// only carries it through, so a plain string stands in for it.
const REV: &str = "r1";

/// The preset file the tests start from: a `[main]` section carrying the project
/// URL comment, a comment above one key, a comment between two keys and a
/// comment after the last key, and a `[other]` section with the same keys. Every
/// key of one section holds a value of its own, so an edit of one section is
/// visible against the other. Two keys of the fixture (`model`, `n-cpu-moe`) are
/// keys the web is never asked to write, so they show as untouched lines. `nl`
/// is the file's line ending, tested with LF and with CRLF. Every key line is
/// spelled compactly, so a rewritten line is visible as `key = value` and an
/// untouched one keeps the compact bytes.
fn file(nl: &str) -> String {
    [
        "; the presets of this machine",
        "",
        "[main]",
        "; url = https://example.test/main",
        "; what the model runs on",
        "ctx-size=8192",
        "model=/models/main.gguf",
        "n-gpu-layers=99",
        "chat-template-kwargs=main-kw",
        "# a comment between two keys",
        "pooling=mean",
        "n-cpu-moe=7",
        "",
        "[other]",
        "ctx-size=4096",
        "model=/models/other.gguf",
        "n-gpu-layers=11",
        "chat-template-kwargs=other-kw",
        "pooling=rank",
        "n-cpu-moe=3",
        "# a comment after the last key",
    ]
    .join(nl)
        + nl
}

/// A value as JSON: a string, quoted and escaped, `null` for a removed key.
fn json_of(value: Option<&str>) -> String {
    match value {
        Some(value) => serde_json::to_string(value).unwrap(),
        None => "null".to_string(),
    }
}

/// The JSON body of a save: `revision` (None writes `null`), and `values`, a
/// `null` value removing that key.
fn body(revision: Option<&str>, values: &[(&str, Option<&str>)]) -> String {
    let mut pairs = String::new();
    for (key, value) in values {
        if !pairs.is_empty() {
            pairs.push(',');
        }
        pairs.push_str(&serde_json::to_string(key).unwrap());
        pairs.push(':');
        pairs.push_str(&json_of(*value));
    }
    body_json(&json_of(revision), &pairs)
}

/// The same object with `revision` and `values` written as given: for a body
/// whose `revision` is not a string and whose `values` is not an object.
fn body_json(revision: &str, values: &str) -> String {
    format!("{{\"revision\":{revision},\"values\":{{{values}}}}}")
}

/// The new text of a save, with the revision the body carried. It panics with
/// the body and the answer when the save was refused.
fn text_of(body: &str, file_text: &str, id: &str, revision: &str) -> String {
    match body_to_text(body.as_bytes(), file_text, id) {
        PresetBodyAnswer::Text {
            text,
            revision: got,
        } => {
            assert_eq!(
                got, revision,
                "the answer must carry the revision the body held, so the save goes to the \
                 file that still holds it: body {body:?}"
            );
            text
        }
        answer => panic!("expected the new text of the file, got {answer:?} for body {body:?}"),
    }
}

/// The 422 of a save: the check's key, its values and the field the form marks.
/// It panics with the body when the save was not refused.
fn refused_of(body: &str, file_text: &str, id: &str) -> (String, Vec<String>, String) {
    match body_to_text(body.as_bytes(), file_text, id) {
        PresetBodyAnswer::Refused { key, args, field } => (key, args, field),
        answer => panic!("expected a refusal, got {answer:?} for body {body:?}"),
    }
}

/// A body that must be refused with 400 `model-profiles-malformed`.
fn assert_malformed(body: &str, file_text: &str, id: &str) {
    let answer = body_to_text(body.as_bytes(), file_text, id);
    assert!(
        matches!(answer, PresetBodyAnswer::Malformed),
        "expected 400 model-profiles-malformed, got {answer:?} for body {body:?}"
    );
}

/// The whole file with the line `from` replaced by the line `to`: every key of
/// the fixture holds a value of its own in each section, so the line is unique
/// to the section it names.
fn replaced(file_text: &str, nl: &str, from: &str, to: &str) -> String {
    file_text.replace(&format!("{from}{nl}"), &format!("{to}{nl}"))
}

/// The whole file with the line `line` taken out.
fn dropped(file_text: &str, nl: &str, line: &str) -> String {
    file_text.replace(&format!("{line}{nl}"), "")
}

#[test]
fn preset_body_unchanged_is_the_file() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let out = text_of(&body(Some(REV), &[]), &file_text, "main", REV);
        assert_eq!(
            out, file_text,
            "a body that changes nothing gives the file text back byte for byte ({nl:?})"
        );
    }
}

#[test]
fn preset_body_set_key_replaces_only_its_line() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let out = text_of(
            &body(Some(REV), &[("ctx-size", Some("16384"))]),
            &file_text,
            "main",
            REV,
        );
        // The whole file, byte for byte, with one line of [main] changed: the
        // rewritten line is spelled `key = value`, every other line keeps its
        // bytes, compact spelling included.
        assert_eq!(
            out,
            replaced(&file_text, nl, "ctx-size=8192", "ctx-size = 16384"),
            "a set key replaces its own line and nothing else ({nl:?})"
        );
        // The comments of the section, spelled out: the one above the key that
        // changed, the project URL, the one between two keys, the one after the
        // last key, the file's own header, and a line of the other section.
        for kept in [
            "; what the model runs on",
            "; url = https://example.test/main",
            "# a comment between two keys",
            "# a comment after the last key",
            "; the presets of this machine",
            "chat-template-kwargs=other-kw",
        ] {
            assert!(
                out.contains(kept),
                "the line {kept:?} must survive ({nl:?})"
            );
        }
        assert_eq!(
            out.matches("ctx-size=").count(),
            1,
            "the compact `ctx-size=` spelling survives on exactly one line, so [main]'s line \
             was rewritten and [other]'s line at 4096 is untouched ({nl:?})"
        );
        assert_eq!(
            out.matches("ctx-size = 16384").count(),
            1,
            "the rewritten [main] line is written once, as `key = value` ({nl:?})"
        );
        assert!(
            out.contains(&format!("ctx-size=4096{nl}")),
            "[other]'s compact line must survive byte for byte ({nl:?})"
        );
    }
}

#[test]
fn preset_body_null_removes_every_line_of_the_key() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        for (key, line) in [
            ("n-gpu-layers", "n-gpu-layers=99"),
            ("pooling", "pooling=mean"),
        ] {
            let out = text_of(&body(Some(REV), &[(key, None)]), &file_text, "main", REV);
            assert!(
                !out.contains(line),
                "the {key} line of [main] must be gone ({nl:?})"
            );
            // The whole file, byte for byte, without that line: the comment
            // above the removed key stays, as do the file's other bytes.
            assert_eq!(
                out,
                dropped(&file_text, nl, line),
                "a null value removes the lines of {key} in [main] and nothing else ({nl:?})"
            );
        }
    }
}

#[test]
fn preset_body_applies_every_change_of_the_body() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let body = body(
            Some(REV),
            &[
                ("ctx-size", Some("16384")),
                ("n-gpu-layers", None),
                ("chat-template-kwargs", Some("{{\"a\": 1}}")),
                ("pooling", Some("rank")),
            ],
        );
        let out = text_of(&body, &file_text, "main", REV);
        let want = dropped(
            &replaced(
                &replaced(
                    &replaced(&file_text, nl, "ctx-size=8192", "ctx-size = 16384"),
                    nl,
                    "chat-template-kwargs=main-kw",
                    "chat-template-kwargs = {{\"a\": 1}}",
                ),
                nl,
                "pooling=mean",
                "pooling = rank",
            ),
            nl,
            "n-gpu-layers=99",
        );
        assert_eq!(
            out, want,
            "every change of one body is applied in one pass ({nl:?})"
        );
    }
}

#[test]
fn preset_body_two_changes_to_one_key_give_the_last() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let body = body(
            Some(REV),
            &[
                ("ctx-size", Some("1")),
                ("ctx-size", Some("2")),
                ("ctx-size", Some("16384")),
            ],
        );
        let out = text_of(&body, &file_text, "main", REV);
        assert_eq!(
            out,
            replaced(&file_text, nl, "ctx-size=8192", "ctx-size = 16384"),
            "two changes to one key give the last of them, once ({nl:?})"
        );
    }
}

#[test]
fn preset_body_edits_only_the_section_named_by_id() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let body = body(
            Some(REV),
            &[
                ("ctx-size", Some("16384")),
                ("chat-template-kwargs", Some("fresh")),
                ("pooling", None),
            ],
        );
        // The section is matched in any letter case, as the read model's ids are.
        for id in ["main", "MAIN", "Main"] {
            let out = text_of(&body, &file_text, id, REV);
            assert_eq!(
                out,
                dropped(
                    &replaced(
                        &replaced(&file_text, nl, "ctx-size=8192", "ctx-size = 16384",),
                        nl,
                        "chat-template-kwargs=main-kw",
                        "chat-template-kwargs = fresh",
                    ),
                    nl,
                    "pooling=mean",
                ),
                "the edits go to the section named by {id:?} and to no other ({nl:?})"
            );
        }
    }
}

#[test]
fn preset_body_key_case_and_duplicates_give_one_line() {
    for nl in ["\n", "\r\n"] {
        // A hand-edited file: the key in another letter case, and the key twice.
        let file_text =
            format!("; the presets{nl}[main]{nl}CTX-SIZE = 1{nl}pooling=mean{nl}ctx-size=8192{nl}");
        let out = text_of(
            &body(Some(REV), &[("ctx-size", Some("16384"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            format!("; the presets{nl}[main]{nl}ctx-size = 16384{nl}pooling=mean{nl}"),
            "every line of the key goes, in any letter case, and one line remains at the first \
             occurrence's place ({nl:?})"
        );
    }
}

#[test]
fn preset_body_accepts_every_key_of_the_web_list() {
    // The list is `pai-core`'s, pinned against the configurator's writer by the
    // `pai-app` test `web_key_lists_are_exactly_what_render_section_writes`, so
    // a key the page can write is accepted here with no list to keep in step.
    for &key in WEB_PRESET_KEYS {
        let file_text = format!("; the presets\n[main]\n{key}=old\n");
        let out = text_of(
            &body(Some(REV), &[(key, Some("12"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            format!("; the presets\n[main]\n{key} = 12\n"),
            "every key of the web's list is accepted, and its line only is written"
        );
    }
}

#[test]
fn preset_body_key_in_another_case_is_malformed() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        // A key of the web's list, spelled in another letter case: the list is
        // matched in exact case, because llama-server knows a key only in its
        // own case and refuses a preset that spells it otherwise.
        for key in ["CTX-SIZE", "Ctx-Size", "POOLING"] {
            // The body also sets a key the list does hold in its own case: 400
            // refuses the whole body, so nothing of it is applied.
            let body = body(Some(REV), &[("ctx-size", Some("16384")), (key, Some("12"))]);
            assert_malformed(&body, &file_text, "main");
        }
    }
}

#[test]
fn preset_body_unknown_key_is_malformed() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        // The keys of `PRESET_KEYS_NOT_ON_WEB` (the configurator writes them, no
        // control of the page enables them yet) and a name no reader knows. The
        // body also sets a key the web list does hold: 400 refuses the whole
        // body, so nothing of it is applied.
        for key in PRESET_KEYS_NOT_ON_WEB.iter().chain(std::iter::once(&"foo")) {
            let body = body(Some(REV), &[("ctx-size", Some("16384")), (key, Some("12"))]);
            assert_malformed(&body, &file_text, "main");
        }
    }
}

#[test]
fn preset_body_control_character_is_malformed() {
    // CR and LF would end the line, and no reader writes them; a tab is not one
    // of these, so the next test sends it.
    for value in ["8\n096", "8\r096", "8\u{0}096", "8\u{7}096", "8\u{1b}096"] {
        let body = body(Some(REV), &[("ctx-size", Some(value))]);
        assert_malformed(&body, &file("\n"), "main");
    }
}

#[test]
fn preset_body_tab_inside_a_value_is_written() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        let out = text_of(
            &body(Some(REV), &[("chat-template-kwargs", Some("a\tb"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            replaced(
                &file_text,
                nl,
                "chat-template-kwargs=main-kw",
                "chat-template-kwargs = a\tb"
            ),
            "a tab is not a control character to this rule ({nl:?})"
        );
    }
}

#[test]
fn preset_body_empty_value_is_malformed() {
    for key in ["ctx-size", "fit-target", "pooling"] {
        for value in ["", "   ", "\t ", " \n "] {
            let body = body(Some(REV), &[(key, Some(value))]);
            assert_malformed(&body, &file("\n"), "main");
        }
    }
}

#[test]
fn preset_body_unicode_space_at_the_ends_is_malformed() {
    // A value that starts or ends with a Unicode space that is not a space or a
    // tab: the writer trims only spaces and tabs, so the byte lands in the file,
    // while the reader trims the line and the value with `str::trim`, which
    // knows every Unicode space (the parser in `pai_core::ini`). Such a value
    // would be read back shorter than what was written, so it is refused here.
    // The body also sets a key the web's list holds: 400 refuses the whole
    // body, so nothing of it is applied.
    for value in [
        "\u{a0}16384",
        "16384\u{a0}",
        "\u{2028}16384",
        "16384\u{2028}",
        "\u{a0}",
        "\u{2028}",
    ] {
        let body = body(
            Some(REV),
            &[("pooling", Some("rank")), ("ctx-size", Some(value))],
        );
        assert_malformed(&body, &file("\n"), "main");
    }
}

#[test]
fn preset_body_unicode_space_inside_a_value_is_written() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        // The rule is about the ends only: a Unicode space in the middle of a
        // value survives the reader's trim, so it is written as it is.
        let out = text_of(
            &body(Some(REV), &[("chat-template-kwargs", Some("a\u{a0}b"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            replaced(
                &file_text,
                nl,
                "chat-template-kwargs=main-kw",
                "chat-template-kwargs = a\u{a0}b"
            ),
            "a Unicode space inside a value is not at its ends, so the rule does not reach \
             it ({nl:?})"
        );
    }
}

#[test]
fn preset_body_value_neither_a_string_nor_null_is_malformed() {
    // A `null` is a removal, so it is not in this list.
    for raw in ["12", "1.5", "true", "[]", "{}", "[\"mean\"]"] {
        let body = format!("{{\"revision\":\"{REV}\",\"values\":{{\"ctx-size\":{raw}}}}}");
        assert_malformed(&body, &file("\n"), "main");
    }
}

#[test]
fn preset_body_revision_missing_or_not_a_string_is_malformed() {
    assert_malformed("{\"values\":{\"ctx-size\":\"16384\"}}", &file("\n"), "main");
    for raw in ["12", "1.5", "true", "null", "[]", "{}"] {
        let body = body_json(raw, "\"ctx-size\":\"16384\"");
        assert_malformed(&body, &file("\n"), "main");
    }
}

#[test]
fn preset_body_values_missing_or_not_an_object_is_malformed() {
    assert_malformed(&format!("{{\"revision\":\"{REV}\"}}"), &file("\n"), "main");
    for raw in ["[]", "[\"ctx-size\"]", "\"ctx-size\"", "12", "null"] {
        let body = body_json(&json_of(Some(REV)), raw);
        assert_malformed(&body, &file("\n"), "main");
    }
}

#[test]
fn preset_body_not_json_is_malformed() {
    for raw in [
        "",
        "   ",
        "ctx-size=16384",
        "{",
        "{\"revision\":\"r1\"",
        "{\"revision\":\"r1\",\"values\":{}}]",
    ] {
        assert_malformed(raw, &file("\n"), "main");
    }
}

#[test]
fn preset_body_value_with_a_comment_marker_is_refused() {
    // The check runs for every key the body sets, not only for a key whose value
    // the manager checks as text, so a number key and a text key answer alike.
    for (key, value) in [
        ("ctx-size", "8192; run"),
        ("fit-target", "1024 # MiB"),
        ("chat-template-kwargs", "{\"a\": \"#1\"}"),
    ] {
        let body = body(Some(REV), &[(key, Some(value))]);
        let (refusal_key, args, field) = refused_of(&body, &file("\n"), "main");
        assert_eq!(
            refusal_key, "comment-markers",
            "a `;` or a `#` in a value is the comment-marker refusal: body {body:?}"
        );
        assert_eq!(
            args,
            [key.to_string()],
            "the refusal names the key whose value holds the marker: body {body:?}"
        );
        assert_eq!(
            field, key,
            "the 422 marks the control of the key it refused: body {body:?}"
        );
    }
}

#[test]
fn preset_body_lists_every_key_with_a_comment_marker() {
    let body = body(
        Some(REV),
        &[
            ("ctx-size", Some("a;b")),
            ("fit-target", Some("c#d")),
            ("pooling", Some("clean")),
        ],
    );
    let (refusal_key, args, _) = refused_of(&body, &file("\n"), "main");
    assert_eq!(refusal_key, "comment-markers", "body {body:?}");
    // The order of the list is not pinned: a JSON object read by serde_json
    // keeps no order of its own, so the keys are compared sorted.
    let mut listed = args.clone();
    listed.sort();
    assert_eq!(
        listed,
        ["ctx-size".to_string(), "fit-target".to_string()],
        "when more than one value holds a marker every one of those keys is listed, and a clean \
         key is not: body {body:?}"
    );
}

#[test]
fn preset_body_value_is_trimmed() {
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        // Spaces and tabs around the value only: a control character inside a
        // value is 400, so it has nothing else to be trimmed to.
        let out = text_of(
            &body(
                Some(REV),
                &[
                    ("ctx-size", Some("  16384  ")),
                    ("chat-template-kwargs", Some("\t fresh ")),
                ],
            ),
            &file_text,
            "main",
            REV,
        );
        let want = replaced(
            &replaced(&file_text, nl, "ctx-size=8192", "ctx-size = 16384"),
            nl,
            "chat-template-kwargs=main-kw",
            "chat-template-kwargs = fresh",
        );
        assert_eq!(
            out, want,
            "a value is trimmed of spaces and tabs before it is written, as the configurator's \
             set does ({nl:?})"
        );
    }
}

#[test]
fn preset_body_two_new_keys_land_after_the_last_line() {
    /// The file with the lines of `keys` appended to `[main]`, after its last
    /// real line and before the blank line that separates the sections: the
    /// whole change, every other byte of the file kept.
    fn appended(file_text: &str, nl: &str, keys: &[(&str, &str)]) -> String {
        let mut lines = "n-cpu-moe=7".to_string();
        for (key, value) in keys {
            lines.push_str(&format!("{nl}{key} = {value}"));
        }
        replaced(file_text, nl, "n-cpu-moe=7", &lines)
    }
    for nl in ["\n", "\r\n"] {
        let file_text = file(nl);
        // Two keys the section does not hold, spelled in the body in their key
        // order (`temp` before `top-k`).
        let out = text_of(
            &body(Some(REV), &[("temp", Some("0.9")), ("top-k", Some("40"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            appended(&file_text, nl, &[("temp", "0.9"), ("top-k", "40")]),
            "the two new lines land after [main]'s last real line, in key order, and before \
             the blank line that separates the sections ({nl:?})"
        );
        // The same two keys in the reverse key order. The bff applies the edits
        // sorted by key, so the text is the one above byte for byte: the map of
        // `values` is serde_json's, a `BTreeMap` sorted by key in this crate's
        // build and the body's order once the workspace turns `preserve_order`
        // on through `pai-app`, and the bytes of the file must not depend on
        // that, nor on the order of the body.
        let out = text_of(
            &body(Some(REV), &[("top-k", Some("40")), ("temp", Some("0.9"))]),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            appended(&file_text, nl, &[("temp", "0.9"), ("top-k", "40")]),
            "a body listing the two new keys in reverse key order writes them in key order \
             ({nl:?})"
        );
        // Three new keys in an order of its own: the edits are sorted by key,
        // not put back into the one order a two-key body happens to allow.
        let out = text_of(
            &body(
                Some(REV),
                &[
                    ("top-k", Some("40")),
                    ("min-p", Some("0.1")),
                    ("temp", Some("0.9")),
                ],
            ),
            &file_text,
            "main",
            REV,
        );
        assert_eq!(
            out,
            appended(
                &file_text,
                nl,
                &[("min-p", "0.1"), ("temp", "0.9"), ("top-k", "40")]
            ),
            "the new keys land in key order, whatever the order of the body ({nl:?})"
        );
    }
}

#[test]
fn preset_body_revision_is_carried_through() {
    let file_text = file("\n");
    // The revision of the card is carried out unchanged, so the save goes to the
    // file that still holds it.
    let out = text_of(
        &body(Some(REV), &[("ctx-size", Some("16384"))]),
        &file_text,
        "main",
        REV,
    );
    assert_eq!(
        out,
        replaced(&file_text, "\n", "ctx-size=8192", "ctx-size = 16384")
    );
    // A revision that is not one of this file is still the text answer: the
    // save that reads the file back before it writes is the one that refuses a
    // stale edit, so the pure function answers 409 to nobody.
    match body_to_text(
        body(Some("r-elsewhere"), &[("ctx-size", Some("16384"))]).as_bytes(),
        &file_text,
        "main",
    ) {
        PresetBodyAnswer::Text { revision, .. } => assert_eq!(
            revision, "r-elsewhere",
            "the answer carries the body's revision, whoever reads it"
        ),
        answer => panic!("expected the text answer, got {answer:?}"),
    }
}
