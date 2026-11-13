//! The GUI's text: one TOML file per language, `<language>.toml` under
//! `paths::strings_dir()`, read once, at the first lookup, so changing a text
//! needs no rebuild. A flat table of `key = "text"`; `ui/strings.slint` calls
//! these through the `Strings` global, Rust calls `t` and `fill` directly.
//!
//! A process reads one file, chosen once at the entry point (`main`, `set_reader`).
//! The GUI reads its language. A CLI command reads `cli.toml` and only that, which
//! holds everything the CLI prints: not a language and never translated, so no
//! language file can change CLI output. Only `--help` is literal, in `cli.rs`. A
//! test has no entry point: it reads the GUI's file over `cli.toml`, so the keys
//! only the CLI has are found too.
//!
//! A key the table does not hold is returned as it is: a miss shows on screen as
//! the key. An unreadable or malformed file is an empty table, every text then
//! being such a miss. `tests/binding_lint.rs` checks the keys of both files
//! against `ui/*.slint` and `src/`.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

/// The language the GUI shows. A second one is a new file plus a change here.
pub const GUI_LANGUAGE: &str = "en_US";
/// What a CLI command reads, whatever the GUI shows.
const CLI_FILE: &str = "cli";

/// Which of the files this process reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reader {
    Gui,
    Cli,
}

static READER: OnceLock<Reader> = OnceLock::new();

/// Called once by `main`, before any text is looked up. Only tests run without
/// it, and they read the GUI's file.
pub fn set_reader(reader: Reader) {
    let _ = READER.set(reader);
}

pub fn file_stem(reader: Reader) -> &'static str {
    match reader {
        Reader::Gui => GUI_LANGUAGE,
        Reader::Cli => CLI_FILE,
    }
}

/// Marks a `&'static str` in a const table as a key of the file, for the lint;
/// the place that shows it passes it to `t`.
pub const fn key(key: &'static str) -> &'static str {
    key
}

pub fn parse(text: &str) -> HashMap<String, String> {
    toml::from_str(text).unwrap_or_default()
}

fn lookup<'a>(table: &'a HashMap<String, String>, key: &'a str) -> &'a str {
    table.get(key).map_or(key, String::as_str)
}

fn table() -> &'static HashMap<String, String> {
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let read = |stem: &str| {
            let file = crate::paths::strings_dir().join(format!("{stem}.toml"));
            parse(&std::fs::read_to_string(file).unwrap_or_default())
        };
        let table = read(file_stem(READER.get().copied().unwrap_or(Reader::Gui)));
        #[cfg(any(test, feature = "test-support"))]
        let table = {
            let mut merged = read(CLI_FILE);
            merged.extend(table);
            merged
        };
        table
    })
}

/// Put each `{N}` of `text` to `args[N]`, in one pass: a value holding `{1}`
/// stays as it is. Any other brace is the text's own (a JSON example, say).
fn substitute(text: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((head, tail)) = rest.split_once('{') {
        out.push_str(head);
        let slot = tail
            .split_once('}')
            .and_then(|(n, after)| Some((args.get(n.parse::<usize>().ok()?)?, after)));
        match slot {
            Some((value, after)) => {
                out.push_str(&value.to_string());
                rest = after;
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The text of `key` in the language shown.
pub fn t(key: &str) -> String {
    lookup(table(), key).to_string()
}

/// The text of `key` with `args` in its `{0}`, `{1}`.
pub fn fill(key: &str, args: &[&dyn Display]) -> String {
    substitute(lookup(table(), key), args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_round_trips_its_escapes() {
        let table = parse(r#"some-hint = "one\ntwo \\ \"three\" \u2026""#);
        assert_eq!(
            lookup(&table, "some-hint"),
            "one\ntwo \\ \"three\" \u{2026}"
        );
    }

    #[test]
    fn placeholders_fill_in_any_order_and_other_braces_stay() {
        assert_eq!(substitute("{1} of {0}, {0}", &[&"a", &"b"]), "b of a, a");
        assert_eq!(substitute("{0}", &[&"{1}", &"b"]), "{1}");
        assert_eq!(
            substitute(r#"e.g. {"x":true} {} {5} {"#, &[&"a"]),
            r#"e.g. {"x":true} {} {5} {"#
        );
        assert_eq!(substitute("{0} of {1}", &[&3, &1.5]), "3 of 1.5");
    }

    /// The product is "Private AI Framework" and its executables `pai-config`
    /// and `pai-manager`: no text either binary shows or prints names the old
    /// project, the old configurator or the old manager executable. The
    /// systemd unit `private-ai-manager.service` keeps its name.
    #[test]
    fn the_strings_name_the_product_and_the_executables_by_their_new_names() {
        for file in ["en_US.toml", "cli.toml"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("strings")
                .join(file);
            let text = std::fs::read_to_string(&path).expect("read strings file");
            for (key, value) in parse(&text) {
                // Spelled in halves so the names lint (pai-config) finds no old name here.
                for old in [
                    concat!("llama-cpp", "-config"),
                    concat!("llama.cpp", "-framework"),
                    concat!("llama.cpp", " Config"),
                ] {
                    assert!(!value.contains(old), "{file} {key}: {value:?} names {old}");
                }
                let manager = value.replace("private-ai-manager.service", "");
                assert!(
                    !manager.contains(concat!("private-ai", "-manager")),
                    "{file} {key}: {value:?} names the old manager executable"
                );
            }
        }
    }

    #[test]
    fn the_window_title_is_the_display_name() {
        let gui = parse(include_str!("../strings/en_US.toml"));
        assert_eq!(gui["window-title"], "Private AI Framework: Config");
        assert_eq!(
            gui["window-title-versioned"],
            "Private AI Framework: Config v{0}"
        );
    }

    #[test]
    fn the_manager_usage_names_pai_manager() {
        let cli = parse(include_str!("../strings/cli.toml"));
        assert_eq!(
            cli["manager-usage"],
            "usage: pai-manager run-instance <id> | pai-manager serve"
        );
        assert_eq!(
            cli["manager-missing-id"],
            "run-instance needs an instance id: pai-manager run-instance <id>"
        );
    }

    #[test]
    fn an_unknown_key_reads_as_itself() {
        let table = parse(r#"known = "text""#);
        assert_eq!(lookup(&table, "known"), "text");
        assert_eq!(lookup(&table, "no-such-key"), "no-such-key");
        assert!(parse("not toml at all").is_empty());
    }
}
