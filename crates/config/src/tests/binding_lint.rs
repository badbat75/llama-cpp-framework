//! Lint-style guard for the stale-widget bug class (shipped in v1.1.1, v1.2.3,
//! v1.2.9): an EDITABLE std widget binding one of its self-assigned properties
//! one-way to `AppState` goes stale on the first user edit: the widget's own
//! imperative write discards the binding. The e2e test (ui_bindings.rs) drives
//! one widget PER KIND, so a brand-new widget instance with a one-way binding
//! would slip past it; this scan closes that hole by reading every ui/*.slint
//! TEXT and flagging the pattern itself. Plain string scanning, no Slint
//! backend; it runs as its own #[test].
//!
//! Sanctioned escapes (why a hit is NOT flagged):
//! - two-way `<=>` bindings (the convention);
//! - `read-only: true` text widgets (pure displays never self-assign);
//! - bindings to non-AppState expressions (component-internal wiring, e.g. the
//!   AutoSlider's `value: root.shown` init that its `changed shown` hook pushes);
//! - custom components (SegmentedControl, MappedComboBox, EnumComboBox,
//!   AutoSlider): the reactive binding lives INSIDE the component (against
//!   `root`, not `AppState`), so an instance carries no one-way `AppState` hit
//!   to flag. AutoSlider's slider push is pinned in ui_bindings.rs; the others
//!   are structural (EnumComboBox drives current-index to sidestep the
//!   `current-value` #11970 bug; see `no_current_value_bindings…` below).
//!   NOT escaped: DefaultLineEdit, whose inner std LineEdit self-assigns through
//!   `<=> root.value`, so the CALL SITE must bind `value`/`default` two-way; it is
//!   listed in SELF_ASSIGNING and scanned like a bare widget (the `Default` prefix
//!   stops it matching LineEdit, so it needs its own entry).
//!
//! A second, unrelated rule lives here because it is the same kind of scan:
//! `no_spinbox_widgets_anywhere`: the std SpinBox is BANNED from this UI (it
//! edits itself on a stray mouse-wheel; see ui/components.slint), and a text scan
//! is the only thing that can say so about a widget nobody has instantiated yet.

use std::fmt::Write as _;
use std::path::Path;

/// Std widgets that imperatively self-assign a property on user input, paired
/// with the property they overwrite. A one-way binding on that property dies
/// at the first edit.
const SELF_ASSIGNING: &[(&str, &[&str])] = &[
    ("LineEdit", &["text:"]),
    ("TextEdit", &["text:"]),
    ("CheckBox", &["checked:"]),
    ("Switch", &["checked:"]),
    // SpinBox is banned outright (`no_spinbox_widgets_anywhere`), but it stays
    // listed: if one ever comes back, it should trip BOTH rules, not neither.
    ("SpinBox", &["value:"]),
    ("Slider", &["value:"]),
    ("ComboBox", &["current-value:", "current-index:"]),
    // A composite wrapping a bare std widget bound `<=> root.value`: the call
    // site must be two-way too (see the header's "NOT escaped" note).
    ("DefaultLineEdit", &["value:", "default:"]),
];

fn strip_line_comment(line: &str) -> &str {
    line.split("//").next().unwrap_or(line)
}

/// `true` if this line opens a block of the given std widget (`Widget {`,
/// possibly behind `name :=` or `if cond :`). Substring match is enough for
/// this codebase's one-widget-per-line style; a custom component whose name
/// merely CONTAINS a std name (e.g. `MappedComboBox`) must not match, so the
/// character before the name must not be part of an identifier.
fn opens_widget(code: &str, widget: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = code[from..].find(widget) {
        let at = from + rel;
        let prev_ok = at == 0
            || !code[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-');
        let after = &code[at + widget.len()..];
        if prev_ok && after.trim_start().starts_with('{') {
            return true;
        }
        from = at + widget.len();
    }
    false
}

/// The one-way violations inside a single widget block's text, as
/// "property" strings (empty when clean). Comments are already stripped in
/// `scan`; here the block is split into statement-like fragments on `;`/`{`/`}`
/// so a property sharing a line with the widget opener
/// (`LineEdit { text: AppState.x; }`) or with a sibling statement is still seen:
/// a per-line `starts_with` only caught the one-statement-per-line style.
fn block_violations(widget: &str, props: &[&str], block: &str) -> Vec<String> {
    let fragments: Vec<&str> = block.split([';', '{', '}']).collect();
    let read_only = fragments
        .iter()
        .any(|f| f.trim_start().starts_with("read-only:") && f.contains("true"));
    if read_only && (widget == "LineEdit" || widget == "TextEdit") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for frag in &fragments {
        let t = frag.trim_start();
        for prop in props {
            if t.starts_with(prop) && frag.contains("AppState.") && !frag.contains("<=>") {
                out.push(format!("{prop} {}", t.trim().replace('\n', " ")));
            }
        }
    }
    out
}

/// Scan one .slint source: track editable-widget blocks by brace depth and
/// collect their violations as "file: widget, line" strings.
fn scan(source: &str, file_label: &str, violations: &mut String) {
    // (widget name, self-assigned props, depth at open, accumulated block text)
    let mut stack: Vec<(&str, &[&str], i32, String)> = Vec::new();
    let mut depth = 0i32;
    for line in source.lines() {
        let code = strip_line_comment(line);
        for (widget, props) in SELF_ASSIGNING {
            if opens_widget(code, widget) {
                stack.push((widget, props, depth, String::new()));
            }
        }
        for block in &mut stack {
            block.3.push_str(code);
            block.3.push('\n');
        }
        depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        while stack.last().is_some_and(|b| depth <= b.2) {
            let (widget, props, _, block) = stack.pop().unwrap();
            for v in block_violations(widget, props, &block) {
                let _ = writeln!(violations, "{file_label}: {widget}, {v}");
            }
        }
    }
}

#[test]
fn no_one_way_appstate_bindings_on_self_assigning_widgets() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut violations = String::new();
    let mut scanned = 0;
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint") {
            let source = std::fs::read_to_string(&path).expect("read .slint");
            scan(
                &source,
                &path.file_name().unwrap().to_string_lossy(),
                &mut violations,
            );
            scanned += 1;
        }
    }
    // 12: every ui/*.slint, downloads_window.slint the latest.
    assert!(
        scanned >= 12,
        "expected the full ui/ set, scanned {scanned}"
    );
    assert!(
        violations.is_empty(),
        "one-way AppState bindings on self-assigning widgets (use `<=>`, or a \
         sanctioned pattern from the README conventions table):\n{violations}"
    );
}

/// The std `SpinBox` is banned from this UI, and this is the only thing that can
/// enforce it: it is not a stale-binding bug (the scan above would catch that) but
/// a SILENT-EDIT one, and it needs no binding at all to bite.
///
/// `SpinBoxBase` (i-slint-compiler's widget library) puts a TouchArea over the
/// whole widget whose `scroll-event` increments/decrements and always returns
/// `accept`, with no `has-focus` guard, unlike its own `ComboBoxBase`, which
/// rejects a scroll it isn't focused for. So wheeling the page with the pointer
/// merely PASSING OVER a SpinBox edits the config (and eats the scroll, so the page
/// doesn't even move): a preset silently gains a different ctx-size, and nothing in
/// the UI says so. There is no property to disable it and no way to intercept the
/// event from outside (the TouchArea lives inside the widget), so the fix was to
/// stop using the widget: every numeric field is a `DefaultLineEdit` now
/// (ui/components.slint). Re-adding a SpinBox re-adds the bug; hence this scan.
#[test]
fn no_spinbox_widgets_anywhere() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut hits = String::new();
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint") {
            let source = std::fs::read_to_string(&path).expect("read .slint");
            for (n, line) in source.lines().enumerate() {
                let code = strip_line_comment(line);
                // The block opener, not a mention: `SpinBox {` (possibly behind
                // `x :=`). `opens_widget`'s identifier-boundary rule keeps
                // DefaultSpinBox-style names from matching, if one ever returns.
                if opens_widget(code, "SpinBox") {
                    let _ = writeln!(
                        hits,
                        "{}:{}: {}",
                        path.file_name().unwrap().to_string_lossy(),
                        n + 1,
                        line.trim()
                    );
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "Slint's SpinBox edits itself on a mouse-wheel over the page (no has-focus \
         guard on its scroll-event); use DefaultLineEdit with `input_type: \
         InputType.number`:\n{hits}"
    );
}

// #11970 guard: writing `current-value` does NOT move a ComboBox's selection,
// so a two-way `current-value <=>` binding leaves the dropdown stale on a model
// change (a preset switch). `EnumComboBox` drives `current-index` instead;
// keep it that way: no `.slint` should BIND `current-value` at all. The one-way
// scan above deliberately allows `<=>`, so it can't catch this two-way form;
// a dedicated line scan can. (Reads like `self.current-value` inside an
// expression are fine: only a binding, i.e. a line STARTING with the property,
// is flagged.)
#[test]
fn no_current_value_bindings_use_enum_combo_box_instead() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut hits = String::new();
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint") {
            let source = std::fs::read_to_string(&path).expect("read .slint");
            for (n, line) in source.lines().enumerate() {
                if strip_line_comment(line)
                    .trim_start()
                    .starts_with("current-value")
                {
                    let _ = writeln!(
                        hits,
                        "{}:{}: {}",
                        path.file_name().unwrap().to_string_lossy(),
                        n + 1,
                        line.trim()
                    );
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "`current-value` bindings don't move the selection (Slint #11970); use \
         EnumComboBox (drives current-index):\n{hits}"
    );
}

/// `EnumComboBox` finds `value` in `options` through an explicit ternary chain
/// (Slint has no index-of), and a value past its last branch falls back to index
/// 0: the combo SHOWS the first entry ("default") while the form still holds the
/// real value, and nothing errors. Adding `bf16`, `q5_1` and `iq4_nl` took `cache_types`
/// from 7 entries to 10 over an 8-branch chain, which is exactly that. So every
/// string-array option list in ui/ (the `Options` globals and inline `options:`
/// literals) must fit the chain.
#[test]
fn enum_combo_box_index_chain_covers_every_option_list() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let components =
        std::fs::read_to_string(ui.join("components.slint")).expect("read components.slint");
    let chain = components
        .split("property <int> value_index:")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .expect("EnumComboBox value_index declaration");
    let branches = chain.matches("root.options[").count();

    let mut longest = (0, String::new());
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if !path.extension().is_some_and(|e| e == "slint") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("read .slint");
        for (n, line) in source.lines().enumerate() {
            let code = strip_line_comment(line);
            // `<[string]>` opens the literal after it too, so counting quotes from
            // the first `[` is still counting only the list's own entries.
            let is_list = code.contains("<[string]>") || code.trim_start().starts_with("options:");
            let (Some(open), Some(close)) = (code.find('['), code.rfind(']')) else {
                continue;
            };
            if !is_list || close < open {
                continue;
            }
            let entries = code[open..close].matches('"').count() / 2;
            if entries > longest.0 {
                let file = path.file_name().unwrap().to_string_lossy();
                longest = (entries, format!("{file}:{}", n + 1));
            }
        }
    }
    assert!(
        longest.0 >= 10,
        "expected cache_types (10 entries) among the lists, longest {longest:?}"
    );
    assert!(
        branches >= longest.0,
        "EnumComboBox's value_index chain has {branches} branches but {} has {} options: \
         extend the chain in ui/components.slint",
        longest.1,
        longest.0
    );
}

// ── Glyph coverage ───────────────────────────────────────────────────────

/// Non-ASCII codepoints the UI may draw with the DEFAULT font. Every one is in
/// Segoe UI's cmap: checked, not assumed (`GlyphTypeface.CharacterToGlyphMap`
/// over `segoeui.ttf`), which is the only thing that settles coverage. Nothing
/// falls back: a codepoint Segoe UI lacks draws as an empty box, full stop.
///
/// An ICON does not belong here. It goes through the embedded icon font (the
/// `Icons` global in `components.slint`) with its family named at the call site:
/// `text: Icons.info; font-family: Icons.font;` is the idiom `LabeledField`'s
/// hint glyph already uses. The scan skips `\u{…}` escapes for exactly that
/// reason, so `Icons` can declare its codepoints.
///
/// Deliberately absent: `ⓘ` and `─`, which appear in this codebase only inside
/// COMMENTS. Segoe UI has neither, so neither is proven; listing them would
/// have pre-approved two glyphs that draw as boxes the day someone used one.
const RENDERABLE: &[char] = &['·', '×', '—', '…', '→', '↔', '≈', '≡', '≤'];

/// Every double-quoted literal in a `.slint` source, with its 1-based line.
/// A real scanner rather than a line grep because `//` inside a string (a URL)
/// must not read as a comment, and `✕` inside a comment must not read as UI text.
fn string_literals(source: &str) -> Vec<(usize, String)> {
    #[derive(PartialEq)]
    enum S {
        Code,
        Line,
        Block,
        Str,
    }
    let mut out = Vec::new();
    let (mut state, mut line, mut buf, mut at) = (S::Code, 1usize, String::new(), 1usize);
    let mut it = source.chars().peekable();
    while let Some(c) = it.next() {
        match state {
            S::Code => match c {
                '/' if it.peek() == Some(&'/') => state = S::Line,
                '/' if it.peek() == Some(&'*') => state = S::Block,
                '"' => {
                    buf.clear();
                    at = line;
                    state = S::Str;
                }
                _ => {}
            },
            S::Line => {
                if c == '\n' {
                    state = S::Code;
                }
            }
            S::Block => {
                if c == '*' && it.peek() == Some(&'/') {
                    it.next();
                    state = S::Code;
                }
            }
            S::Str => match c {
                '\\' => {
                    it.next(); // the escaped char is never a glyph we judge
                }
                '"' => {
                    out.push((at, std::mem::take(&mut buf)));
                    state = S::Code;
                }
                _ => buf.push(c),
            },
        }
        if c == '\n' {
            line += 1;
        }
    }
    out
}

// The tensor table's remove button first shipped reading `✕` (U+2715, Dingbats),
// which Segoe UI does not have, so it drew as an empty box: a button with no
// icon. Nothing failed. The glyph is valid Slint, valid UTF-8, and the layout is
// identical; only a human looking at the window could tell, which is what this
// scan replaces. It is not a hypothetical: EVERY non-ASCII character this UI
// actually draws is in Segoe UI, and the one that wasn't is the one that broke.
#[test]
fn slint_string_literals_only_use_glyphs_proven_to_render() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let (mut hits, mut scanned) = (String::new(), 0);
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint") {
            let source = std::fs::read_to_string(&path).expect("read .slint");
            for (line, text) in string_literals(&source) {
                for c in text.chars().filter(|c| !c.is_ascii()) {
                    if !RENDERABLE.contains(&c) {
                        let _ = writeln!(
                            hits,
                            "{}:{}: U+{:04X} '{c}' in {text:?}",
                            path.file_name().unwrap().to_string_lossy(),
                            line,
                            c as u32,
                        );
                    }
                }
            }
            scanned += 1;
        }
    }
    // The text a page shows through `Strings.t` is drawn just the same.
    for (key, text) in &language_file() {
        for c in text.chars().filter(|c| !c.is_ascii()) {
            if !RENDERABLE.contains(&c) {
                let _ = writeln!(hits, "en_US.toml: U+{:04X} '{c}' in {key}", c as u32);
            }
        }
    }
    // 12: every ui/*.slint, downloads_window.slint the latest.
    assert!(
        scanned >= 12,
        "expected the full ui/ set, scanned {scanned}"
    );
    assert!(
        hits.is_empty(),
        "glyph(s) not known to render in the GUI's font stack: they draw as an \
         empty box, silently. Run the GUI, look at it, and only then add the char \
         to RENDERABLE:\n{hits}"
    );
}

// ── Language file ────────────────────────────────────────────────────────

/// A file of the strings folder, which lives in the core: both binaries read it.
fn strings_file(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/strings")
        .join(name)
}

/// The `src/` of every crate whose text the files hold: this one, the core and
/// the manager.
fn src_dirs() -> [std::path::PathBuf; 3] {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    ["config", "core", "manager"].map(|c| crates.join(c).join("src"))
}

/// Every `.rs` of every crate in `src_dirs`, as `rust_sources` picks them.
fn all_rust_sources() -> Vec<std::path::PathBuf> {
    let mut sources = Vec::new();
    for dir in src_dirs() {
        rust_sources(&dir, &mut sources);
    }
    sources
}

fn language_file() -> std::collections::HashMap<String, String> {
    let file = strings_file("en_US.toml");
    crate::strings::parse(&std::fs::read_to_string(file).expect("read en_US.toml"))
}

/// What each `Strings.t(`, `t1(` or `t2(` in a `.slint` source is given, with the
/// count of values it passes: the key when it is a string literal, else the text
/// behind the parenthesis, which `is_key` refuses (a computed key could not be
/// checked against the file).
fn strings_t_keys(source: &str) -> Vec<(usize, String)> {
    const CALL: &str = "Strings.t";
    let mut keys = Vec::new();
    for line in source.lines() {
        let mut rest = strip_line_comment(line);
        while let Some(at) = rest.find(CALL) {
            rest = &rest[at + CALL.len()..];
            let (values, after) = match rest.split_once('(') {
                Some(("", after)) => (0, after),
                Some(("1", after)) => (1, after),
                Some(("2", after)) => (2, after),
                _ => continue,
            };
            let key = after
                .strip_prefix('"')
                .and_then(|s| s.split_once('"'))
                .map_or(after, |(key, _)| key);
            keys.push((values, key.to_string()));
        }
    }
    keys
}

/// What each `strings::t(`, `fill(` or `key(` in a Rust source is given: the key
/// when it is a string literal, with the count of values `fill` passes in its
/// `&[...]`, 0 for `t`, and `None` where the count is not at the call (`key` marks
/// a key shown later). A call on a non-literal is skipped: that key is marked with
/// `key` where it is written.
fn rust_string_calls(source: &str) -> Vec<(Option<usize>, String)> {
    let code: String = source
        .lines()
        .map(|l| format!("{}\n", strip_line_comment(l)))
        .collect();
    let mut calls = Vec::new();
    for (call, is_fill) in [
        ("strings::t(", false),
        ("strings::key(", false),
        ("strings::fill(", true),
    ] {
        let mut rest = code.as_str();
        while let Some(at) = rest.find(call) {
            rest = &rest[at + call.len()..];
            let Some((key, after)) = rest
                .trim_start()
                .strip_prefix('"')
                .and_then(|s| s.split_once('"'))
            else {
                continue;
            };
            let values = match (call, is_fill) {
                ("strings::t(", _) => Some(0),
                (_, true) => after
                    .trim_start()
                    .strip_prefix(',')
                    .and_then(|s| slice_len(s.trim_start())),
                _ => None,
            };
            calls.push((values, key.to_string()));
        }
    }
    calls
}

/// The element count of the `&[a, b(c, d), "e,"]` at the start of `s`, a trailing
/// comma not counted.
fn slice_len(s: &str) -> Option<usize> {
    let s = s.strip_prefix("&[")?;
    let (mut depth, mut commas, mut quoted, mut escaped, mut pending) = (0, 0, false, false, false);
    for c in s.chars() {
        if quoted {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '(' | '[' | '{' => depth += 1,
            ']' if depth == 0 => return Some(commas + usize::from(pending)),
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                commas += 1;
                pending = false;
                continue;
            }
            _ => {}
        }
        pending |= !c.is_whitespace();
    }
    None
}

/// Rust sources whose text is the GUI's unless a source the CLI reaches calls into
/// them (`reached_gui_only_calls`). Every other source outside `gui/` is shared:
/// its keys belong in `cli.toml` too.
const GUI_ONLY: [&str; 12] = [
    "ini.rs",
    "tensor_override.rs",
    "devices.rs",
    "form.rs",
    "gguf.rs",
    "gpu_split.rs",
    "kv_cache.rs",
    "model_scan.rs",
    "moe_cache.rs",
    "net_ifaces.rs",
    "server_form.rs",
    "tensor_picker.rs",
];

fn gui_only_source(path: &Path) -> bool {
    path.ancestors()
        .any(|p| p.file_name().is_some_and(|n| n == "gui"))
        || path
            .file_name()
            .is_some_and(|n| n == "gui.rs" || GUI_ONLY.iter().any(|g| n == *g))
}

/// Keys of a source a CLI command does reach, written where it does not:
/// `linked_files.rs` shows its rows and its deletion report to the GUI alone,
/// `integrations::claude_code_env_script` is the Integrations tab's snippet,
/// `bench::sweep::check_values` is the tab's A/B check, `presets::rename` and the
/// `ini::rename_section` under it are the rename dialog's, `startup::set_enabled`
/// is the Settings tab's, and `main` prints `gui-launch-error` on a launch with no
/// arguments, which reads the language file.
const GUI_ONLY_KEYS: [&str; 27] = [
    "claude-snippet-fit-note",
    "claude-snippet-launch",
    "claude-snippet-no-context",
    "claude-snippet-pick",
    "claude-snippet-pick-with-context",
    "claude-snippet-run",
    "command-too-long",
    "gpu-lock-held",
    "gpus-in-use",
    "could-not-delete",
    "delete-also-files",
    "delete-preset-only",
    "deleted-preset",
    "empty-folder-many",
    "empty-folder-one",
    "gui-launch-error",
    "in-use-kept",
    "kept-in-use",
    "key-value-refused",
    "login-start-unavailable",
    "new-preset-id-empty",
    "new-preset-id-unchanged",
    "preset-id-charset",
    "removed-file-many",
    "removed-file-one",
    "section-exists",
    "section-not-found",
];

/// Functions of the shared sources that only a CLI command runs, beside all of
/// `cli.rs`: their keys are in `cli.toml` alone. `cli_only_code_is_not_called_from_elsewhere`
/// fails when other code starts calling one, since the GUI would then miss its keys.
const CLI_ONLY_FNS: [(&str, &str); 15] = [
    ("bench/db.rs", "set_answer"),
    ("bench/gpu_lock.rs", "acquire"),
    ("bench/interrupt.rs", "install"),
    ("bench/interrupt.rs", "restore"),
    ("bench/query.rs", "answer"),
    ("bench/query.rs", "publish"),
    ("bench/query.rs", "run"),
    ("bench/query.rs", "run_detail"),
    ("bench/query.rs", "where_clause"),
    ("bench/spec.rs", "run"),
    ("bench/sweep.rs", "measure"),
    ("bench/sweep.rs", "report_leg"),
    ("bench/sweep.rs", "run"),
    ("manager.rs", "main"),
    ("manager.rs", "run"),
];

/// A source split into what only a CLI command runs and the rest. The split of a
/// source with such functions comes from `rust_fns`, so its tests are in neither.
fn split_cli_only(path: &Path, source: &str) -> (String, String) {
    if path.file_name().is_some_and(|n| n == "cli.rs") {
        return (source.to_string(), String::new());
    }
    let listed: Vec<&str> = CLI_ONLY_FNS
        .iter()
        .filter(|(file, _)| path.ends_with(file))
        .map(|(_, f)| *f)
        .collect();
    if listed.is_empty() {
        return (String::new(), source.to_string());
    }
    let (mut cli, mut rest) = (String::new(), String::new());
    for (name, indented, text) in rust_fns(source) {
        let side = if !indented && listed.contains(&name.as_str()) {
            &mut cli
        } else {
            &mut rest
        };
        side.push_str(&text);
    }
    (cli, rest)
}

/// The functions of a rustfmt-ed source, `(name, indented, text)`, its tests left
/// out: a body ends at the `}` at its `fn`'s indent. What lies outside every
/// function comes last, under an empty name.
fn rust_fns(source: &str) -> Vec<(String, bool, String)> {
    let mut fns = Vec::new();
    let mut outside = String::new();
    let mut lines = source.lines().take_while(|l| l.trim() != "mod tests {");
    while let Some(raw) = lines.next() {
        let line = strip_line_comment(raw);
        let code = line.trim_start();
        let bare = ["pub(crate) ", "pub(super) ", "pub "]
            .iter()
            .find_map(|p| code.strip_prefix(p))
            .unwrap_or(code);
        let bare = bare.strip_prefix("const ").unwrap_or(bare);
        let Some(rest) = bare.strip_prefix("fn ") else {
            outside.push_str(line);
            outside.push('\n');
            continue;
        };
        let name = rest
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .next()
            .unwrap_or_default();
        let indent = &line[..line.len() - code.len()];
        let mut body = format!("{line}\n");
        if !line.trim_end().ends_with(['}', ';']) {
            let close = format!("{indent}}}");
            for raw in lines.by_ref() {
                let line = strip_line_comment(raw);
                body.push_str(line);
                body.push('\n');
                if line.trim_end() == close {
                    break;
                }
            }
        }
        fns.push((name.to_string(), !indent.is_empty(), body));
    }
    fns.push((String::new(), false, outside));
    fns
}

/// `name` in `text` as a whole identifier path, not the tail or the head of a longer
/// one; `then` is what must follow it (`None`: anything but an identifier character).
fn names(text: &str, name: &str, then: Option<char>) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(name).any(|(at, _)| {
        let next = text[at + name.len()..].chars().next();
        !word(text[..at].chars().next_back()) && then.map_or(!word(next), |c| next == Some(c))
    })
}

/// The string calls of the `GUI_ONLY` functions (`(module, source)`) that the texts
/// in `reachers` can get to: by `module::function`, through a same-file bare call
/// once inside, or by a method carrying text, matched by name alone. `lines` is left
/// out of that match: it is `str::lines` everywhere else.
fn reached_gui_only_calls(
    reachers: &[String],
    gui_only: &[(String, String)],
) -> Vec<(Option<usize>, String)> {
    let fns: Vec<(&str, (String, bool, String))> = gui_only
        .iter()
        .flat_map(|(module, source)| {
            rust_fns(source)
                .into_iter()
                .map(move |f| (module.as_str(), f))
        })
        .filter(|(_, (name, _, _))| !name.is_empty())
        .collect();
    let mut reached = vec![false; fns.len()];
    let mut work: Vec<(&str, &str)> = reachers.iter().map(|t| ("", t.as_str())).collect();
    let mut calls = Vec::new();
    while let Some((from, text)) = work.pop() {
        for (i, (module, (name, indented, body))) in fns.iter().enumerate() {
            let method = *indented
                && name != "lines"
                && body.contains("strings::")
                && text.contains(&format!(".{name}("));
            let bare = *module == from
                && text.match_indices(&format!("{name}(")).any(|(at, _)| {
                    !text[..at].ends_with(|c: char| c.is_alphanumeric() || "_.:".contains(c))
                });
            if !reached[i] && (method || bare || names(text, &format!("{module}::{name}"), None)) {
                reached[i] = true;
                calls.extend(rust_string_calls(body));
                work.push((module, body));
            }
        }
    }
    calls
}

/// A key held in a `strings::key` mark is shown later, through a call on a variable.
/// Its text must take what that call passes: in a function with a `strings::fill` on
/// a non-literal key, that call's values; anywhere else (a table, a `strings::t` on a
/// variable) none.
fn marked_key_calls(source: &str) -> Vec<(Option<usize>, String)> {
    let mut calls = Vec::new();
    for (_, _, body) in rust_fns(source) {
        let passed: Vec<usize> = body
            .match_indices("strings::fill(")
            .filter_map(|(at, call)| {
                let args = body[at + call.len()..].trim_start();
                let (_, after) = args.split_once(',').filter(|_| !args.starts_with('"'))?;
                slice_len(after.trim_start())
            })
            .collect();
        let passed = if passed.is_empty() { vec![0] } else { passed };
        for (at, call) in body.match_indices("strings::key(") {
            let Some((key, _)) = body[at + call.len()..]
                .trim_start()
                .strip_prefix('"')
                .and_then(|s| s.split_once('"'))
            else {
                continue;
            };
            calls.extend(passed.iter().map(|n| (Some(*n), key.to_string())));
        }
    }
    calls
}

/// Every `.rs` under `dir`, the lint's own tests and the loader left out.
fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src/ dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap_or_default();
        if path.is_dir() {
            if name != "tests" {
                rust_sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") && name != "strings.rs" {
            out.push(path);
        }
    }
}

fn mistuned<'a>(
    calls: &'a std::collections::BTreeSet<(Option<usize>, String)>,
    table: &std::collections::HashMap<String, String>,
) -> Vec<&'a (Option<usize>, String)> {
    calls
        .iter()
        .filter(|(values, key)| {
            let Some(values) = values else { return false };
            table.get(key).is_some_and(|text| {
                (0..9).any(|n| text.contains(&format!("{{{n}}}")) != (n < *values))
            })
        })
        .collect()
}

/// Lowercase words joined by hyphens: no number, no quoting needed in TOML.
fn is_key(key: &str) -> bool {
    key.split('-')
        .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()))
}

// A key with no entry shows on screen as the key, and nothing else would say so:
// this scan is what stands between a typo and the window.
#[test]
fn strings_t_keys_and_the_language_file_match() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut calls = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&ui).expect("ui/ dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().is_some_and(|e| e == "slint") {
            let source = std::fs::read_to_string(&path).expect("read .slint");
            calls.extend(
                strings_t_keys(&source)
                    .into_iter()
                    .map(|(n, key)| (Some(n), key)),
            );
        }
    }
    let sources = all_rust_sources();
    let (mut shared, mut reachers, mut gui_only) =
        (std::collections::BTreeSet::new(), Vec::new(), Vec::new());
    let mut cli_only = std::collections::BTreeSet::new();
    for path in &sources {
        let source = std::fs::read_to_string(path).expect("read .rs");
        let found = rust_string_calls(&source);
        let (cli_text, rest) = split_cli_only(path, &source);
        cli_only.extend(rust_string_calls(&cli_text).into_iter().map(|(_, key)| key));
        calls.extend(marked_key_calls(&rest));
        calls.extend(rust_string_calls(&rest));
        if !gui_only_source(path) {
            shared.extend(found.iter().cloned());
            reachers.push(
                rust_fns(&source)
                    .into_iter()
                    .map(|(_, _, text)| text)
                    .collect::<String>(),
            );
        } else if path
            .file_name()
            .is_some_and(|n| GUI_ONLY.iter().any(|g| n == *g))
        {
            let module = path
                .file_stem()
                .expect("file stem")
                .to_string_lossy()
                .into_owned();
            gui_only.push((module, source));
        }
    }
    shared.extend(reached_gui_only_calls(&reachers, &gui_only));
    shared.retain(|(_, key)| !GUI_ONLY_KEYS.contains(&key.as_str()));
    let used: std::collections::BTreeSet<&String> = calls.iter().map(|(_, key)| key).collect();
    let table = language_file();
    let bad_values = mistuned(&calls, &table);
    assert!(
        bad_values.is_empty(),
        "values passed do not match the text's {{N}}: {bad_values:?}"
    );
    let invalid: Vec<_> = used
        .iter()
        .copied()
        .chain(table.keys())
        .filter(|k| !is_key(k))
        .collect();
    assert!(
        invalid.is_empty(),
        "not a literal lowercase-hyphen key: {invalid:?}"
    );
    let missing: Vec<_> = used.iter().filter(|k| !table.contains_key(**k)).collect();
    assert!(
        missing.is_empty(),
        "used in ui/ or src/ but not in en_US.toml: {missing:?}"
    );
    let unused: Vec<_> = table.keys().filter(|k| !used.contains(k)).collect();
    assert!(
        unused.is_empty(),
        "in en_US.toml but used nowhere in ui/ or in src/ outside the CLI's own code: {unused:?}"
    );

    // cli.toml: exactly the keys a CLI command can reach, each also in the language
    // file unless only the CLI's own code uses it, its {N} matching the calls.
    let file = strings_file("cli.toml");
    let cli = crate::strings::parse(&std::fs::read_to_string(file).expect("read cli.toml"));
    let bad_values = mistuned(&shared, &cli);
    assert!(
        bad_values.is_empty(),
        "values passed do not match cli.toml's {{N}}: {bad_values:?}"
    );
    let shared: std::collections::BTreeSet<&String> = shared.iter().map(|(_, key)| key).collect();
    let missing: Vec<_> = shared.iter().filter(|k| !cli.contains_key(**k)).collect();
    assert!(
        missing.is_empty(),
        "used by a source the CLI reaches but not in cli.toml: {missing:?}"
    );
    let unused: Vec<_> = cli.keys().filter(|k| !shared.contains(k)).collect();
    assert!(
        unused.is_empty(),
        "in cli.toml but used by no source the CLI reaches: {unused:?}"
    );
    let invalid: Vec<_> = cli.keys().filter(|k| !is_key(k)).collect();
    assert!(
        invalid.is_empty(),
        "not a lowercase-hyphen key in cli.toml: {invalid:?}"
    );
    let alone: Vec<_> = cli
        .keys()
        .filter(|k| !table.contains_key(*k) && !cli_only.contains(*k))
        .collect();
    assert!(
        alone.is_empty(),
        "in cli.toml but not in en_US.toml, and not only the CLI's: {alone:?}"
    );
}

/// A key in `cli.toml` alone is a miss in the window, so what holds one must stay
/// out of the GUI's reach: nothing but the CLI's own code (and the binary roots,
/// `main.rs` and `bin/pai-manager.rs`, which dispatch and which no module
/// can call) may call into `cli.rs` or a `CLI_ONLY_FNS` function. The library root
/// `lib.rs` is NOT exempt: every module, the GUI's included, can call into it.
#[test]
fn cli_only_code_is_not_called_from_elsewhere() {
    let sources = all_rust_sources();
    let texts: Vec<_> = sources
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path).expect("read .rs");
            let code: String = rust_fns(&source)
                .into_iter()
                .map(|(_, _, text)| text)
                .collect();
            let rest = split_cli_only(path, &code).1;
            (path.clone(), rest)
        })
        .collect();
    let mut hits = Vec::new();
    for (path, rest) in &texts {
        if path.ends_with("main.rs") || path.ends_with("bin/pai-manager.rs") {
            continue;
        }
        if names(rest, "cli", Some(':')) {
            hits.push(format!("{}: cli::", path.display()));
        }
        for (file, function) in CLI_ONLY_FNS {
            let module = Path::new(file)
                .file_stem()
                .expect("file stem")
                .to_string_lossy();
            let bare = path.ends_with(file)
                && rest.match_indices(&format!("{function}(")).any(|(at, _)| {
                    !rest[..at].ends_with(|c: char| c.is_alphanumeric() || "_.:".contains(c))
                });
            if bare || names(rest, &format!("{module}::{function}"), None) {
                hits.push(format!("{}: {module}::{function}", path.display()));
            }
        }
    }
    assert!(hits.is_empty(), "the GUI could reach text of cli.toml alone: put its keys in en_US.toml too and take the function off CLI_ONLY_FNS: {hits:?}");
}

/// The macros of the CLI's print paths whose text is shown to the user.
const SHOWN_MACROS: [&str; 7] = [
    "println", "eprintln", "print", "eprint", "bail", "anyhow", "ensure",
];

/// Sources whose print and error macros are scanned, and, for those marked, every
/// `format!` and `.context` too: there a format string is shown, recorded
/// (`SHOWN_LITERALS`) or layout, and nothing else.
const CLI_PRINT_PATHS: [(&str, bool); 7] = [
    ("cli.rs", true),
    ("main.rs", true),
    ("manager.rs", true),
    ("bench/interrupt.rs", true),
    ("integrations.rs", false),
    ("bench/spec.rs", false),
    ("bench/sweep.rs", false),
];

/// The format strings with a word in them that stay literal, each with why.
const SHOWN_LITERALS: [(&str, &str); 5] = [
    ("sweep-{}", "a campaign name: it goes into bench.db"),
    ("{} (default)", "a value of `server show`, read by scripts"),
    ("{v} MiB", "a value of `server show`; a unit"),
    (
        "  [{}]{off}  model={}",
        "`preset list` row: `model` is the INI key",
    ),
    (
        "mmproj: {}",
        "`preset download` row: `mmproj` is the INI key",
    ),
];

/// Every string literal handed to one of `macros` (`format` and `.context` with
/// `all`) in a Rust source, its test module left out, however many lines the call
/// takes; a literal that is a key of a `strings::` call is not one.
fn shown_literals(source: &str, all: bool) -> Vec<String> {
    let code: String = source
        .lines()
        .take_while(|l| !l.trim_end().ends_with("mod tests {"))
        .map(|l| format!("{l}\n"))
        .collect();
    let starts_call = |before: &str| {
        if all && before.ends_with(".context(") {
            return true;
        }
        let Some(head) = before.strip_suffix("!(") else {
            return false;
        };
        let word = &head[head
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map_or(0, |i| i + 1)..];
        SHOWN_MACROS.contains(&word) || (all && word == "format")
    };
    let (mut out, mut before, mut depth) = (Vec::new(), String::new(), 0usize);
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                chars.by_ref().find(|c| *c == '\n');
                continue;
            }
            // A char literal: `'"'`, `'\\''`, never a lifetime.
            '\'' => {
                let mut ahead = chars.clone();
                let first = ahead.next();
                if first == Some('\\') {
                    ahead.next();
                }
                if ahead.next() == Some('\'') {
                    chars = ahead;
                    continue;
                }
            }
            '"' => {
                let mut text = String::new();
                while let Some(s) = chars.next() {
                    match s {
                        '\\' => {
                            text.push(' ');
                            chars.next();
                        }
                        '"' => break,
                        _ => text.push(s),
                    }
                }
                let is_key = ["strings::t(", "strings::fill(", "strings::key("]
                    .iter()
                    .any(|k| before.trim_end().ends_with(k));
                if depth > 0 && !is_key {
                    out.push(text);
                }
                before.push('"');
                continue;
            }
            '(' => {
                before.push(c);
                if depth > 0 {
                    depth += 1;
                } else if starts_call(&before) {
                    depth = 1;
                }
                continue;
            }
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        before.push(c);
    }
    out
}

/// The words of a format string left to translate: ASCII letters, three or more,
/// once `{...}` placeholders, flags (`--x-y`) and file names (`server.ini`) are out.
fn language_words(literal: &str) -> Vec<String> {
    let mut rest = String::new();
    let mut skip = false;
    for c in literal.chars() {
        match c {
            '{' => skip = true,
            '}' if skip => {
                skip = false;
                rest.push(' ');
            }
            _ if !skip => rest.push(c),
            _ => {}
        }
    }
    let mut words = Vec::new();
    for token in rest.split_whitespace() {
        let bare = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
        let file_name = bare
            .split_once('.')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty());
        if bare.starts_with("--") || file_name {
            continue;
        }
        words.extend(
            bare.split(|c: char| !c.is_ascii_alphabetic())
                .filter(|w| w.len() >= 3)
                .map(str::to_string),
        );
    }
    words
}

// The owner's rule: no user-visible text literal in the code. Text the CLI prints
// goes through `strings::`; this stops a new literal in the paths already moved.
#[test]
fn cli_print_paths_hold_no_language() {
    let mut hits = Vec::new();
    let mut allowed = std::collections::BTreeSet::new();
    for (file, all) in CLI_PRINT_PATHS {
        let path = src_dirs()
            .iter()
            .map(|src| src.join(file))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("{file}: in no crate's src/"));
        let source = std::fs::read_to_string(path).expect("read .rs");
        for literal in shown_literals(&source, all) {
            if language_words(&literal).is_empty() {
                continue;
            }
            match SHOWN_LITERALS.iter().find(|(text, _)| *text == literal) {
                Some((text, _)) => {
                    allowed.insert(*text);
                }
                None => hits.push(format!("{file}: {literal:?}")),
            }
        }
    }
    assert!(hits.is_empty(), "text in a print or error macro: give it a key in strings/cli.toml (AGENTS.md, The CLI): {hits:#?}");
    let stale: Vec<_> = SHOWN_LITERALS
        .iter()
        .filter(|(text, _)| !allowed.contains(text))
        .collect();
    assert!(
        stale.is_empty(),
        "SHOWN_LITERALS entries nothing uses any more: {stale:?}"
    );
}

#[test]
fn shown_literal_scan_reads_macros_and_leaves_the_rest() {
    let source = "// println!(\"comment\");\nprintln!(\"one {x}\");\neprintln!(\n    \"two \\\n     lines\",\n    a(b),\n);\nanyhow::bail!(\"three\");\nlet c = '\"';\nprintln!(\"{}\", strings::fill(\"a-key\", &[&\"four\"]));\nlet s = \"five\";\nlet f = format!(\"six\");\nx.context(\"seven\");\n#[cfg(test)]\nmod tests {\n    println!(\"eight\");\n}\n";
    assert_eq!(
        shown_literals(source, false),
        ["one {x}", "two       lines", "three", "{}", "four"]
    );
    assert_eq!(
        shown_literals(source, true),
        [
            "one {x}",
            "two       lines",
            "three",
            "{}",
            "four",
            "six",
            "seven"
        ]
    );
    for none in [
        "{line}",
        "{}",
        "  {:<60}  {x:.1}",
        "--image-size: {e}",
        "--image {}: {e}",
        " to ",
        "{key} = {applied} (presets.ini).",
        "server.ini: {}",
    ] {
        assert!(
            language_words(none).iter().all(|w| w == "to"),
            "{none:?}: {:?}",
            language_words(none)
        );
    }
    assert_eq!(language_words("Wrote {path}"), ["Wrote"]);
    assert_eq!(
        language_words("--draft takes mtp, not {d}"),
        ["takes", "mtp", "not"]
    );
    assert_eq!(language_words("Done."), ["Done"]);
}

#[test]
fn reach_follows_calls_out_of_a_shared_source() {
    let gui_only = [
        ("alpha".to_string(), "pub fn used() {\n    helper();\n}\nfn helper() {\n    strings::t(\"reached\");\n    beta::far();\n}\nfn idle() {\n    strings::t(\"idle\");\n}\nstruct S;\nimpl S {\n    fn called(&self) {\n        strings::t(\"method\");\n    }\n    fn lines(&self) {\n        strings::t(\"lines\");\n    }\n}\n".to_string()),
        ("beta".to_string(), "pub fn far() {\n    strings::t(\"far\");\n}\npub fn far_not() {\n    strings::t(\"far-not\");\n}\n".to_string()),
    ];
    let reached = reached_gui_only_calls(
        &["alpha::used(); x.called(); y.lines();".to_string()],
        &gui_only,
    );
    let mut keys: Vec<_> = reached.into_iter().map(|(_, key)| key).collect();
    keys.sort();
    assert_eq!(keys, ["far", "method", "reached"]);
}

#[test]
fn marked_keys_take_what_the_later_call_passes() {
    let source = "fn a() {\n    let key = strings::key(\"one\");\n    strings::fill(key, &[&x, &y]);\n}\nfn b() {\n    strings::t(strings::key(\"two\"));\n}\nconst T: &str = strings::key(\"three\");\n";
    let expect =
        [(Some(2), "one"), (Some(0), "two"), (Some(0), "three")].map(|(n, k)| (n, k.to_string()));
    assert_eq!(marked_key_calls(source), expect);
}

#[test]
fn rust_scan_reads_keys_and_counts_values() {
    let calls = rust_string_calls(
        "// strings::t(\"comment\")\nstrings::t(\"one\"); strings::t(key);\nstrings::key(\"two\");\nstrings::fill(\n    \"three\",\n    &[&a(b, c), &\"d,\", &[1, 2],\n],\n);\nstrings::fill(\"four\", args);\n",
    );
    let expect = [
        (Some(0), "one"),
        (None, "two"),
        (Some(3), "three"),
        (None, "four"),
    ]
    .map(|(n, k)| (n, k.to_string()));
    assert_eq!(calls, expect);
}

#[test]
fn strings_t_scan_reads_keys_and_ignores_comments() {
    let keys = strings_t_keys(
        "// Strings.t(\"comment\")\ntext: a ? Strings.t(\"one\") : Strings.t2(\"two-three\", a, b);\ntext: Strings.t1(x, y);\n",
    );
    let expect = [(0, "one"), (2, "two-three"), (1, "x, y);")].map(|(n, k)| (n, k.to_string()));
    assert_eq!(keys, expect);
    assert!(is_key("two-three"));
    for bad in ["x, y);", "Two", "two words", "two-", "size-32", ""] {
        assert!(!is_key(bad), "{bad:?} passed as a key");
    }
}

#[test]
fn glyph_scan_reads_ui_text_and_ignores_comments_and_urls() {
    // The bug, as shipped: found AND rejected. Without the second assert this
    // test would still pass with '✕' sitting in RENDERABLE, i.e. with the lint
    // that exists for it disarmed.
    let lits = string_literals("Button { text: \"✕\"; }\n");
    assert_eq!(lits, vec![(1, "✕".to_string())]);
    assert!(
        !RENDERABLE.contains(&'✕'),
        "the tofu glyph must stay rejected"
    );

    // A comment ABOUT the bad glyph is not UI text (this very file's fix has
    // one), and a `//` inside a string is a URL, not a comment.
    let src = "// don't use ✕ here\nText { text: \"see http://x/y — ok\"; }\n";
    let lits = string_literals(src);
    assert_eq!(lits, vec![(2, "see http://x/y — ok".to_string())]);
    assert!(lits[0]
        .1
        .chars()
        .filter(|c| !c.is_ascii())
        .all(|c| RENDERABLE.contains(&c)));

    // Escaped quotes must not end the literal early: one literal, not three;
    // otherwise the tail of a hint string would go unscanned.
    let lits = string_literals(r#"Text { text: "a \"q\" b ≡"; }"#);
    assert_eq!(lits.len(), 1, "escaped quote split the literal: {lits:?}");
    assert!(lits[0].1.ends_with('≡'), "tail lost: {:?}", lits[0].1);

    // The sanctioned way to declare an icon (a `\u{…}` escape, drawn under the
    // embedded icon font) must stay open: the escape is skipped, so no bare glyph
    // reaches the allowlist check and the lint has nothing to say about it.
    let lits = string_literals(r#"out property <string> info: "\u{F4A2}";"#);
    assert!(
        lits.iter()
            .flat_map(|(_, t)| t.chars())
            .all(|c| c.is_ascii()),
        "a \\u{{…}} icon escape must not surface a non-ASCII char to judge: {lits:?}"
    );
}

// The scanner must actually catch the bug class it exists for: feed it the
// exact v1.1.1-shaped regression and a few sanctioned shapes.
#[test]
fn scanner_flags_the_known_bad_shapes_and_passes_the_sanctioned_ones() {
    let mut v = String::new();
    scan("LineEdit {\n    text: AppState.form.id;\n}\n", "t", &mut v);
    assert!(v.contains("LineEdit"), "one-way LineEdit must be flagged");

    // Same bug written as a one-liner: the property shares the opener's line,
    // so the old per-line `starts_with` missed it entirely.
    let mut v = String::new();
    scan("LineEdit { text: AppState.form.id; }\n", "t", &mut v);
    assert!(
        v.contains("LineEdit"),
        "one-LINER one-way LineEdit must be flagged"
    );

    for good in [
        "LineEdit {\n    text <=> AppState.form.id;\n}\n",
        "LineEdit {\n    read-only: true;\n    text: AppState.chat_url;\n}\n",
        // Sanctioned read-only, but as a one-liner.
        "LineEdit { read-only: true; text: AppState.chat_url; }\n",
        "Text {\n    text: AppState.status_text;\n}\n",
        "Slider {\n    value: root.shown;\n}\n",
        "MappedComboBox {\n    current-value: AppState.x;\n}\n",
        "CheckBox {\n    checked: item.enabled;\n}\n",
    ] {
        let mut v = String::new();
        scan(good, "t", &mut v);
        assert!(v.is_empty(), "false positive on:\n{good}\n→ {v}");
    }
}
