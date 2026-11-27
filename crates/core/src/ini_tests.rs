use super::*;
use std::path::PathBuf;

fn ini_file(content: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.ini");
    fs::write(&path, content).unwrap();
    (dir, path)
}

#[test]
fn read_all_parses_sections_in_order() {
    let (_d, path) = ini_file(
            "; file comment\r\n[Server]\r\nPort = 8080\r\n\r\n[alpha]\r\nmodel = a.gguf\r\n# comment\r\nctx-size = 4096\r\n",
        );
    let sections = read_all(&path);
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].id, "Server");
    assert_eq!(sections[0].keys["Port"], "8080");
    assert_eq!(sections[1].id, "alpha");
    assert_eq!(sections[1].keys["ctx-size"], "4096");
    assert!(!sections[1].keys.contains_key("# comment"));
}

#[test]
fn read_section_is_case_insensitive() {
    let (_d, path) = ini_file("[server]\r\nPort = 1234\r\n");
    assert_eq!(read_section(&path, "Server")["Port"], "1234");
}

#[test]
fn strip_inline_comment_cases() {
    assert_eq!(strip_inline_comment("8080 ; note"), "8080");
    assert_eq!(strip_inline_comment("8080 # note"), "8080");
    // llama-server's PEG cuts at the FIRST marker even unspaced: we must
    // read exactly what it would read, not keep a longer value it won't.
    assert_eq!(strip_inline_comment("a;b"), "a");
    assert_eq!(strip_inline_comment("x ;y"), "x");
    assert_eq!(strip_inline_comment("q8#0"), "q8");
    assert_eq!(strip_inline_comment("plain  "), "plain");
}

// The refusal names the field and the text, and it is exactly the complement
// of `parse_int_in`: overflow, blank, garbage, a fraction and an out-of-range
// value are refused; a padded number inside the range is not.
#[test]
fn int_problem_refuses_exactly_what_parse_int_in_rejects() {
    let cases = [
        ("196608245760", &INT_POSITIVE, false),
        ("", &INT_POSITIVE, false),
        ("  ", &INT_ANY, false),
        ("abc", &INT_ANY, false),
        ("1.5", &INT_ANY, false),
        ("0", &INT_POSITIVE, false),
        ("-1", &INT_NON_NEGATIVE, false),
        ("0", &INT_NON_NEGATIVE, true),
        ("-1", &INT_ANY, true),
        (" 245760 ", &INT_POSITIVE, true),
    ];
    for (text, range, ok) in cases {
        assert_eq!(parse_int_in(text, range).is_some(), ok, "{text:?}");
        assert_eq!(
            int_problem("ctx-size", text, range).is_none(),
            ok,
            "{text:?}"
        );
    }
    assert_eq!(
        int_problem("ctx-size", "196608245760", &INT_POSITIVE).unwrap(),
        "ctx-size = `196608245760` is not a whole number from 1 to 2147483647"
    );
    assert_eq!(
        int_problem("Port", " ", &(1..=65535)).unwrap(),
        "Port is empty, expected a whole number from 1 to 65535"
    );
}

#[test]
fn parse_float_accepts_dot_or_comma_separator() {
    // Period: the canonical form llama-server's CLI expects.
    assert_eq!(parse_float("0.5"), Some(0.5));
    assert_eq!(parse_float("1.25"), Some(1.25));
    // Comma: the natural separator on a comma-decimal keyboard
    // (Italian/German/French). Must normalize to the same value so the
    // sampling knobs are editable on those layouts.
    assert_eq!(parse_float("0,5"), Some(0.5));
    assert_eq!(parse_float("1,25"), Some(1.25));
    // Surrounding whitespace is tolerated, blank/garbage yields None.
    assert_eq!(parse_float("  0,7  "), Some(0.7));
    assert_eq!(parse_float(""), None);
    assert_eq!(parse_float("abc"), None);
}

#[test]
fn replace_key_updates_existing_line() {
    let (_d, path) = ini_file("[Server]\r\nPort = 8080\r\nHostname = localhost\r\n");
    replace_key(&path, "Server", "Port", "9090").unwrap();
    let keys = read_section(&path, "Server");
    assert_eq!(keys["Port"], "9090");
    assert_eq!(keys["Hostname"], "localhost");
}

#[test]
fn replace_key_appends_missing_key_to_section() {
    let (_d, path) = ini_file("[Server]\r\nPort = 8080\r\n\r\n[other]\r\nk = v\r\n");
    replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
    // Exact content: the new line slots in after the last real line, and
    // the single blank separator line stays single (the append path once
    // duplicated the terminator, growing a blank line per appended key).
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[Server]\r\nPort = 8080\r\nModelsDir = C:/models\r\n\r\n[other]\r\nk = v\r\n"
    );
}

// next_section_start only recognizes a header AFTER a '\n' at/beyond
// `from`, so scanning from the body start missed a neighbouring header
// sitting exactly there: appending to an empty-bodied section spliced the
// key into the NEXT section instead (and read_section still saw nothing).
#[test]
fn replace_key_respects_empty_section_body() {
    let (_d, path) = ini_file("[Server]\r\n[other]\r\nk = 2\r\n");
    replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[Server]\r\nModelsDir = C:/models\r\n[other]\r\nk = 2\r\n"
    );
}

// A header that is the file's last line (no terminator) used to have the
// key glued straight onto the `]`, destroying the header line.
#[test]
fn replace_key_handles_header_without_trailing_newline() {
    let (_d, path) = ini_file("[Server]");
    replace_key(&path, "Server", "Port", "8080").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[Server]\r\nPort = 8080\r\n"
    );
}

// Replacing an UNTERMINATED final line has no ending to preserve: it
// must fall back to the file's detected EOL, not hardcode `\n` and mix
// endings into a CRLF file.
#[test]
fn replace_key_keeps_crlf_when_replacing_unterminated_last_line() {
    let (_d, path) = ini_file("[Server]\r\nModelsDir = C:/old");
    replace_key(&path, "Server", "ModelsDir", "C:/new").unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "[Server]\r\nModelsDir = C:/new\r\n"
    );
}

#[test]
fn replace_key_creates_missing_section_and_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("new.ini");
    replace_key(&path, "Server", "Port", "8080").unwrap();
    assert_eq!(read_section(&path, "Server")["Port"], "8080");
}

#[test]
fn replace_key_matches_section_case_insensitively() {
    // A hand-edited lowercase header must not produce a duplicate section.
    let (_d, path) = ini_file("[server]\r\nPort = 8080\r\n");
    replace_key(&path, "Server", "Port", "9090").unwrap();
    let sections = read_all(&path);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].keys["Port"], "9090");
}

#[test]
fn replace_key_preserves_lf_only_files() {
    let (_d, path) = ini_file("[Server]\nPort = 8080\nHostname = localhost\n");
    replace_key(&path, "Server", "Hostname", "0.0.0.0").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("Hostname = 0.0.0.0\n"));
    assert!(!content.contains("Hostname = 0.0.0.0\r\n"));
    // The insert path too: appending a missing key must not smuggle a CRLF
    // into an LF-only file.
    replace_key(&path, "Server", "ModelsDir", "C:/models").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains('\r'), "LF-only file gained a CR");
    assert_eq!(read_section(&path, "Server")["ModelsDir"], "C:/models");
}

#[test]
fn replace_section_swaps_body_and_keeps_neighbours() {
    let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
    replace_section(&path, "a", "[a]\r\nk = 9\r\nextra = yes\r\n").unwrap();
    let keys = read_section(&path, "a");
    assert_eq!(keys["k"], "9");
    assert_eq!(keys["extra"], "yes");
    assert_eq!(read_section(&path, "b")["k"], "2");
}

#[test]
fn replace_section_appends_when_missing() {
    let (_d, path) = ini_file("[a]\r\nk = 1\r\n");
    replace_section(&path, "b", "[b]\r\nk = 2\r\n").unwrap();
    assert_eq!(read_section(&path, "b")["k"], "2");
    assert_eq!(read_section(&path, "a")["k"], "1");
}

#[test]
fn replace_section_preserves_lf_only_files() {
    let (_d, path) = ini_file("[a]\nk = 1\n");
    // Both paths (replacing an existing section and appending a new one)
    // must convert the CRLF-rendered body to the file's LF-only style.
    replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
    replace_section(&path, "b", "[b]\r\nk = 2\r\n").unwrap();
    let content = fs::read_to_string(&path).unwrap();
    assert!(
        !content.contains('\r'),
        "LF-only file gained a CR: {content:?}"
    );
    assert_eq!(read_section(&path, "a")["k"], "9");
    assert_eq!(read_section(&path, "b")["k"], "2");
}

#[test]
fn rename_section_basic_and_conflict() {
    let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
    rename_section(&path, "a", "c").unwrap();
    assert_eq!(read_section(&path, "c")["k"], "1");
    assert!(rename_section(&path, "c", "b").is_err());
    assert!(rename_section(&path, "missing", "x").is_err());
}

#[test]
fn rename_section_allows_case_only_rename() {
    let (_d, path) = ini_file("[qwen]\r\nk = 1\r\n");
    rename_section(&path, "qwen", "Qwen").unwrap();
    let sections = read_all(&path);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].id, "Qwen");
}

#[test]
fn delete_section_removes_only_target() {
    let (_d, path) = ini_file("[a]\r\nk = 1\r\n\r\n[b]\r\nk = 2\r\n");
    delete_section(&path, "a").unwrap();
    let sections = read_all(&path);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].id, "b");
    // Deleting a missing section (or from a missing file) is a no-op.
    delete_section(&path, "nope").unwrap();
    delete_section(Path::new("does/not/exist.ini"), "a").unwrap();
}

// The move primitive behind a model profile's on/off switch: the text must come out
// exactly as written (a hand comment and an unknown key included), stop at
// the next header, and splice into another file as that same section.
#[test]
fn section_text_moves_a_section_verbatim() {
    let (_d, path) = ini_file("[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n\r\n  [B]\r\nk = 2\r\n");
    let a = section_text(&path, "A").expect("case-insensitive lookup");
    assert_eq!(a, "[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n\r\n");
    // An indented header comes out whole, indentation included.
    assert_eq!(section_text(&path, "b").unwrap(), "  [B]\r\nk = 2\r\n");
    assert!(section_text(&path, "nope").is_none());
    assert!(section_text(Path::new("does/not/exist.ini"), "a").is_none());

    let (_d2, other) = ini_file("[c]\r\nk = 3\r\n");
    replace_section(&other, "a", &a).unwrap();
    let moved = fs::read_to_string(&other).unwrap();
    assert!(
        moved.contains("[a]\r\n; mine\r\nk = 1\r\nodd-key = x\r\n"),
        "{moved}"
    );
    assert_eq!(read_section(&other, "a")["odd-key"], "x");
    assert_eq!(read_section(&other, "c")["k"], "3");
}

// The reader trims each line before the `[` check, so a hand-indented
// header IS a section: the writers' boundary scan must honor the same
// rule or a neighbouring rewrite silently swallows it.
#[test]
fn writers_respect_indented_section_headers() {
    let (_d, path) = ini_file("[a]\r\nk = 1\r\n  [b]\r\nk = 2\r\n");

    // Rewriting [a] must stop at the indented [b], not absorb it.
    replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
    assert_eq!(read_section(&path, "a")["k"], "9");
    assert_eq!(read_section(&path, "b")["k"], "2");

    // A key appended to [a] must land in [a], not fall through into [b].
    replace_key(&path, "a", "extra", "3").unwrap();
    assert_eq!(read_section(&path, "a")["extra"], "3");
    assert!(!read_section(&path, "b").contains_key("extra"));

    // The indented header itself is addressable...
    replace_section(&path, "b", "[b]\r\nk = 7\r\n").unwrap();
    assert_eq!(read_section(&path, "b")["k"], "7");

    // ...and deleting [a] leaves [b] intact.
    delete_section(&path, "a").unwrap();
    let sections = read_all(&path);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].id, "b");
}

#[test]
fn reject_comment_markers_matches_the_strip_rule() {
    assert!(reject_comment_markers("f", r"C:\Models\plain").is_ok());
    for hostile in [r"C:\Models #1", r"C:\a;b", "x#y"] {
        let err = reject_comment_markers("ModelsDir", hostile).expect_err(hostile);
        assert!(
            err.to_string().contains("ModelsDir"),
            "error names the field"
        );
        // The guard exists precisely because the reader would truncate it.
        assert_ne!(strip_inline_comment(hostile), hostile);
    }
}

// read_all trims the NAME inside the brackets, so a hand-edited `[ a ]`
// lists as section `a`. The writers must find it there too: matching the
// whole line against `[a]` missed it, so replace_section appended a fresh
// duplicate `[a]` and every later read returned the stale first copy
// ("edits never stick").
#[test]
fn writers_respect_internal_header_whitespace() {
    let (_d, path) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
    assert_eq!(read_section(&path, "a")["k"], "1");

    // Rewrite must hit the existing `[ a ]`, never spawn a second [a]…
    replace_section(&path, "a", "[a]\r\nk = 9\r\n").unwrap();
    let sections = read_all(&path);
    assert_eq!(sections.iter().filter(|s| s.id == "a").count(), 1);
    assert_eq!(read_section(&path, "a")["k"], "9");

    // …replace_key must land inside it…
    let (_d2, path2) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
    replace_key(&path2, "a", "extra", "3").unwrap();
    assert_eq!(read_section(&path2, "a")["extra"], "3");
    assert!(!read_section(&path2, "b").contains_key("extra"));

    // …rename must swap the REAL `[ a ]` span (no dangling ` ]`)…
    let (_d3, path3) = ini_file("[ a ]\r\nk = 1\r\n");
    rename_section(&path3, "a", "c").unwrap();
    assert_eq!(
        fs::read_to_string(&path3).unwrap(),
        "[c]\r\nk = 1\r\n",
        "rename must replace the actual header text"
    );

    // …and delete must remove it.
    let (_d4, path4) = ini_file("[ a ]\r\nk = 1\r\n[b]\r\nk = 2\r\n");
    delete_section(&path4, "a").unwrap();
    let sections = read_all(&path4);
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].id, "b");
}

/// The `; url = ` reader, with the cases the configurator has always
/// pinned (`crates/app/src/model_profiles.rs`): the value of the first url
/// comment, and empty for anything else. The web's section read answers
/// this same value, so the reader is in `pai-core` and both callers call
/// it (`crates/bff/tests/model_profile_read.rs`).
#[test]
fn project_url_of_reads_the_url_comment() {
    assert_eq!(
        project_url_of(&["; url = https://a.b/c".into()]),
        "https://a.b/c"
    );
    // `#` and no spacing around `=`: a hand-written line counts, and a
    // fragment is part of the URL.
    assert_eq!(
        project_url_of(&["#url=https://a.b/c#x".into()]),
        "https://a.b/c#x"
    );
    // Prose about urls is prose; a word starting with `url` is not `url`.
    assert_eq!(
        project_url_of(&["; urls are listed below".into(), "; Model: x".into()]),
        ""
    );
    assert_eq!(project_url_of(&["; url-ish = no".into()]), "");
    // The first url comment is the one, as the configurator shows it.
    assert_eq!(
        project_url_of(&["; url = first".into(), "; url = second".into()]),
        "first"
    );
    // No comment, no URL: the empty string, never a missing field.
    assert_eq!(project_url_of(&[]), "");
}

/// The reader takes the comments `read_all` collects, so a model profile file
/// read end to end answers its URL: the plumbing the section read uses.
#[test]
fn project_url_of_takes_the_comments_the_parser_collects() {
    let (_d, path) = ini_file(
            "; the file\r\n[alpha]\r\n; the router reads this file at launch\r\n; url = https://a.b/c\r\nmodel = a.gguf\r\n\r\n[b]\r\nmodel = b.gguf\r\n",
        );
    let sections = read_all(&path);
    assert_eq!(sections.len(), 2);
    assert_eq!(
        project_url_of(&sections[0].comments),
        "https://a.b/c",
        "a prose comment before it is skipped"
    );
    assert_eq!(
        project_url_of(&sections[1].comments),
        "",
        "a section without the comment"
    );
}

// ── The model profile save's in-place edits (text::set_key / text::remove_key) ──
//
// The web's save changes only the edited lines and keeps the rest of the
// file byte for byte: `set_key` rewrites one line, `remove_key` drops the
// key's lines, and a key written more than once collapses to the one line
// every reader reads.

/// A file of these lines, each one terminated by `eol`.
fn file_of(lines: &[&str], eol: &str) -> String {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push_str(eol);
    }
    out
}

/// The lines of `text` the writers count as `key`, the way `replace_key`
/// counts them.
fn key_line_count(text: &str, key: &str) -> usize {
    text.lines()
        .filter(|l| line_starts_with_key(l, key))
        .count()
}

#[test]
fn remove_key_drops_every_line_of_the_key() {
    let text = file_of(
        &[
            "[alpha]",
            "; tuned by hand",
            "ctx-size = 4096",
            "CTX-SIZE = 8192",
            "   ctx-size = 123",
            "ctx-size=7 ; note",
            "; ctx-size = 9",
            "ctx-size-draft = 512",
            "other = 1",
            "",
            "[beta]",
            "ctx-size = 42",
        ],
        "\r\n",
    );
    let out = text::remove_key(&text, "alpha", "ctx-size");
    assert_eq!(
        out,
        file_of(
            &[
                "[alpha]",
                "; tuned by hand",
                "; ctx-size = 9",
                "ctx-size-draft = 512",
                "other = 1",
                "",
                "[beta]",
                "ctx-size = 42",
            ],
            "\r\n",
        ),
        "every line of the key goes (any case, indented, unspaced around the \
             `=`) with its ending, and the comment naming it, the longer key, the \
             blank separator and the other section's own line all stay"
    );
    let sections = parse_sections(&out);
    assert!(!sections[0].keys.contains_key("ctx-size"));
    assert_eq!(sections[1].keys["ctx-size"], "42");
    assert_eq!(key_line_count(&out, "ctx-size"), 1);
}

#[test]
fn remove_key_keeps_the_files_line_ending_style() {
    for eol in ["\n", "\r\n"] {
        let text = file_of(
            &["[alpha]", "ctx-size = 4096", "ctx-size = 8192", "other = 1"],
            eol,
        );
        let out = text::remove_key(&text, "alpha", "ctx-size");
        assert_eq!(out, file_of(&["[alpha]", "other = 1"], eol));
        if eol == "\n" {
            assert!(!out.contains('\r'), "an LF-only file gained a CR");
        }
    }
}

#[test]
fn remove_key_of_an_absent_key_or_section_leaves_the_text_equal() {
    let text = file_of(
        &[
            "[alpha]",
            "; tuned by hand",
            "other = 1",
            "",
            "[beta]",
            "ctx-size = 42",
        ],
        "\r\n",
    );
    assert_eq!(
        text::remove_key(&text, "alpha", "ctx-size"),
        text,
        "the key is only in the other section"
    );
    assert_eq!(
        text::remove_key(&text, "gamma", "ctx-size"),
        text,
        "no such section"
    );
    assert_eq!(text::remove_key("", "alpha", "ctx-size"), "", "empty text");
}

// The file's last line has no ending of its own: the key's line goes with
// its ending, and the section header is never touched.
#[test]
fn remove_key_of_the_last_line_of_the_last_section() {
    for eol in ["\n", "\r\n"] {
        let with_break = file_of(&["[alpha]", "k = 1", "ctx-size = 4096"], eol);
        assert_eq!(
            text::remove_key(&with_break, "alpha", "ctx-size"),
            file_of(&["[alpha]", "k = 1"], eol),
            "the file keeps a final break"
        );
        let without = with_break.strip_suffix(eol).unwrap();
        assert_eq!(
            text::remove_key(without, "alpha", "ctx-size"),
            file_of(&["[alpha]", "k = 1"], eol),
            "a final line with no break of its own"
        );
        assert_eq!(
            text::remove_key(
                &file_of(&["[alpha]", "ctx-size = 4096"], eol),
                "alpha",
                "ctx-size"
            ),
            format!("[alpha]{eol}"),
            "the header stays"
        );
    }
}

// `read_all` trims the name inside the brackets and the line around them,
// so an indented `[ Alpha ]` IS the section the reader lists: the writers
// edit it there, and stop at the next (also indented) header.
#[test]
fn remove_key_takes_the_section_the_reader_finds() {
    let text = file_of(
        &[
            "  [ Alpha ]",
            "ctx-size = 4096",
            "k = 1",
            "  [beta]",
            "ctx-size = 42",
        ],
        "\r\n",
    );
    assert_eq!(
        text::remove_key(&text, "alpha", "ctx-size"),
        file_of(
            &["  [ Alpha ]", "k = 1", "  [beta]", "ctx-size = 42"],
            "\r\n"
        )
    );
}

#[test]
fn remove_key_edits_only_the_first_of_a_repeated_section() {
    // Two `[a]` headers are not mended here (the manager refuses the write
    // with `model-profile-name-taken`), so the second copy is left as it is.
    let text = file_of(&["[a]", "ctx-size = 1", "[A]", "ctx-size = 2"], "\r\n");
    assert_eq!(
        text::remove_key(&text, "a", "ctx-size"),
        file_of(&["[a]", "[A]", "ctx-size = 2"], "\r\n")
    );
}

#[test]
fn set_key_rewrites_only_the_edited_line() {
    let text = file_of(
        &[
            "[alpha]",
            "; keep me",
            "  Port   =   8080   ; hand note",
            "Host = localhost",
            "",
            "[beta]",
            "Port = 1",
        ],
        "\r\n",
    );
    assert_eq!(
        text::set_key(&text, "alpha", "Port", "9090"),
        file_of(
            &[
                "[alpha]",
                "; keep me",
                "Port = 9090",
                "Host = localhost",
                "",
                "[beta]",
                "Port = 1",
            ],
            "\r\n",
        ),
        "the edited line is rewritten whole as `key = value` (its indentation, \
             its spacing and its inline comment go with it, as the design says), \
             its own ending stays, the comment line above stays, and the same key \
             in the other section is untouched"
    );
}

#[test]
fn set_key_keeps_the_files_line_ending_style() {
    // Replacing an existing key and appending a new one in an LF-only file.
    let text = file_of(&["[alpha]", "Port = 8080", "", "[beta]", "k = 2"], "\n");
    let out = text::set_key(&text, "alpha", "Port", "9090");
    let out = text::set_key(&out, "alpha", "n-cpu-moe", "8");
    assert_eq!(
        out,
        file_of(
            &[
                "[alpha]",
                "Port = 9090",
                "n-cpu-moe = 8",
                "",
                "[beta]",
                "k = 2"
            ],
            "\n"
        )
    );
    // A rewritten final line keeps its OWN ending: an unterminated last
    // line stays unterminated, the rewrite only replaces the line's text.
    assert_eq!(
        text::set_key("[a]\r\nPort = 1", "a", "Port", "9"),
        "[a]\r\nPort = 9"
    );
    assert_eq!(
        text::set_key("[a]\nPort = 1", "a", "Port", "9"),
        "[a]\nPort = 9"
    );
}

// The readers take the LAST line of a key (llama-server the same), so the
// page shows 3 here; after the edit exactly one line stands, at the first's
// place, carrying the written value. A line of the key in another letter
// case is an occurrence and goes too.
#[test]
fn set_key_collapses_a_repeated_key_onto_the_first() {
    let text = file_of(
        &[
            "[alpha]",
            "; note above",
            "ctx-size = 1",
            "CTX-SIZE = 2",
            "ctx-size = 3 ; last",
            "other = 9",
        ],
        "\r\n",
    );
    let out = text::set_key(&text, "alpha", "ctx-size", "4096");
    assert_eq!(
        out,
        file_of(
            &["[alpha]", "; note above", "ctx-size = 4096", "other = 9"],
            "\r\n"
        ),
        "the first keeps its place, every other line of the key goes"
    );
    assert_eq!(key_line_count(&out, "ctx-size"), 1);
    assert_eq!(parse_sections(&out)[0].keys["ctx-size"], "4096");
    // The only line in another case is the one that is set, and it takes the
    // spelling the caller asked for (the readers key on the exact name).
    assert_eq!(
        text::set_key("[alpha]\r\nCTX-SIZE = 1\r\n", "alpha", "ctx-size", "9"),
        "[alpha]\r\nctx-size = 9\r\n"
    );
}

#[test]
fn set_key_appends_a_new_key_inside_the_section() {
    // The new line sits after the section's last real line, before its
    // trailing blank lines, and never past the next section's header.
    let text = file_of(&["[alpha]", "k = 1", "", "[beta]", "k = 2"], "\r\n");
    assert_eq!(
        text::set_key(&text, "alpha", "n-cpu-moe", "8"),
        file_of(
            &["[alpha]", "k = 1", "n-cpu-moe = 8", "", "[beta]", "k = 2"],
            "\r\n"
        )
    );
    // An empty body: the key lands in the section, not in the next one.
    assert_eq!(
        text::set_key("[alpha]\r\n[beta]\r\nk = 2\r\n", "alpha", "n", "9"),
        "[alpha]\r\nn = 9\r\n[beta]\r\nk = 2\r\n"
    );
}

// A section header sitting on the file's LAST line has no line ending of
// its own. An absent key appended under it must start on its own line,
// never glued to the `]` (the header's terminator is added, as
// `replace_key` does), and the reader must find the key in that section.
#[test]
fn set_key_of_an_absent_key_under_a_last_line_header() {
    // An LF file: the break in front of the key, and the one closing it,
    // are the file's LF, no CR smuggled in.
    let text = format!("{}[Sec]", file_of(&["[alpha]", "k = 1"], "\n"));
    let out = text::set_key(&text, "Sec", "n-cpu-moe", "8");
    assert_eq!(
        out,
        file_of(&["[alpha]", "k = 1", "[Sec]", "n-cpu-moe = 8"], "\n"),
        "the header line keeps its own line, the key follows on the next"
    );
    let sections = parse_sections(&out);
    assert_eq!(sections.last().expect("the Sec section").id, "Sec");
    assert_eq!(
        sections.last().expect("the Sec section").keys["n-cpu-moe"],
        "8",
        "the key is read in the section it was written to"
    );

    // The same header as the file's last line, in a CRLF file (every other
    // line CRLF): the break between the header and the appended key is the
    // file's CRLF, and the key's own line takes it as the file's ending
    // (the append path's rule, the one the LF case above pins).
    let text = format!("{}[Sec]", file_of(&["[alpha]", "k = 1"], "\r\n"));
    let out = text::set_key(&text, "Sec", "n-cpu-moe", "8");
    assert_eq!(
        out,
        file_of(&["[alpha]", "k = 1", "[Sec]", "n-cpu-moe = 8"], "\r\n"),
        "the appended key is separated from the header by the file's CRLF"
    );
    let sections = parse_sections(&out);
    assert_eq!(sections.last().expect("the Sec section").id, "Sec");
    assert_eq!(
        sections.last().expect("the Sec section").keys["n-cpu-moe"],
        "8",
        "the key is read in the section it was written to"
    );

    // A file whose only line is the header has no line break at all, so
    // nothing to detect a style from: the writer still puts a break of its
    // default style between the header and the key. Which break (CRLF, as
    // `detect_eol` answers for content with no newline, the way
    // `replace_key` does, or LF) is not pinned here, only that one is
    // there and the header survives.
    const HEADER: &str = "[Sec]";
    let out = text::set_key(HEADER, "Sec", "n-cpu-moe", "8");
    assert!(out.starts_with(HEADER), "the header line: {out:?}");
    let rest = &out[HEADER.len()..];
    assert!(
        rest.starts_with('\n') || rest.starts_with("\r\n"),
        "no line break between the header and the key: {out:?}"
    );
    assert_eq!(
        rest.trim_matches(['\r', '\n']),
        "n-cpu-moe = 8",
        "the key line, between the breaks: {out:?}"
    );
    assert_eq!(key_line_count(&out, "n-cpu-moe"), 1);
    let sections = parse_sections(&out);
    assert_eq!(sections.len(), 1, "one section: {out:?}");
    assert_eq!(sections[0].id, "Sec", "{out:?}");
    assert_eq!(sections[0].keys["n-cpu-moe"], "8", "{out:?}");
}

#[test]
fn set_key_leaves_an_absent_section_untouched() {
    // The route answers a model profile that is in neither file before it edits
    // anything, so this transform creates no section (`replace_key` does).
    let text = file_of(&["[alpha]", "k = 1"], "\r\n");
    assert_eq!(
        text::set_key(&text, "beta", "k", "2"),
        text,
        "byte for byte"
    );
    assert_eq!(text::set_key("", "beta", "k", "2"), "", "empty text");
}

#[test]
fn set_key_of_the_value_the_key_already_has_is_the_text_unchanged() {
    for eol in ["\n", "\r\n"] {
        let text = file_of(&["[alpha]", "ctx-size = 4096", ""], eol);
        assert_eq!(
            text::set_key(&text, "alpha", "ctx-size", "4096"),
            text,
            "the route writes nothing when the text comes back equal"
        );
    }
}

#[test]
fn set_key_writes_the_value_as_it_is_given() {
    // Trimming the value and refusing `;`/`#` are the route's rules; this
    // transform checks nothing, so the value it is handed is the value that
    // lands after `key =`.
    assert_eq!(
        text::set_key("[a]\r\nPort = 1\r\n", "a", "Port", " 9 "),
        "[a]\r\nPort =  9 \r\n"
    );
    assert_eq!(
        text::set_key("[a]\r\nPort = 1\r\n", "a", "Port", "C:/Models #1"),
        "[a]\r\nPort = C:/Models #1\r\n"
    );
}

// "Already has that value" is compared the way the parser READS the line:
// the value with any inline comment cut at the first `;`/`#` and trimmed.
// When the key sits exactly once in the section, spells the key in the
// exact case given, and that reading equals the value given, the text comes
// back byte for byte: the line is NOT canonicalised, its indentation,
// spacing and inline comment all stay. A line spelling the key in ANOTHER
// case is rewritten, because llama-server refuses that spelling
// (common/preset.cpp): leaving it would answer a save with 200 over a
// model profile that does not start.
#[test]
fn set_key_of_an_equal_value_returns_the_text_byte_for_byte() {
    for eol in ["\n", "\r\n"] {
        let canonical = file_of(&["[alpha]", "Port = 9090", "Host = localhost"], eol);
        assert_eq!(
            text::set_key(&canonical, "alpha", "Port", "9090"),
            canonical,
            "the canonical line is not rewritten at all"
        );

        let commented = file_of(
            &["[alpha]", "Port = 9090 ; hand note", "Host = localhost"],
            eol,
        );
        assert_eq!(
            text::set_key(&commented, "alpha", "Port", "9090"),
            commented,
            "the value read is `9090`, the inline comment stays"
        );

        let spaced = file_of(
            &[
                "[alpha]",
                "  Port\t=  9090  ; hand note",
                "Host = localhost",
            ],
            eol,
        );
        assert_eq!(
            text::set_key(&spaced, "alpha", "Port", "9090"),
            spaced,
            "indentation, tabs and spacing are kept, not canonicalised"
        );
    }
    // The file's last line has no ending of its own: the text, ending
    // included, comes back as it was.
    let last = "[alpha]\nPort = 9090";
    assert_eq!(
        text::set_key(last, "alpha", "Port", "9090"),
        last,
        "the unterminated final line stays unterminated"
    );
}

// The equal-value case of the sibling test is for a line that spells the
// key AS IT IS GIVEN: `CTX-SIZE = 4096` set to `ctx-size` `4096` is
// rewritten as `ctx-size = 4096`, at the same place, with the line's own
// ending. llama-server knows a key only in its own case and refuses a
// model profile holding another spelling, so an untouched line here is a save that
// answers 200 over a model profile that will not start.
#[test]
fn set_key_of_an_equal_value_rewrites_a_key_spelled_in_another_case() {
    for eol in ["\n", "\r\n"] {
        let upper = file_of(&["[alpha]", "CTX-SIZE = 4096", "host = localhost"], eol);
        assert_eq!(
            text::set_key(&upper, "alpha", "ctx-size", "4096"),
            file_of(&["[alpha]", "ctx-size = 4096", "host = localhost"], eol),
            "the line is rewritten in the spelling given, and the neighbour \
                 line keeps its own ({eol:?})"
        );

        // The line keeps its own ending: an unterminated last line stays
        // unterminated.
        assert_eq!(
            text::set_key("[alpha]\nCTX-SIZE = 4096", "alpha", "ctx-size", "4096"),
            "[alpha]\nctx-size = 4096",
            "the rewrite of an unterminated final line leaves it unterminated"
        );
    }
}

// When the value DOES differ, the line is rewritten whole as `key = value`
// (the decision the sibling test pins: only an equal value is untouched),
// and the rewritten line keeps its own ending.
#[test]
fn set_key_of_a_changed_value_keeps_the_last_lines_own_ending() {
    let text = file_of(
        &["[alpha]", "Port = 9090 ; hand note", "Host = localhost"],
        "\n",
    );
    assert_eq!(
        text::set_key(&text, "alpha", "Port", "8080"),
        file_of(&["[alpha]", "Port = 8080", "Host = localhost"], "\n"),
        "a changed value rewrites the line whole, comment included"
    );
    assert_eq!(
        text::set_key("[a]\nPort = 1 ; note", "a", "Port", "9"),
        "[a]\nPort = 9",
        "the unterminated final line stays unterminated"
    );
}

// A key that occurs more than once is collapsed onto the first occurrence
// even when every copy already holds the value: "nothing to write" is only
// true of a key that sits exactly once in the section.
#[test]
fn set_key_collapses_a_repeated_key_even_when_the_values_match() {
    let text = file_of(
        &[
            "[alpha]",
            "ctx-size = 4096",
            "CTX-SIZE = 4096 ; note",
            "other = 9",
        ],
        "\r\n",
    );
    let out = text::set_key(&text, "alpha", "ctx-size", "4096");
    assert_eq!(
        out,
        file_of(&["[alpha]", "ctx-size = 4096", "other = 9"], "\r\n"),
        "one line stands, at the first's place"
    );
    assert_eq!(key_line_count(&out, "ctx-size"), 1);
}

// Blank and comment lines are not lines of the key: a collapse keeps every
// one of them, wherever it sits.
#[test]
fn set_key_keeps_the_blank_and_comment_lines_between_duplicates() {
    let text = file_of(
        &[
            "[alpha]",
            "ctx-size = 1",
            "",
            "; a note between the two",
            "ctx-size = 2",
            "other = 9",
        ],
        "\r\n",
    );
    assert_eq!(
        text::set_key(&text, "alpha", "ctx-size", "4096"),
        file_of(
            &[
                "[alpha]",
                "ctx-size = 4096",
                "",
                "; a note between the two",
                "other = 9",
            ],
            "\r\n",
        )
    );
}

// A key is matched as a whole name, not as a prefix: `keyboard` is not a
// line of `key` (the same rule as `ctx-size-draft`).
#[test]
fn set_key_and_remove_key_of_key_leave_a_longer_key_alone() {
    let text = file_of(&["[alpha]", "keyboard = 1", "other = 2"], "\r\n");
    assert_eq!(
        text::set_key(&text, "alpha", "key", "9"),
        file_of(&["[alpha]", "keyboard = 1", "other = 2", "key = 9"], "\r\n"),
        "the new line lands after the section's last real line, `keyboard` untouched"
    );
    assert_eq!(
        text::remove_key(&text, "alpha", "key"),
        text,
        "nothing to drop: the section has no `key`"
    );
}

// The key and value spans are byte counts into a line: every name and
// value here is multi-byte UTF-8.
#[test]
fn set_key_and_remove_key_in_a_section_with_non_ascii_text() {
    let text = file_of(
        &[
            "[café]",
            ";注释",
            "名字 = Café",
            "ctx-size = 4096",
            "notes = Ω 日本語",
        ],
        "\r\n",
    );
    assert_eq!(
        text::remove_key(&text, "café", "名字"),
        file_of(
            &["[café]", ";注释", "ctx-size = 4096", "notes = Ω 日本語"],
            "\r\n"
        ),
        "the non-ASCII key's line goes, its neighbour stays"
    );
    assert_eq!(
        text::set_key(&text, "café", "ctx-size", "8192"),
        file_of(
            &[
                "[café]",
                ";注释",
                "名字 = Café",
                "ctx-size = 8192",
                "notes = Ω 日本語"
            ],
            "\r\n",
        ),
        "the multi-byte value three lines away is not touched"
    );
    assert_eq!(
        text::set_key(&text, "café", "名字", "日本語"),
        file_of(
            &[
                "[café]",
                ";注释",
                "名字 = 日本語",
                "ctx-size = 4096",
                "notes = Ω 日本語"
            ],
            "\r\n",
        ),
        "a non-ASCII key and value are rewritten as given"
    );
    // The equal value is byte for byte even with a multi-byte value.
    assert_eq!(
        text::set_key(&text, "café", "notes", "Ω 日本語"),
        text,
        "the section name and the value are both kept as they are"
    );
    // A key whose byte length lands INSIDE a multi-byte character of a
    // line is not that line's key: the writers count bytes into a line, so
    // this is the shape that would panic on a slice.
    let edges = file_of(&["[café]", "aΩ = 1", "ab = 2"], "\r\n");
    assert_eq!(
        text::remove_key(&edges, "café", "ab"),
        file_of(&["[café]", "aΩ = 1"], "\r\n"),
        "only the real line goes"
    );
    assert_eq!(
        text::set_key(&edges, "café", "ab", "3"),
        file_of(&["[café]", "aΩ = 1", "ab = 3"], "\r\n"),
        "the neighbour line is not read as `ab`"
    );
}

// The reader parses `[*]` as a section named `*` (llama-server's global
// section), so the writers edit it like any other.
#[test]
fn set_key_and_remove_key_in_the_star_section() {
    let text = file_of(&["[*]", "use-jinja = true", "", "[alpha]", "k = 1"], "\r\n");
    assert_eq!(
        text::set_key(&text, "*", "use-jinja", "false"),
        file_of(
            &["[*]", "use-jinja = false", "", "[alpha]", "k = 1"],
            "\r\n"
        )
    );
    assert_eq!(
        text::remove_key(&text, "*", "use-jinja"),
        file_of(&["[*]", "", "[alpha]", "k = 1"], "\r\n")
    );
    assert_eq!(
        text::set_key(&text, "alpha", "k", "1"),
        text,
        "the guard: the equal value in the real section comes back untouched"
    );
}

// The writers take the FIRST section the reader finds; two headers of one
// name are not mended here (the manager refuses the write).
#[test]
fn set_key_edits_only_the_first_of_a_repeated_section() {
    let text = file_of(&["[a]", "k = 1", "[A]", "k = 2"], "\r\n");
    assert_eq!(
        text::set_key(&text, "a", "k", "9"),
        file_of(&["[a]", "k = 9", "[A]", "k = 2"], "\r\n")
    );
}

// The body of a section with no keys: the new line lands between the two
// headers. The file's last line has no ending of its own, so an appended
// key must not be glued to that header: it starts on its own line and
// closes with the file's ending.
#[test]
fn set_key_on_a_last_line_header_with_an_empty_body() {
    assert_eq!(
        text::set_key("[a]\n[b]", "a", "k", "1"),
        "[a]\nk = 1\n[b]",
        "the key is inside `a`, the next header keeps its line"
    );
    assert_eq!(
        text::set_key("[a]\n[b]", "b", "k", "1"),
        "[a]\n[b]\nk = 1\n",
        "the key is inside `b`, not glued to its header"
    );
    assert_eq!(
        parse_sections(&text::set_key("[a]\n[b]", "b", "k", "1"))
            .last()
            .expect("b")
            .keys["k"],
        "1",
        "the reader finds the key in `b`"
    );
}
