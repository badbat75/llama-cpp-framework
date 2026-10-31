//! docs-site: the HTML site of docs\ and benchmarks\bench.db.
//!
//! `cargo run -p docs-site -- [--check] [--out DIR]`. Reads every page of
//! docs\ (README.md is the index; AGENTS.md and templates\ are not pages) and
//! bench.db read only, checks the rules of docs\AGENTS.md that a program can
//! (front matter, required sections, every cited campaign present, every
//! bench-chart block drawable) and fails on the first build with a problem.
//! Without `--check` it then writes the site: one page per guide, a page per
//! campaign (question, answer, runs, the guides citing it), docs\assets\ and
//! the stylesheet, into DIR or `build/<os>/site/`. Static HTML, no script:
//! GitHub Pages serves it (.github\workflows\docs-site.yml).

mod chart;
mod data;
mod page;

use data::{stat, Db};
use page::Page;
use std::path::{Path, PathBuf};

pub const REPO: &str = "https://github.com/badbat75/llama-cpp-framework";

const CSS: &str = include_str!("style.css");

fn main() {
    if let Err(e) = run() {
        eprintln!("docs-site: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut out: Option<PathBuf> = None;
    let mut check = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--check" => check = true,
            "--out" => out = Some(args.next().ok_or("--out needs a directory")?.into()),
            "-h" | "--help" => {
                println!("usage: docs-site [--check] [--out DIR]");
                return Ok(());
            }
            _ => return Err(format!("unexpected argument `{a}`")),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap_or(root);
    let db = Db::open(&root.join("benchmarks/bench.db"))?;
    let pages = page::load_all(&root.join("docs"))?;
    let errors: Vec<String> = pages.iter().flat_map(|p| page::check(p, &db)).collect();
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("{e}");
        }
        return Err(format!("{} problem(s) in docs/", errors.len()));
    }
    if check {
        println!("docs-site: {} pages, {} campaigns: ok", pages.len(), db.campaigns.len());
        return Ok(());
    }
    let os = if cfg!(windows) { "windows" } else if cfg!(target_os = "macos") { "macos" } else { "linux" };
    let out = out.unwrap_or_else(|| root.join("build").join(os).join("site"));
    let _ = std::fs::remove_dir_all(&out);
    let w = |rel: &str, text: &str| -> Result<(), String> {
        let p = out.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| format!("{}: {e}", p.display()))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    w("style.css", CSS)?;
    copy_dir(&root.join("docs/assets"), &out.join("assets"))?;
    for p in &pages {
        let mut body = meta_line(p);
        body += &page::render(p, &pages, &db, "");
        let cited = p.citations();
        if !cited.is_empty() {
            body += "<section class=\"cited\"><h2>Campaigns cited</h2><ul>";
            for c in &cited {
                let q = db.campaign(c).map(|c| c.started.as_str()).unwrap_or("");
                body += &format!("<li><a href=\"campaigns/{0}.html\"><code>{0}</code></a> <span class=\"muted\">{1}</span></li>", esc(c), esc(q));
            }
            body += "</ul></section>";
        }
        w(&format!("{}.html", p.stem), &shell(p.title(), "", &body))?;
    }
    w("campaigns/index.html", &shell("Campaigns", "../", &campaign_index(&db)))?;
    for c in &db.campaigns {
        w(&format!("campaigns/{}.html", c.name), &shell(&c.name, "../", &campaign_page(c, &db, &pages)))?;
    }
    println!("docs-site: {} pages, {} campaigns -> {}", pages.len(), db.campaigns.len(), out.display());
    Ok(())
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(from) else { return Ok(()) };
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in rd.flatten() {
        let (s, d) = (e.path(), to.join(e.file_name()));
        if s.is_dir() {
            copy_dir(&s, &d)?;
        } else {
            std::fs::copy(&s, &d).map_err(|e| format!("{}: {e}", s.display()))?;
        }
    }
    Ok(())
}

fn shell(title: &str, base: &str, body: &str) -> String {
    let commit = std::env::var("GITHUB_SHA").ok().map(|s| {
        let short = &s[..s.len().min(7)];
        format!(" at commit <a href=\"{REPO}/commit/{s}\"><code>{short}</code></a>")
    });
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{t} | llama.cpp-framework</title><link rel=\"stylesheet\" href=\"{base}style.css\"></head><body>\
<header><a class=\"home\" href=\"{base}index.html\">llama.cpp-framework</a><nav><a href=\"{base}index.html\">Guides</a><a href=\"{base}campaigns/index.html\">Campaigns</a><a href=\"{REPO}\">GitHub</a></nav></header>\
<main>{body}</main><footer>Generated from <code>docs/</code> and <code>benchmarks/bench.db</code>{c}. Every number traces to a campaign; the markdown on GitHub is the source.</footer></body></html>\n",
        t = esc(title),
        c = commit.unwrap_or_default()
    )
}

fn meta_line(p: &Page) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (k, label) in [("type", ""), ("status", ""), ("platforms", ""), ("llama-cpp", "llama.cpp "), ("updated", "updated ")] {
        if let Some(v) = p.get(k).filter(|v| !v.is_empty()) {
            parts.push(format!("{label}{}", esc(v.trim_start_matches('[').trim_end_matches(']'))));
        }
    }
    if parts.is_empty() || p.stem == "index" {
        return String::new();
    }
    format!("<p class=\"meta\">{}</p>", parts.join(" · "))
}

fn campaign_index(db: &Db) -> String {
    let mut s = String::from(
        "<h1>Campaigns</h1><p>Every set of runs in <code>benchmarks/bench.db</code>, newest first: the question it was run to answer and, once drawn, the answer. A run recorded without a spec reads as <code>custom</code>.</p>\
<table><thead><tr><th>campaign</th><th>started</th><th>model</th><th>runs</th><th>spec</th><th>answer</th></tr></thead><tbody>",
    );
    for c in &db.campaigns {
        let runs: Vec<_> = db.runs_of(c).collect();
        let mut models: Vec<String> = runs.iter().map(|r| r.model.clone()).collect();
        models.dedup();
        let mut specs: Vec<String> = runs.iter().map(|r| r.spec()).collect();
        specs.sort();
        specs.dedup();
        s += &format!(
            "<tr><td><a href=\"{0}.html\"><code>{0}</code></a></td><td>{1}</td><td>{2}</td><td>{3}</td><td>{4}</td><td>{5}</td></tr>",
            esc(&c.name),
            esc(&c.started),
            esc(&models.join(", ")),
            runs.len(),
            esc(&specs.join(", ")),
            if c.answer.is_some() { "yes" } else { "not yet" }
        );
    }
    s + "</tbody></table>"
}

fn pm(v: &[f64]) -> String {
    match stat(v) {
        None => String::new(),
        Some((m, Some(sd), _)) => format!("{m:.1} ±{sd:.1}"),
        Some((m, None, _)) => format!("{m:.1}"),
    }
}

fn campaign_page(c: &data::Campaign, db: &Db, pages: &[Page]) -> String {
    let mut s = format!("<h1><code>{}</code></h1><p class=\"meta\">started {}</p>", esc(&c.name), esc(&c.started));
    s += &format!("<h2>Question</h2><p>{}</p>", esc(&c.question));
    s += &match &c.answer {
        Some(a) => format!("<h2>Answer</h2><p>{}</p>", esc(a)),
        None => "<h2>Answer</h2><p class=\"muted\">Not drawn yet.</p>".to_string(),
    };
    let citing: Vec<&Page> = pages.iter().filter(|p| p.citations().contains(&c.name)).collect();
    if !citing.is_empty() {
        s += "<h2>Cited by</h2><ul>";
        for p in citing {
            s += &format!("<li><a href=\"../{}.html\">{}</a></li>", p.stem, esc(p.title()));
        }
        s += "</ul>";
    }
    s += "<h2>Runs</h2><p>t/s, mean ± sample standard deviation over the requests; prefill of the cold requests only; a total is the tokens of the requests that ran together over the window they shared, per repetition. Free MiB: after the first prefill, per device.</p>\
<div class=\"wide\"><table><thead><tr><th>run</th><th>label</th><th>status</th><th>model</th><th>llama.cpp</th><th>spec</th><th>requests</th><th>prefill</th><th>decode</th><th>prefill total</th><th>decode total</th><th>draft acc.</th><th>free MiB</th></tr></thead><tbody>";
    let runs: Vec<_> = db.runs_of(c).collect();
    for r in &runs {
        let multi = r.reqs.iter().any(|q| r.reqs.iter().filter(|o| o.rep == q.rep).count() > 1);
        let tot = |m: &str| if multi { pm(&r.samples(m)) } else { String::new() };
        s += &format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{} {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            r.id,
            esc(&r.label),
            esc(&r.status),
            esc(&r.model),
            esc(&r.quant),
            esc(&r.llama_cpp),
            esc(&r.spec()),
            r.reqs.len(),
            pm(&r.samples("prefill")),
            pm(&r.samples("decode")),
            tot("prefill-total"),
            tot("decode-total"),
            stat(&r.samples("acceptance")).map(|(m, _, _)| format!("{m:.0}%")).unwrap_or_default(),
            esc(&r.free.iter().map(|(d, f)| format!("{d} {f}")).collect::<Vec<_>>().join("; "))
        );
    }
    s += "</tbody></table></div><h2>Parameters</h2>";
    for r in &runs {
        s += &format!("<details><summary>run {} <code>{}</code></summary>", r.id, esc(&r.label));
        if !r.gpus.is_empty() {
            s += &format!(
                "<p>{}</p>",
                esc(&r.gpus.iter().map(|(d, g)| format!("{d}: {g}")).collect::<Vec<_>>().join(", "))
            );
        }
        s += "<table><thead><tr><th>scope</th><th>key</th><th>value</th></tr></thead><tbody>";
        for (sc, k, v) in &r.params {
            s += &format!("<tr><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>", esc(sc), esc(k), esc(v));
        }
        s += "</tbody></table></details>";
    }
    s
}
