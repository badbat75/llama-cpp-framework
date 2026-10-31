//! The pages of docs\: front matter, the checks of docs\AGENTS.md, and the
//! markdown rendered to HTML with charts drawn and citations linked.

use crate::chart::{self, Spec};
use crate::data::Db;
use crate::{esc, REPO};
use pulldown_cmark::{html, CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd};
use std::path::Path;

pub struct Page {
    /// The file name in docs\: `qwen3.8_27b.md`.
    pub file: String,
    /// The output name without `.html`: `index` for README.md.
    pub stem: String,
    pub meta: Vec<(String, String)>,
    pub body: String,
}

impl Page {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.meta.iter().find(|m| m.0 == k).map(|m| m.1.as_str())
    }

    /// A `[a, b]` list of the front matter (empty when absent).
    pub fn list(&self, k: &str) -> Vec<String> {
        let v = self.get(k).unwrap_or("").trim().trim_start_matches('[').trim_end_matches(']');
        v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    }

    pub fn title(&self) -> &str {
        self.get("title").unwrap_or(&self.file)
    }

    /// The campaigns the text cites, as "campaign `name`", in order of first use.
    pub fn citations(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut rest = self.body.as_str();
        while let Some(i) = rest.find("campaign `") {
            rest = &rest[i + "campaign `".len()..];
            if let Some(j) = rest.find('`') {
                let name = &rest[..j];
                if !name.is_empty() && !name.contains(char::is_whitespace) && !out.iter().any(|n| n == name) {
                    out.push(name.to_string());
                }
            }
        }
        out
    }

    /// The source of every `bench-chart` block.
    pub fn charts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur: Option<String> = None;
        for line in self.body.lines() {
            match &mut cur {
                None if line.trim_start().starts_with("```bench-chart") => cur = Some(String::new()),
                Some(_) if line.trim_start().starts_with("```") => out.push(cur.take().unwrap()),
                Some(c) => {
                    c.push_str(line);
                    c.push('\n');
                }
                None => {}
            }
        }
        out
    }
}

pub fn load_all(dir: &Path) -> Result<Vec<Page>, String> {
    let mut files: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|f| f.ends_with(".md") && f != "AGENTS.md")
        .collect();
    files.sort_by_key(|f| (f != "README.md", f.clone()));
    files
        .into_iter()
        .map(|file| {
            let text = std::fs::read_to_string(dir.join(&file))
                .map_err(|e| format!("docs/{file}: {e}"))?
                .replace("\r\n", "\n");
            let (meta, body) = split_front(&text);
            let stem = if file == "README.md" { "index".to_string() } else { file.trim_end_matches(".md").to_string() };
            Ok(Page { file, stem, meta, body: body.to_string() })
        })
        .collect()
}

fn split_front(s: &str) -> (Vec<(String, String)>, &str) {
    let Some(rest) = s.strip_prefix("---\n") else { return (Vec::new(), s) };
    let Some(end) = rest.find("\n---\n") else { return (Vec::new(), s) };
    let meta = rest[..end]
        .lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.split(" #").next().unwrap_or("").trim().to_string()))
        .collect();
    (meta, rest[end + 5..].trim_start_matches('\n'))
}

const REQUIRED_GPU: [&str; 6] = [
    "Runtime prerequisites",
    "Device facts",
    "Settings",
    "Choices that look better than they measure",
    "Checking a configuration",
    "Not yet measured (placeholders)",
];
const REQUIRED_MODEL: [&str; 4] = [
    "Model facts",
    "Recommended settings",
    "Choices that look better than they measure",
    "Not yet measured (placeholders)",
];

/// The rules of docs\AGENTS.md a program can check; each problem names its page.
pub fn check(p: &Page, db: &Db) -> Vec<String> {
    let mut errs = Vec::new();
    let mut err = |m: String| errs.push(format!("docs/{}: {m}", p.file));
    if p.meta.is_empty() {
        err("no front matter".into());
        return errs;
    }
    let ty = p.get("type").unwrap_or("");
    for k in ["title", "type", "updated"] {
        if p.get(k).is_none_or(str::is_empty) {
            err(format!("front matter has no `{k}`"));
        }
    }
    let required: &[&str] = match ty {
        "gpu" => &REQUIRED_GPU,
        "model" => &REQUIRED_MODEL,
        "reference" => &[],
        t => {
            err(format!("unknown type `{t}` (gpu, model or reference)"));
            &[]
        }
    };
    if ty == "gpu" || ty == "model" {
        for k in ["status", "platforms", "llama-cpp"] {
            if p.get(k).is_none_or(str::is_empty) {
                err(format!("front matter has no `{k}`"));
            }
        }
    }
    for g in p.list("gpus") {
        if !db.gpus.contains(&g) {
            err(format!("gpus: no GPU named `{g}` in bench.db"));
        }
    }
    for m in p.list("models") {
        if !db.models.iter().any(|n| n.starts_with(&m)) {
            err(format!("models: no model name in bench.db starts with `{m}`"));
        }
    }
    let heads: Vec<(u32, String)> = p
        .body
        .lines()
        .filter_map(|l| l.strip_prefix("## "))
        .filter_map(|h| h.split_once(". "))
        .filter_map(|(n, t)| Some((n.parse().ok()?, t.to_string())))
        .collect();
    if heads.iter().enumerate().any(|(i, h)| h.0 as usize != i + 1) {
        err(format!("sections are not numbered 1..{}", heads.len()));
    }
    for r in required {
        if !heads.iter().any(|(_, t)| t == r || t.starts_with(&format!("{r} ("))) {
            err(format!("no `## N. {r}` section"));
        }
    }
    for c in p.citations() {
        if db.campaign(&c).is_none() {
            err(format!("cites campaign `{c}`, which bench.db does not hold"));
        }
    }
    for (i, src) in p.charts().iter().enumerate() {
        if let Err(e) = Spec::parse(src).and_then(|s| chart::build(&s, db).map(|_| ())) {
            err(format!("bench-chart {}: {e}", i + 1));
        }
    }
    errs
}

/// The page body as HTML. `base` is the path back to the site root ("" or "../").
pub fn render(p: &Page, pages: &[Page], db: &Db, base: &str) -> String {
    let campaign_href = |c: &str| format!("{base}campaigns/{c}.html");
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let mut out: Vec<Event> = Vec::new();
    let mut chart_src: Option<String> = None;
    let mut after_campaign = false;
    for ev in Parser::new_ext(&p.body, opts) {
        let next = matches!(&ev, Event::Text(t) if t.ends_with("campaign "));
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref lang))) if lang.as_ref() == "bench-chart" => {
                chart_src = Some(String::new());
            }
            Event::Text(t) if chart_src.is_some() => chart_src.as_mut().unwrap().push_str(&t),
            Event::End(TagEnd::CodeBlock) if chart_src.is_some() => {
                let src = chart_src.take().unwrap();
                let html = match Spec::parse(&src).and_then(|s| chart::build(&s, db).map(|d| (s, d))) {
                    Ok((s, d)) => chart::svg(&s, &d, &campaign_href),
                    Err(e) => format!("<p class=\"bad\">chart: {}</p>", esc(&e)),
                };
                out.push(Event::Html(html.into()));
            }
            Event::Code(c) if after_campaign && db.campaign(&c).is_some() => {
                out.push(Event::Html(
                    format!("<a href=\"{}\"><code>{}</code></a>", campaign_href(&c), esc(&c)).into(),
                ));
            }
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => out.push(Event::Start(Tag::Link {
                link_type,
                dest_url: link(&dest_url, pages, base).into(),
                title,
                id,
            })),
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => out.push(Event::Start(Tag::Image {
                link_type,
                dest_url: link(&dest_url, pages, base).into(),
                title,
                id,
            })),
            ev => out.push(ev),
        }
        after_campaign = next;
    }
    let mut s = String::new();
    html::push_html(&mut s, out.into_iter());
    s
}

/// A link of the markdown, as the site serves it: a page of docs\ becomes its
/// HTML page, docs\assets\ stays relative, anything else in the repo goes to
/// GitHub.
fn link(dest: &CowStr, pages: &[Page], base: &str) -> String {
    let d = dest.as_ref();
    if d.contains("://") || d.starts_with('#') || d.starts_with("mailto:") {
        return d.to_string();
    }
    let (path, frag) = match d.split_once('#') {
        Some((p, f)) => (p, format!("#{f}")),
        None => (d, String::new()),
    };
    let mut parts: Vec<&str> = vec!["docs"];
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    let full = parts.join("/");
    if let Some(f) = full.strip_prefix("docs/") {
        if let Some(pg) = pages.iter().find(|pg| pg.file == f) {
            return format!("{base}{}.html{frag}", pg.stem);
        }
        if f.starts_with("assets/") {
            return format!("{base}{f}{frag}");
        }
    }
    let kind = if path.ends_with('/') { "tree" } else { "blob" };
    format!("{REPO}/{kind}/main/{full}{frag}")
}
