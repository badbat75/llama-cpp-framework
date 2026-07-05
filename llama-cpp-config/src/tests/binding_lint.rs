// Lint-style guard for the stale-widget bug class (shipped in v1.1.1, v1.2.3,
// v1.2.9): an EDITABLE std widget binding one of its self-assigned properties
// one-way to `AppState` goes stale on the first user edit — the widget's own
// imperative write discards the binding. The e2e test (ui_bindings.rs) drives
// one widget PER KIND, so a brand-new widget instance with a one-way binding
// would slip past it; this scan closes that hole by reading every ui/*.slint
// TEXT and flagging the pattern itself. Plain string scanning, no Slint
// backend — it runs as its own #[test].
//
// Sanctioned escapes (why a hit is NOT flagged):
// - two-way `<=>` bindings (the convention);
// - `read-only: true` text widgets (pure displays never self-assign);
// - bindings to non-AppState expressions (component-internal wiring, e.g. the
//   AutoSlider's `value: root.shown` init that its `changed shown` hook pushes);
// - custom components (SegmentedControl, MappedComboBox, EnumComboBox,
//   AutoSlider) — the reactive binding lives INSIDE the component (against
//   `root`, not `AppState`), so an instance carries no one-way `AppState` hit
//   to flag. AutoSlider's slider push is pinned in ui_bindings.rs; the others
//   are structural (EnumComboBox drives current-index to sidestep the
//   `current-value` #11970 bug — see `no_current_value_bindings…` below).

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
    ("SpinBox", &["value:"]),
    ("Slider", &["value:"]),
    ("ComboBox", &["current-value:", "current-index:"]),
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
/// (`LineEdit { text: AppState.x; }`) or with a sibling statement is still seen
/// — a per-line `starts_with` only caught the one-statement-per-line style.
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
/// collect their violations as "file: widget — line" strings.
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
                let _ = writeln!(violations, "{file_label}: {widget} — {v}");
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
    assert!(scanned >= 7, "expected the full ui/ set, scanned {scanned}");
    assert!(
        violations.is_empty(),
        "one-way AppState bindings on self-assigning widgets (use `<=>`, or a \
         sanctioned pattern from the README conventions table):\n{violations}"
    );
}

// #11970 guard: writing `current-value` does NOT move a ComboBox's selection,
// so a two-way `current-value <=>` binding leaves the dropdown stale on a model
// change (a preset switch). `EnumComboBox` drives `current-index` instead —
// keep it that way: no `.slint` should BIND `current-value` at all. The one-way
// scan above deliberately allows `<=>`, so it can't catch this two-way form;
// a dedicated line scan can. (Reads like `self.current-value` inside an
// expression are fine — only a binding, i.e. a line STARTING with the property,
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
        "`current-value` bindings don't move the selection (Slint #11970) — use \
         EnumComboBox (drives current-index):\n{hits}"
    );
}

// The scanner must actually catch the bug class it exists for — feed it the
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
