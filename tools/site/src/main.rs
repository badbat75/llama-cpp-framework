//! docs-site: the HTML site of docs\ and benchmarks\bench.db.
//!
//! `cargo run -p docs-site -- [--check] [--out DIR]`. Reads every page of
//! docs\ (README.md is the index; AGENTS.md and templates\ are not pages) and
//! bench.db read only, checks the rules of docs\AGENTS.md that a program can
//! (front matter, required sections, every cited campaign present, every
//! bench-chart block drawable) and fails on the first build with a problem.
//! Without `--check` it then writes the site: one page per guide, a page per
//! campaign a guide cites or charts (question, answer, runs, the guides citing
//! it), docs\assets\ and
//! the stylesheet and site.js, into DIR or `build/<os>/site/`. Static HTML
//! that GitHub Pages serves (.github\workflows\docs-site.yml); site.js only
//! adds chart tooltips and series toggles, everything reads without it.

mod chart;
mod data;
mod page;

use data::{stat, Db};
use page::Page;
use std::path::{Path, PathBuf};

pub const REPO: &str = "https://github.com/badbat75/llama-cpp-framework";

const CSS: &str = include_str!("style.css");
const JS: &str = include_str!("site.js");

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
        println!(
            "docs-site: {} pages, {} campaigns: ok",
            pages.len(),
            db.campaigns.len()
        );
        return Ok(());
    }
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let out = out.unwrap_or_else(|| root.join("build").join(os).join("site"));
    let _ = std::fs::remove_dir_all(&out);
    let w = |rel: &str, text: &str| -> Result<(), String> {
        let p = out.join(rel);
        std::fs::create_dir_all(p.parent().unwrap())
            .map_err(|e| format!("{}: {e}", p.display()))?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    let published = published(&db, &pages);
    w("style.css", CSS)?;
    w("site.js", JS)?;
    copy_dir(&root.join("docs/assets"), &out.join("assets"))?;
    for p in &pages {
        let mut body = if p.stem == "index" {
            hero(&pages, &db, &published)
        } else {
            meta_line(p)
        };
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
    w(
        "campaigns/index.html",
        &shell("Campaigns", "../", &campaign_index(&db, &published)),
    )?;
    for c in &published {
        w(
            &format!("campaigns/{}.html", c.name),
            &shell(&c.name, "../", &campaign_page(c, &db, &pages)),
        )?;
    }
    println!(
        "docs-site: {} pages, {} of {} campaigns -> {}",
        pages.len(),
        published.len(),
        db.campaigns.len(),
        out.display()
    );
    Ok(())
}

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(from) else {
        return Ok(());
    };
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
<title>{t} | Private AI Framework</title><link rel=\"stylesheet\" href=\"{base}style.css\"><script src=\"{base}site.js\" defer></script></head><body>\
<header><a class=\"home\" href=\"{base}index.html\">Private AI Framework</a><nav><a href=\"{base}index.html\">Guides</a><a href=\"{base}campaigns/index.html\">Campaigns</a><a href=\"{REPO}\">GitHub</a></nav></header>\
<main>{body}</main><footer>Generated from <code>docs/</code> and <code>benchmarks/bench.db</code>{c}. Every number traces to a campaign; the markdown on GitHub is the source.</footer></body></html>\n",
        t = esc(title),
        c = commit.unwrap_or_default()
    )
}

fn meta_line(p: &Page) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (k, label) in [
        ("type", ""),
        ("status", ""),
        ("platforms", ""),
        ("llama-cpp", "llama.cpp "),
        ("updated", "updated "),
    ] {
        if let Some(v) = p.get(k).filter(|v| !v.is_empty()) {
            let class = if k == "status" {
                format!(" status-{}", esc(v))
            } else {
                String::new()
            };
            parts.push(format!(
                "<span class=\"pill{class}\">{label}{}</span>",
                esc(v.trim_start_matches('[').trim_end_matches(']'))
            ));
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("<p class=\"meta\">{}</p>", parts.join(""))
}

/// The campaigns that get a page: those a guide cites or draws a chart from,
/// the only ones a link can reach. The rest stay in bench.db, off the site.
fn published<'a>(db: &'a Db, pages: &[Page]) -> Vec<&'a data::Campaign> {
    let used: std::collections::HashSet<String> = pages
        .iter()
        .flat_map(|p| {
            let charted = p
                .charts()
                .iter()
                .filter_map(|c| chart::Spec::parse(c).ok())
                .flat_map(|s| s.campaigns)
                .collect::<Vec<_>>();
            p.citations().into_iter().chain(charted)
        })
        .collect();
    db.campaigns
        .iter()
        .filter(|c| used.contains(&c.name))
        .collect()
}

/// The home page's banner: what the site is and how much bench.db holds.
fn hero(pages: &[Page], db: &Db, published: &[&data::Campaign]) -> String {
    let guides = pages
        .iter()
        .filter(|p| matches!(p.get("type"), Some("gpu" | "model")))
        .count();
    let all_runs: Vec<_> = published.iter().flat_map(|c| db.runs_of(c)).collect();
    let runs = all_runs.iter().filter(|r| r.status == "ok").count();
    let reqs: usize = all_runs.iter().map(|r| r.reqs.len()).sum();
    let stat =
        |n: usize, what: &str| format!("<div class=\"stat\"><b>{n}</b><span>{what}</span></div>");
    format!(
        "<section class=\"hero\"><h1>Private AI Framework</h1><p>Measured configuration guides for llama.cpp on AMD and NVIDIA cards: every value traces to a benchmark campaign in <code>benchmarks/bench.db</code>.</p><div class=\"stats\">{}{}{}{}{}</div></section>",
        stat(guides, "card and model guides"),
        stat(published.len(), "campaigns"),
        stat(runs, "runs"),
        stat(reqs, "requests measured"),
        stat(db.gpus.len(), "GPU models")
    )
}

fn campaign_index(db: &Db, published: &[&data::Campaign]) -> String {
    let mut s = String::from(
        "<h1>Campaigns</h1><p>The sets of runs of <code>benchmarks/bench.db</code> the guides cite, newest first: the question it was run to answer and, once drawn, the answer. A run recorded without a spec reads as <code>custom</code>.</p>\
<table><thead><tr><th>campaign</th><th>started</th><th>model</th><th>runs</th><th>spec</th><th>answer</th></tr></thead><tbody>",
    );
    for c in published {
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
    let mut s = format!(
        "<h1><code>{}</code></h1><p class=\"meta\">started {}</p>",
        esc(&c.name),
        esc(&c.started)
    );
    s += &format!("<h2>Question</h2><p>{}</p>", esc(&c.question));
    s += &match &c.answer {
        Some(a) => format!("<h2>Answer</h2><p>{}</p>", esc(a)),
        None => "<h2>Answer</h2><p class=\"muted\">Not drawn yet.</p>".to_string(),
    };
    let citing: Vec<&Page> = pages
        .iter()
        .filter(|p| p.citations().contains(&c.name))
        .collect();
    if !citing.is_empty() {
        s += "<h2>Cited by</h2><ul>";
        for p in citing {
            s += &format!(
                "<li><a href=\"../{}.html\">{}</a></li>",
                p.stem,
                esc(p.title())
            );
        }
        s += "</ul>";
    }
    s += "<h2>Runs</h2><p>t/s, mean ± sample standard deviation over the requests; prefill of the cold requests only; a total is the tokens of the requests that ran together over the window they shared, per repetition. Free MiB: after the first prefill, per device.</p>\
<div class=\"wide\"><table><thead><tr><th>run</th><th>label</th><th>status</th><th>model</th><th>llama.cpp</th><th>spec</th><th>requests</th><th>prefill</th><th>decode</th><th>prefill total</th><th>decode total</th><th>draft acc.</th><th>free MiB</th></tr></thead><tbody>";
    let runs: Vec<_> = db.runs_of(c).collect();
    for r in &runs {
        let multi = r
            .reqs
            .iter()
            .any(|q| r.reqs.iter().filter(|o| o.rep == q.rep).count() > 1);
        let tot = |m: &str| {
            if multi {
                pm(&r.samples(m))
            } else {
                String::new()
            }
        };
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
        s += &format!(
            "<details><summary>run {} <code>{}</code></summary>",
            r.id,
            esc(&r.label)
        );
        if !r.gpus.is_empty() {
            s += &format!(
                "<p>{}</p>",
                esc(&r
                    .gpus
                    .iter()
                    .map(|(d, g)| format!("{d}: {g}"))
                    .collect::<Vec<_>>()
                    .join(", "))
            );
        }
        s += "<table><thead><tr><th>scope</th><th>key</th><th>value</th></tr></thead><tbody>";
        for (sc, k, v) in &r.params {
            s += &format!(
                "<tr><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>",
                esc(sc),
                esc(k),
                esc(v)
            );
        }
        s += "</tbody></table></details>";
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What every page and the home banner call the project. The repository
    /// link (`REPO`) is not pinned here: it follows the GitHub repository.
    const PRODUCT: &str = "Private AI Framework";

    /// The old project name, split so this file does not spell it.
    const OLD_NAME: &str = concat!("llama.cpp", "-framework");

    fn empty_db() -> Db {
        Db {
            campaigns: Vec::new(),
            runs: Vec::new(),
            models: Vec::new(),
            gpus: Vec::new(),
        }
    }

    #[test]
    fn every_page_is_titled_and_headed_with_the_product_name() {
        let html = shell("Guides", "", "<p>body</p>");
        assert!(
            html.contains(&format!("<title>Guides | {PRODUCT}</title>")),
            "the <title> names {PRODUCT}: {html}"
        );
        assert!(
            html.contains(&format!(
                "<a class=\"home\" href=\"index.html\">{PRODUCT}</a>"
            )),
            "the header's home link reads {PRODUCT}: {html}"
        );
        let without_repo = html.replace(REPO, "");
        assert!(
            !without_repo.contains(OLD_NAME),
            "no {OLD_NAME} outside the repository link: {without_repo}"
        );
    }

    #[test]
    fn the_home_banner_names_the_product() {
        let html = hero(&[], &empty_db(), &[]);
        assert!(
            html.contains(&format!("<h1>{PRODUCT}</h1>")),
            "the hero's heading is {PRODUCT}: {html}"
        );
        assert!(
            !html.contains(OLD_NAME),
            "the hero still says {OLD_NAME}: {html}"
        );
    }
}
