//! Pins on `build.rs`'s text, which no test can run: what it watches when it
//! embeds the fixture, and that `PRIVATE_AI_FE_DIST` must be absolute (the
//! value `tests/fe.rs` reads with `option_env!`, unresolved).

fn code() -> String {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/build.rs"))
        .expect("build.rs");
    // Comments explain the choice and may name what the code must not do.
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The argument of every `watch(...)` call, and of every literal
/// `rerun-if-changed` line, as written.
fn watched(code: &str) -> Vec<String> {
    let mut args = Vec::new();
    for (at, _) in code.match_indices("watch(") {
        let rest = &code[at + "watch(".len()..];
        let arg = &rest[..rest.find(')').expect("closing paren")];
        if !arg.starts_with("dir:") {
            args.push(arg.trim().to_string());
        }
    }
    for (at, _) in code.match_indices("rerun-if-changed=") {
        let rest = &code[at..];
        args.push(rest[..rest.find('"').unwrap_or(rest.len())].to_string());
    }
    args
}

// ── Failure paths first ─────────────────────────────────────────────────

/// A relative `PRIVATE_AI_FE_DIST` is a build error naming it, not a path
/// resolved against `crates/bff`.
#[test]
fn a_relative_fe_dist_is_refused() {
    let code = code();
    assert!(
        code.contains("is_absolute()") || code.contains("is_relative()"),
        "build.rs does not check that {} is absolute",
        "PRIVATE_AI_FE_DIST"
    );
    assert!(
        !code.contains("manifest.join(d)"),
        "build.rs still resolves the value against the crate"
    );
}

/// On the fixture the script watches what a later `npm run build` reads
/// (`fe/src`, `fe/index.html`), never the whole `fe/`, whose `node_modules`
/// npm rewrites and cargo would scan file by file.
#[test]
fn the_fixture_branch_does_not_watch_fe_whole() {
    let code = code();
    let args = watched(&code);
    for arg in &args {
        assert!(
            arg != "&fe" && arg != "fe" && !arg.contains("node_modules"),
            "build.rs watches {arg:?}: {args:?}"
        );
    }
    for wanted in ["\"src\"", "\"index.html\""] {
        assert!(
            args.iter().any(|a| a.contains(wanted)),
            "build.rs does not watch fe/{}: {args:?}",
            wanted.trim_matches('"')
        );
    }
}

/// The script tells the crate which directory it embedded, so the tests
/// read that one instead of choosing again (`tests/fe.rs` `embedded()`).
#[test]
fn the_embedded_directory_is_exported_to_the_crate() {
    let code = code();
    assert!(
        code.contains("cargo:rustc-env=PAI_BFF_FE_DIR=")
            || code.contains("cargo::rustc-env=PAI_BFF_FE_DIR="),
        "build.rs does not emit rustc-env PAI_BFF_FE_DIR"
    );
}
