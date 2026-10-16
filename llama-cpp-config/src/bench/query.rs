//! `bench query`, `bench answer`, `bench publish`: reading bench.db, closing a
//! campaign with its answer, and copying campaigns into the repo's database.
//!
//! Read-only: `query`
//! opens the file with `SQLITE_OPEN_READ_ONLY`, and `sql` can therefore not
//! write either.

use std::path::Path;

use rusqlite::types::Value;
use rusqlite::{params_from_iter, Connection};

use super::db;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Table,
    Csv,
    Json,
}

impl Format {
    pub fn from_str(s: &str) -> Format {
        match s {
            "csv" => Format::Csv,
            "json" => Format::Json,
            _ => Format::Table,
        }
    }
}

/// The run filters every listing takes.
#[derive(Clone, Debug, Default)]
pub struct Filters {
    pub run_id: Option<i64>,
    pub campaign: Option<String>,
    pub model: Option<String>,
    pub gpu: Option<String>,
    pub device: Option<String>,
    pub llama: Option<String>,
    pub status: Option<String>,
    pub since: Option<String>,
    /// `K=V` (equals) or `K~V` (contains), any scope.
    pub params: Vec<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Opts {
    pub format: Format,
    pub full: bool,
    /// Parameter columns added to `runs` and `summary`.
    pub show: Vec<String>,
}

pub enum Cmd {
    Campaigns,
    Models,
    Machines,
    Prompts,
    Runs,
    Summary,
    Results,
    Run(i64),
    Diff(i64, i64),
    Sql(String),
}

/// A result to print: headers and cells.
#[derive(Debug, Default, PartialEq)]
pub struct Table {
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => format!("{f:.1}"),
        Value::Text(s) => s.clone(),
        Value::Blob(b) => format!("<{} bytes>", b.len()),
    }
}

fn rows(conn: &Connection, sql: &str, args: &[Value]) -> rusqlite::Result<Vec<Vec<Value>>> {
    let mut st = conn.prepare(sql)?;
    let n = st.column_count();
    let out = st
        .query_map(params_from_iter(args.iter()), |r| {
            (0..n).map(|i| r.get::<_, Value>(i)).collect()
        })?
        .collect::<rusqlite::Result<Vec<Vec<Value>>>>()?;
    Ok(out)
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

/// WHERE clause and arguments for the run filters, on the run alias `r`.
pub fn where_clause(f: &Filters) -> Result<(String, Vec<Value>), String> {
    let mut w: Vec<String> = Vec::new();
    let mut a: Vec<Value> = Vec::new();
    let like = |s: &str| text(&format!("%{s}%"));
    if let Some(id) = f.run_id {
        w.push("r.id = ?".into());
        a.push(Value::Integer(id));
    }
    if let Some(c) = &f.campaign {
        w.push("r.campaign_id IN (SELECT id FROM campaign WHERE name LIKE ?)".into());
        a.push(like(c));
    }
    if let Some(m) = &f.model {
        w.push(
            "r.model_id IN (SELECT id FROM model WHERE name LIKE ? OR file LIKE ? OR quant LIKE ?)"
                .into(),
        );
        a.extend([like(m), like(m), like(m)]);
    }
    if let Some(g) = &f.gpu {
        w.push("r.id IN (SELECT run_id FROM run_gpu WHERE gpu LIKE ?)".into());
        a.push(like(g));
    }
    if let Some(d) = &f.device {
        w.push(
            "r.id IN (SELECT run_id FROM run_param WHERE scope = 'preset' AND key = 'device' \
             AND value LIKE ?)"
                .into(),
        );
        a.push(like(d));
    }
    if let Some(l) = &f.llama {
        w.push("r.software_id IN (SELECT id FROM software WHERE llama_cpp_tag = ?)".into());
        a.push(text(l));
    }
    if let Some(s) = &f.status {
        w.push("r.status = ?".into());
        a.push(text(s));
    }
    if let Some(s) = &f.since {
        w.push("r.started_utc >= ?".into());
        a.push(text(s));
    }
    for p in &f.params {
        let (k, op, v) = match (p.find('~'), p.find('=')) {
            (Some(i), Some(j)) if i < j => (&p[..i], "LIKE", like(&p[i + 1..])),
            (Some(i), None) => (&p[..i], "LIKE", like(&p[i + 1..])),
            (_, Some(j)) => (&p[..j], "=", text(&p[j + 1..])),
            _ => return Err(format!("--param {p}: expected K=V or K~V")),
        };
        w.push(format!(
            "r.id IN (SELECT run_id FROM run_param WHERE key = ? AND value {op} ?)"
        ));
        a.extend([text(k), v]);
    }
    let clause = if w.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", w.join(" AND "))
    };
    Ok((clause, a))
}

fn param_cells(conn: &Connection, run: i64, keys: &[String]) -> rusqlite::Result<Vec<String>> {
    keys.iter()
        .map(|k| {
            conn.query_row(
                "SELECT group_concat(value, ' | ') FROM run_param WHERE run_id = ?1 AND key = ?2",
                rusqlite::params![run, k],
                |r| r.get::<_, Option<String>>(0),
            )
            .map(Option::unwrap_or_default)
        })
        .collect()
}

pub fn run(conn: &Connection, cmd: &Cmd, f: &Filters, o: &Opts) -> Result<Table, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let (w, a) = where_clause(f)?;
    let table = |head: &[&str], data: Vec<Vec<Value>>| Table {
        head: head.iter().map(|s| s.to_string()).collect(),
        rows: data.iter().map(|r| r.iter().map(cell).collect()).collect(),
    };
    Ok(match cmd {
        Cmd::Campaigns => table(
            &["campaign", "started", "runs", "ok", "question", "answer"],
            rows(
                conn,
                &format!(
                    "SELECT c.name, c.started, count(r.id), sum(r.status = 'ok'), c.question, \
                     c.answer FROM campaign c JOIN run r ON r.campaign_id = c.id {w} \
                     GROUP BY c.id ORDER BY c.started, c.name"
                ),
                &a,
            )
            .map_err(e)?,
        ),
        Cmd::Models => table(
            &[
                "model",
                "quant",
                "arch",
                "mtp",
                "GiB",
                "file",
                "runs",
                "campaigns",
            ],
            rows(
                conn,
                &format!(
                    "SELECT m.name, m.quant, m.arch, m.mtp, \
                     printf('%.2f', m.size_bytes / 1073741824.0), m.file, count(r.id), \
                     count(DISTINCT r.campaign_id) FROM model m JOIN run r ON r.model_id = m.id \
                     {w} GROUP BY m.id ORDER BY m.name, m.quant"
                ),
                &a,
            )
            .map_err(e)?,
        ),
        Cmd::Machines => machines(conn).map_err(e)?,
        Cmd::Prompts => table(
            &["id", "prompt", "tokens", "chars", "runs", "text (start)"],
            rows(
                conn,
                "SELECT p.id, p.name, p.tokens, length(p.text), count(DISTINCT x.run_id), \
                 replace(substr(p.text, 1, 60), char(10), ' ') FROM prompt p \
                 LEFT JOIN result x ON x.prompt_id = p.id GROUP BY p.id ORDER BY p.id",
                &[],
            )
            .map_err(e)?,
        ),
        Cmd::Runs => {
            let data = rows(
                conn,
                &format!(
                    "SELECT r.id, c.name, r.label, r.started_utc, \
                     m.name || ' ' || coalesce(m.quant, ''), \
                     s.llama_cpp_tag, r.status, coalesce(r.error, r.notes) \
                     FROM run r JOIN campaign c ON c.id = r.campaign_id \
                     JOIN model m ON m.id = r.model_id JOIN software s ON s.id = r.software_id \
                     {w} ORDER BY c.started, c.name, r.id"
                ),
                &a,
            )
            .map_err(e)?;
            let mut head: Vec<String> = [
                "id",
                "campaign",
                "label",
                "started",
                "model",
                "llama.cpp",
                "status",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect();
            head.extend(o.show.iter().cloned());
            head.push("error / notes".into());
            let mut out = Table {
                head,
                rows: Vec::new(),
            };
            for r in data {
                let id = match r[0] {
                    Value::Integer(i) => i,
                    _ => 0,
                };
                let mut row: Vec<String> = r[..7].iter().map(cell).collect();
                row.extend(param_cells(conn, id, &o.show).map_err(e)?);
                row.push(cell(&r[7]).replace('\n', "; "));
                out.rows.push(row);
            }
            out
        }
        Cmd::Summary => summary(conn, &w, &a, &o.show).map_err(e)?,
        Cmd::Results => results(conn, f, &w, &a).map_err(e)?,
        Cmd::Run(id) => return run_detail(conn, *id, o),
        Cmd::Diff(x, y) => diff(conn, *x, *y).map_err(e)?,
        Cmd::Sql(q) => {
            let mut st = conn.prepare(q).map_err(|e| format!("sql: {e}"))?;
            let head: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
            let n = head.len();
            let data = st
                .query_map([], |r| (0..n).map(|i| r.get::<_, Value>(i)).collect())
                .map_err(|e| format!("sql: {e}"))?
                .collect::<rusqlite::Result<Vec<Vec<Value>>>>()
                .map_err(|e| format!("sql: {e}"))?;
            Table {
                head,
                rows: data.iter().map(|r| r.iter().map(cell).collect()).collect(),
            }
        }
    })
}

fn machines(conn: &Connection) -> rusqlite::Result<Table> {
    let ms = rows(
        conn,
        "SELECT id, cpu, cores, threads, ram_mib, ram_type, ram_speed_mts, ram_modules, os, kernel, \
         (SELECT count(*) FROM run WHERE machine_id = machine.id) FROM machine ORDER BY id",
        &[],
    )?;
    let mut out = Table {
        head: ["id", "cpu", "cores", "ram", "modules", "gpus", "os", "runs"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rows: Vec::new(),
    };
    for m in ms {
        let gpus = rows(
            conn,
            "SELECT name || ' ' || vram_mib || ' MiB (' || backend || \
             coalesce(' ' || driver, '') || ')' FROM gpu WHERE machine_id = ? ORDER BY name",
            &[m[0].clone()],
        )?
        .iter()
        .map(|g| cell(&g[0]))
        .collect::<Vec<_>>()
        .join("; ");
        let ram_gib = match m[4] {
            Value::Integer(i) => (i as f64 / 1024.0).round() as i64,
            _ => 0,
        };
        let mut ram = format!("{ram_gib} GiB");
        if !matches!(m[6], Value::Null) {
            ram.push_str(&format!(" {} {} MT/s", cell(&m[5]), cell(&m[6])));
        }
        out.rows.push(vec![
            cell(&m[0]),
            cell(&m[1]),
            format!("{}c/{}t", cell(&m[2]), cell(&m[3])),
            ram,
            cell(&m[7]),
            gpus,
            format!("{}, {}", cell(&m[8]), cell(&m[9])),
            cell(&m[10]),
        ]);
    }
    Ok(out)
}

/// `mean +- sd (n)` from a sum, a sum of squares and a count.
fn mean_sd(sum: f64, sq: f64, n: i64) -> String {
    if n == 0 {
        return String::new();
    }
    let mean = sum / n as f64;
    if n < 2 {
        return format!("{mean:.1}");
    }
    let var = ((sq - n as f64 * mean * mean) / (n - 1) as f64).max(0.0);
    format!("{mean:.1} +- {:.1}", var.sqrt())
}

/// One line per run and prompt (live) or test (llama-bench): the means the
/// tab shows, with their spread and count.
fn summary(conn: &Connection, w: &str, a: &[Value], show: &[String]) -> rusqlite::Result<Table> {
    let data = rows(
        conn,
        &format!(
            "SELECT r.id, c.name, r.label, r.status, coalesce(x.test, p.name, ''), count(x.id), \
             avg(x.prompt_tokens), \
             sum(x.prefill_tps), sum(x.prefill_tps * x.prefill_tps), count(x.prefill_tps), \
             sum(x.decode_tps), sum(x.decode_tps * x.decode_tps), count(x.decode_tps), \
             avg(x.reasoning_tps), avg(x.answer_tps), sum(x.draft_acc), sum(x.draft_gen), \
             (SELECT group_concat(device || ' ' || free_mib, '; ') FROM run_device \
              WHERE run_id = r.id AND free_mib IS NOT NULL) \
             FROM run r JOIN campaign c ON c.id = r.campaign_id \
             JOIN result x ON x.run_id = r.id LEFT JOIN prompt p ON p.id = x.prompt_id \
             {w} GROUP BY r.id, coalesce(x.test, p.name, '') \
             ORDER BY c.started, c.name, r.id, min(x.id)"
        ),
        a,
    )?;
    let f = |v: &Value| match v {
        Value::Real(f) => *f,
        Value::Integer(i) => *i as f64,
        _ => 0.0,
    };
    let i = |v: &Value| match v {
        Value::Integer(i) => *i,
        _ => 0,
    };
    let mut head: Vec<String> = ["id", "campaign", "label", "status"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    head.extend(show.iter().cloned());
    head.extend(
        [
            "prompt / test",
            "n",
            "prompt tok",
            "prefill t/s",
            "decode t/s",
            "reasoning",
            "answer",
            "draft acc",
            "free MiB",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    let mut out = Table {
        head,
        rows: Vec::new(),
    };
    for r in data {
        let mut row: Vec<String> = r[..4].iter().map(cell).collect();
        row.extend(param_cells(conn, i(&r[0]), show)?);
        let tok = match r[6] {
            Value::Real(t) => format!("{t:.0}"),
            _ => String::new(),
        };
        let acc = if i(&r[16]) > 0 {
            format!("{:.0}%", 100.0 * f(&r[15]) / f(&r[16]))
        } else {
            String::new()
        };
        row.extend([
            cell(&r[4]),
            cell(&r[5]),
            tok,
            mean_sd(f(&r[7]), f(&r[8]), i(&r[9])),
            mean_sd(f(&r[10]), f(&r[11]), i(&r[12])),
            cell(&r[13]),
            cell(&r[14]),
            acc,
            cell(&r[17]),
        ]);
        out.rows.push(row);
    }
    Ok(out)
}

fn results(conn: &Connection, f: &Filters, w: &str, a: &[Value]) -> rusqlite::Result<Table> {
    let mut w = w.to_string();
    let mut a = a.to_vec();
    if let Some(p) = &f.prompt {
        w.push_str(if w.is_empty() { " WHERE " } else { " AND " });
        w.push_str("(p.name = ? OR x.test = ?)");
        a.extend([text(p), text(p)]);
    }
    let data = rows(
        conn,
        &format!(
            "SELECT r.id, c.name, r.label, coalesce(x.test, p.name), x.rep, x.prompt_tokens, \
             x.prefill_tps, x.gen_tokens, x.decode_tps, x.draft_gen, x.draft_acc \
             FROM result x JOIN run r ON r.id = x.run_id JOIN campaign c ON c.id = r.campaign_id \
             LEFT JOIN prompt p ON p.id = x.prompt_id {w} ORDER BY c.started, c.name, r.id, x.id"
        ),
        &a,
    )?;
    Ok(Table {
        head: [
            "run",
            "campaign",
            "label",
            "prompt / test",
            "rep",
            "prompt tok",
            "prefill t/s",
            "gen tok",
            "decode t/s",
            "drafts",
            "accepted",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        rows: data.iter().map(|r| r.iter().map(cell).collect()).collect(),
    })
}

fn diff(conn: &Connection, x: i64, y: i64) -> rusqlite::Result<Table> {
    let get = |id: i64| -> rusqlite::Result<std::collections::BTreeMap<(String, String), String>> {
        Ok(rows(
            conn,
            "SELECT scope, key, value FROM run_param WHERE run_id = ?",
            &[Value::Integer(id)],
        )?
        .into_iter()
        .map(|r| ((cell(&r[0]), cell(&r[1])), cell(&r[2])))
        .collect())
    };
    let (a, b) = (get(x)?, get(y)?);
    let keys: std::collections::BTreeSet<&(String, String)> = a.keys().chain(b.keys()).collect();
    let unset = "(unset)".to_string();
    Ok(Table {
        head: vec![
            "scope".into(),
            "key".into(),
            format!("run {x}"),
            format!("run {y}"),
        ],
        rows: keys
            .into_iter()
            .filter(|k| a.get(*k) != b.get(*k))
            .map(|k| {
                vec![
                    k.0.clone(),
                    k.1.clone(),
                    a.get(k).unwrap_or(&unset).clone(),
                    b.get(k).unwrap_or(&unset).clone(),
                ]
            })
            .collect(),
    })
}

/// Everything about one run, as text (or one JSON document). Returned as a
/// one-column table so the printer stays the single way out.
fn run_detail(conn: &Connection, id: i64, o: &Opts) -> Result<Table, String> {
    let e = |e: rusqlite::Error| e.to_string();
    let one = |sql: &str, id: i64| -> Result<Option<Vec<(String, Value)>>, String> {
        let mut st = conn.prepare(sql).map_err(e)?;
        let names: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
        let mut q = st.query([id]).map_err(e)?;
        let Some(r) = q.next().map_err(e)? else {
            return Ok(None);
        };
        Ok(Some(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| (n.clone(), r.get::<_, Value>(i).unwrap_or(Value::Null)))
                .collect(),
        ))
    };
    let get = |row: &[(String, Value)], k: &str| -> String {
        row.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| cell(v))
            .unwrap_or_default()
    };
    let r = one(
        "SELECT r.*, c.name AS campaign, c.question, c.answer FROM run r \
         JOIN campaign c ON c.id = r.campaign_id WHERE r.id = ?",
        id,
    )?
    .ok_or_else(|| format!("no run {id}"))?;
    let num = |k: &str| get(&r, k).parse::<i64>().unwrap_or(0);
    let m = one("SELECT * FROM machine WHERE id = ?", num("machine_id"))?.unwrap_or_default();
    let s = one("SELECT * FROM software WHERE id = ?", num("software_id"))?.unwrap_or_default();
    let mo = one("SELECT * FROM model WHERE id = ?", num("model_id"))?.unwrap_or_default();
    let params = rows(
        conn,
        "SELECT scope, key, value FROM run_param WHERE run_id = ? ORDER BY scope, key",
        &[Value::Integer(id)],
    )
    .map_err(e)?;
    let gpus = rows(
        conn,
        "SELECT name, vram_mib, backend, driver FROM gpu WHERE machine_id = ? ORDER BY name",
        &[Value::Integer(num("machine_id"))],
    )
    .map_err(e)?;
    let used = rows(
        conn,
        "SELECT device, gpu FROM run_gpu WHERE run_id = ? ORDER BY device",
        &[Value::Integer(id)],
    )
    .map_err(e)?;
    let filters = Filters {
        run_id: Some(id),
        ..Default::default()
    };
    let (w, a) = where_clause(&filters)?;
    let res = results(conn, &filters, &w, &a).map_err(e)?;

    if o.format == Format::Json {
        let obj = |row: &[(String, Value)]| -> serde_json::Value {
            row.iter()
                .map(|(k, v)| {
                    let j = match v {
                        Value::Null => serde_json::Value::Null,
                        Value::Integer(i) => serde_json::json!(i),
                        Value::Real(f) => serde_json::json!(f),
                        Value::Text(t) => serde_json::json!(t),
                        Value::Blob(_) => serde_json::Value::Null,
                    };
                    (k.clone(), j)
                })
                .collect::<serde_json::Map<_, _>>()
                .into()
        };
        let mut p = serde_json::Map::new();
        for row in &params {
            let scope = p
                .entry(cell(&row[0]))
                .or_insert_with(|| serde_json::json!({}));
            scope[cell(&row[1])] = serde_json::json!(cell(&row[2]));
        }
        let results: Vec<serde_json::Value> = res
            .rows
            .iter()
            .map(|r| {
                res.head
                    .iter()
                    .zip(r)
                    .map(|(h, c)| (h.clone(), serde_json::json!(c)))
                    .collect::<serde_json::Map<_, _>>()
                    .into()
            })
            .collect();
        let doc = serde_json::json!({
            "run": obj(&r), "machine": obj(&m), "software": obj(&s), "model": obj(&mo),
            "gpus": gpus.iter().map(|g| g.iter().map(cell).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "devices": used.iter().map(|g| g.iter().map(cell).collect::<Vec<_>>()).collect::<Vec<_>>(),
            "params": p, "results": results,
        });
        return Ok(Table {
            head: Vec::new(),
            rows: vec![vec![serde_json::to_string_pretty(&doc).unwrap_or_default()]],
        });
    }

    let mut t: Vec<String> = Vec::new();
    let ended = get(&r, "ended_utc");
    t.push(format!(
        "run {id}: {}  [{}]  {}{}",
        get(&r, "label"),
        get(&r, "status"),
        get(&r, "started_utc"),
        if ended.is_empty() {
            String::new()
        } else {
            format!(" to {ended}")
        }
    ));
    t.push(format!(
        "campaign: {}\n  question: {}\n  answer: {}",
        get(&r, "campaign"),
        get(&r, "question"),
        get(&r, "answer")
    ));
    for k in ["error", "notes"] {
        let v = get(&r, k);
        if !v.is_empty() {
            t.push(format!("{k}: {}", v.replace('\n', "\n  ")));
        }
    }
    let gib = get(&mo, "size_bytes").parse::<f64>().unwrap_or(0.0) / 1073741824.0;
    t.push(format!(
        "model: {} {} ({}, {gib:.2} GiB{}) {}",
        get(&mo, "name"),
        get(&mo, "quant"),
        get(&mo, "arch"),
        if get(&mo, "mtp") == "1" { ", MTP" } else { "" },
        get(&mo, "file")
    ));
    t.push(format!(
        "llama.cpp {} (build {}, {}), framework {}, ROCm {}, CUDA {}, Vulkan SDK {}, patches {}",
        get(&s, "llama_cpp_tag"),
        get(&s, "llama_cpp_build"),
        get(&s, "llama_cpp_commit"),
        get(&s, "framework_version"),
        get(&s, "rocm"),
        get(&s, "cuda"),
        get(&s, "vulkan_sdk"),
        match get(&s, "patches") {
            p if p.is_empty() => "none".to_string(),
            p => p,
        }
    ));
    let ram_gib = (get(&m, "ram_mib").parse::<f64>().unwrap_or(0.0) / 1024.0).round();
    t.push(format!(
        "machine: {} {}c/{}t, {ram_gib} GiB {} {} MT/s, {}, {}",
        get(&m, "cpu"),
        get(&m, "cores"),
        get(&m, "threads"),
        get(&m, "ram_type"),
        get(&m, "ram_speed_mts"),
        get(&m, "os"),
        get(&m, "kernel")
    ));
    for g in &gpus {
        t.push(format!(
            "  gpu: {}, {} MiB, {}, {}",
            cell(&g[0]),
            cell(&g[1]),
            cell(&g[2]),
            cell(&g[3])
        ));
    }
    if !used.is_empty() {
        let list: Vec<String> = used
            .iter()
            .map(|u| format!("{} = {}", cell(&u[0]), cell(&u[1])))
            .collect();
        t.push(format!("computed on: {}", list.join(", ")));
    }
    let mut scope = String::new();
    for p in &params {
        let sc = cell(&p[0]);
        if sc != scope {
            t.push(format!("{sc}:"));
            scope = sc;
        }
        t.push(format!("  {} = {}", cell(&p[1]), cell(&p[2])));
    }
    let mut out = Table {
        head: Vec::new(),
        rows: vec![vec![t.join("\n")]],
    };
    out.rows.push(vec![render(
        &res,
        &Opts {
            full: true,
            ..o.clone()
        },
    )]);
    Ok(out)
}

/// The table as the format asks. A headless table (`run`) is printed as its
/// cells, one block each.
pub fn render(t: &Table, o: &Opts) -> String {
    if t.head.is_empty() {
        return t
            .rows
            .iter()
            .map(|r| r.join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
    }
    match o.format {
        Format::Json => {
            let arr: Vec<serde_json::Value> = t
                .rows
                .iter()
                .map(|r| {
                    t.head
                        .iter()
                        .zip(r)
                        .map(|(h, c)| (h.clone(), serde_json::json!(c)))
                        .collect::<serde_json::Map<_, _>>()
                        .into()
                })
                .collect();
            serde_json::to_string_pretty(&arr).unwrap_or_default()
        }
        Format::Csv => {
            let esc = |c: &str| {
                if c.contains([',', '"', '\n']) {
                    format!("\"{}\"", c.replace('"', "\"\""))
                } else {
                    c.to_string()
                }
            };
            std::iter::once(&t.head)
                .chain(&t.rows)
                .map(|r| r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(","))
                .collect::<Vec<_>>()
                .join("\n")
        }
        Format::Table => {
            let cut = |c: &str| -> String {
                let c = c.replace('\n', "; ");
                if o.full || c.chars().count() <= 70 {
                    c
                } else {
                    format!("{}...", c.chars().take(67).collect::<String>())
                }
            };
            let rows: Vec<Vec<String>> = t
                .rows
                .iter()
                .map(|r| r.iter().map(|c| cut(c)).collect())
                .collect();
            let n = t.head.len();
            let width: Vec<usize> = (0..n)
                .map(|i| {
                    std::iter::once(&t.head[i])
                        .chain(rows.iter().filter_map(|r| r.get(i)))
                        .map(|c| c.chars().count())
                        .max()
                        .unwrap_or(0)
                })
                .collect();
            let numeric: Vec<bool> = (0..n)
                .map(|i| {
                    !rows.is_empty()
                        && rows.iter().all(|r| {
                            let c = r.get(i).map(String::as_str).unwrap_or("");
                            c.is_empty() || c.parse::<f64>().is_ok()
                        })
                })
                .collect();
            let line = |r: &[String]| -> String {
                r.iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let pad = width[i].saturating_sub(c.chars().count());
                        if numeric[i] {
                            format!("{}{c}", " ".repeat(pad))
                        } else {
                            format!("{c}{}", " ".repeat(pad))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("  ")
                    .trim_end()
                    .to_string()
            };
            let mut out = vec![
                line(&t.head),
                width
                    .iter()
                    .map(|w| "-".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("  "),
            ];
            out.extend(rows.iter().map(|r| line(r)));
            out.push(format!("({} rows)", rows.len()));
            out.join("\n")
        }
    }
}

// ── answer, publish ──────────────────────────────────────────────────────

pub fn answer(path: &Path, campaign: &str, text: &str) -> Result<(), String> {
    let conn = db::open(path)?;
    db::set_answer(&conn, campaign, text)
}

/// Copy whole campaigns, with everything their runs point at, from one
/// database into another (the repo's). Rows are matched by their natural
/// keys, never by id, so the two files number their rows independently. A
/// campaign the target already has is refused rather than merged: publishing
/// twice must not double its runs. Returns the runs copied per campaign.
pub fn publish(
    from: &Path,
    to: &Path,
    campaigns: &[String],
) -> Result<Vec<(String, usize)>, String> {
    if from == to {
        return Err("the source and the target are the same file".into());
    }
    let src = db::open_ro(from)?;
    let mut dst = db::open(to)?;
    let e = |e: rusqlite::Error| e.to_string();
    let tx = dst.transaction().map_err(e)?;
    let mut done = Vec::new();
    for name in campaigns {
        let Some(c) = rows(
            &src,
            "SELECT id, question, started, answer, notes, report FROM campaign WHERE name = ?",
            &[text(name)],
        )
        .map_err(e)?
        .into_iter()
        .next() else {
            return Err(format!("no campaign `{name}` in {}", from.display()));
        };
        if tx
            .prepare("SELECT 1 FROM campaign WHERE name = ?1")
            .and_then(|mut s| s.exists([name]))
            .map_err(e)?
        {
            return Err(format!(
                "campaign `{name}` is already in {}; nothing was copied",
                to.display()
            ));
        }
        tx.execute(
            "INSERT INTO campaign (name, question, started, answer, notes, report) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![name, c[1], c[2], c[3], scrub(&c[4]), scrub(&c[5])],
        )
        .map_err(e)?;
        let cid = tx.last_insert_rowid();
        let runs = rows(
            &src,
            "SELECT id, label, started_utc, machine_id, software_id, model_id, tool, status, error, \
             notes, log, ended_utc, points FROM run WHERE campaign_id = ? ORDER BY id",
            &[c[0].clone()],
        )
        .map_err(e)?;
        for r in &runs {
            let machine = copy_machine(&src, &tx, &r[3]).map_err(e)?;
            let software = copy_row(
                &src,
                &tx,
                "software",
                &[
                    "llama_cpp_tag",
                    "llama_cpp_build",
                    "llama_cpp_commit",
                    "framework_version",
                    "rocm",
                    "cuda",
                    "vulkan_sdk",
                    "patches",
                ],
                &[
                    "llama_cpp_tag",
                    "llama_cpp_build",
                    "llama_cpp_commit",
                    "framework_version",
                    "rocm",
                    "cuda",
                    "vulkan_sdk",
                    "patches",
                ],
                &r[4],
            )
            .map_err(e)?;
            let model = copy_row(
                &src,
                &tx,
                "model",
                &[
                    "file",
                    "size_bytes",
                    "name",
                    "arch",
                    "quant",
                    "params_b",
                    "layers",
                    "experts",
                    "mtp",
                    "notes",
                ],
                &["file", "size_bytes"],
                &r[5],
            )
            .map_err(e)?;
            // The log and notes pass `public_text` again: a row written
            // before it existed may still name local paths.
            tx.execute(
                "INSERT INTO run (campaign_id, label, started_utc, machine_id, software_id, \
                 model_id, tool, status, error, notes, log, ended_utc, points) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                rusqlite::params![
                    cid,
                    r[1],
                    r[2],
                    machine,
                    software,
                    model,
                    r[6],
                    r[7],
                    r[8],
                    scrub(&r[9]),
                    scrub(&r[10]),
                    r[11],
                    r[12]
                ],
            )
            .map_err(e)?;
            let rid = tx.last_insert_rowid();
            copy_children(&src, &tx, &r[0], rid).map_err(e)?;
        }
        done.push((name.clone(), runs.len()));
    }
    tx.commit().map_err(e)?;
    Ok(done)
}

/// A free-text cell through `db::public_text`; any other value as it is.
fn scrub(v: &Value) -> Value {
    match v {
        Value::Text(t) => Value::Text(db::public_text(t)),
        other => other.clone(),
    }
}

/// Copy the row `id` of `table` unless the target has one with the same `key`
/// columns; return the target's id either way.
fn copy_row(
    src: &Connection,
    dst: &Connection,
    table: &str,
    cols: &[&str],
    key: &[&str],
    id: &Value,
) -> rusqlite::Result<i64> {
    let row = rows(
        src,
        &format!("SELECT {} FROM {table} WHERE id = ?", cols.join(", ")),
        std::slice::from_ref(id),
    )?
    .into_iter()
    .next()
    .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let val = |c: &str| row[cols.iter().position(|x| *x == c).unwrap()].clone();
    let cond: Vec<String> = key.iter().map(|k| format!("{k} IS ?")).collect();
    let keyvals: Vec<Value> = key.iter().map(|k| val(k)).collect();
    if let Some(found) = rows(
        dst,
        &format!("SELECT id FROM {table} WHERE {}", cond.join(" AND ")),
        &keyvals,
    )?
    .into_iter()
    .next()
    {
        if let Value::Integer(i) = found[0] {
            return Ok(i);
        }
    }
    let marks: Vec<&str> = cols.iter().map(|_| "?").collect();
    dst.execute(
        &format!(
            "INSERT INTO {table} ({}) VALUES ({})",
            cols.join(", "),
            marks.join(", ")
        ),
        params_from_iter(row.iter()),
    )?;
    Ok(dst.last_insert_rowid())
}

fn copy_machine(src: &Connection, dst: &Connection, id: &Value) -> rusqlite::Result<i64> {
    let cols = [
        "fingerprint",
        "cpu",
        "cores",
        "threads",
        "ram_mib",
        "ram_type",
        "ram_speed_mts",
        "ram_modules",
        "os",
        "kernel",
    ];
    let new = copy_row(src, dst, "machine", &cols, &["fingerprint"], id)?;
    for g in rows(
        src,
        "SELECT name, vram_mib, backend, driver FROM gpu WHERE machine_id = ?",
        std::slice::from_ref(id),
    )? {
        dst.execute(
            "INSERT OR IGNORE INTO gpu (machine_id, name, vram_mib, backend, driver) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![new, g[0], g[1], g[2], g[3]],
        )?;
    }
    Ok(new)
}

fn copy_children(
    src: &Connection,
    dst: &Connection,
    old: &Value,
    new: i64,
) -> rusqlite::Result<()> {
    let one = [old.clone()];
    for p in rows(
        src,
        "SELECT scope, key, value FROM run_param WHERE run_id = ?",
        &one,
    )? {
        dst.execute(
            "INSERT INTO run_param (run_id, scope, key, value) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![new, p[0], p[1], p[2]],
        )?;
    }
    for g in rows(
        src,
        "SELECT device, gpu FROM run_gpu WHERE run_id = ?",
        &one,
    )? {
        dst.execute(
            "INSERT INTO run_gpu (run_id, device, gpu) VALUES (?1, ?2, ?3)",
            rusqlite::params![new, g[0], g[1]],
        )?;
    }
    for d in rows(
        src,
        "SELECT device, free_mib, model_mib, kv_mib, compute_mib FROM run_device WHERE run_id = ?",
        &one,
    )? {
        dst.execute(
            "INSERT INTO run_device (run_id, device, free_mib, model_mib, kv_mib, compute_mib) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![new, d[0], d[1], d[2], d[3], d[4]],
        )?;
    }
    for x in rows(
        src,
        "SELECT prompt_id, rep, test, temperature, cache_prompt, prompt_tokens, prefill_tps, \
         gen_tokens, decode_tps, draft_gen, draft_acc, cache_n, reasoning_tps, answer_tps, detail \
         FROM result WHERE run_id = ? ORDER BY id",
        &one,
    )? {
        let prompt = match &x[0] {
            Value::Null => Value::Null,
            id => Value::Integer(copy_row(
                src,
                dst,
                "prompt",
                &["sha256", "name", "tokens", "text"],
                &["sha256"],
                id,
            )?),
        };
        let mut vals = vec![Value::Integer(new), prompt];
        vals.extend(x[1..].iter().cloned());
        dst.execute(
            "INSERT INTO result (run_id, prompt_id, rep, test, temperature, cache_prompt, \
             prompt_tokens, prefill_tps, gen_tokens, decode_tps, draft_gen, draft_acc, cache_n, \
             reasoning_tps, answer_tps, detail) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params_from_iter(vals.iter()),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::db::{self, Machine, Model, NewRun, Sample, Software};

    fn seeded(path: &Path) {
        let c = db::open(path).unwrap();
        let cid = db::campaign_id(&c, "camp", "which ctx?", "2026-10-16").unwrap();
        let mid = db::machine_id(
            &c,
            &Machine {
                cpu: "cpu".into(),
                gpus: vec![db::Gpu {
                    name: "R9700".into(),
                    vram_mib: 32624,
                    backend: "ROCm".into(),
                    driver: None,
                }],
                ..Default::default()
            },
        )
        .unwrap();
        let sid = db::software_id(
            &c,
            &Software {
                llama_cpp_tag: "v0.7.0".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let model = db::model_id(
            &c,
            &Model {
                file: "m.gguf".into(),
                size_bytes: 1 << 30,
                name: "m".into(),
                quant: Some("Q8_0".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let prompt = db::prompt_id(&c, "long", "a long prompt").unwrap();
        for (label, ctx, decode) in [("A", "163840", [35.0, 35.4]), ("B", "262144", [31.5, 31.5])] {
            let run = db::insert_run(
                &c,
                &NewRun {
                    campaign: cid,
                    label,
                    started_utc: "2026-10-16 18:00:00 UTC",
                    ended_utc: "2026-10-16 18:05:00 UTC",
                    machine: mid,
                    software: sid,
                    model,
                    tool: "live",
                    notes: "",
                    log: "local chatter",
                    points: r#"[{"test":"decode","mean":35.2,"sd":0.3,"n":2,"note":""}]"#,
                },
            )
            .unwrap();
            db::add_params(
                &c,
                run,
                "preset",
                &[
                    ("ctx-size".into(), ctx.into()),
                    ("device".into(), "ROCm0".into()),
                ],
            )
            .unwrap();
            db::add_gpus(&c, run, &[("ROCm0".into(), "R9700".into())]).unwrap();
            for (i, d) in decode.iter().enumerate() {
                db::add_sample(
                    &c,
                    run,
                    &Sample {
                        prompt: Some(prompt),
                        rep: i as i64 + 1,
                        prompt_tokens: Some(43730),
                        prefill_tps: Some(1160.0),
                        decode_tps: Some(*d),
                        draft_gen: Some(100),
                        draft_acc: Some(80),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("benchdb-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn opts() -> Opts {
        Opts {
            format: Format::Table,
            full: false,
            show: Vec::new(),
        }
    }

    #[test]
    fn filters_and_summary() {
        let d = tmp("q");
        let p = d.join("bench.db");
        seeded(&p);
        let c = db::open_ro(&p).unwrap();
        let f = Filters {
            params: vec!["ctx-size=163840".into()],
            ..Default::default()
        };
        let t = run(
            &c,
            &Cmd::Summary,
            &f,
            &Opts {
                show: vec!["ctx-size".into()],
                ..opts()
            },
        )
        .unwrap();
        assert_eq!(t.rows.len(), 1);
        let col = |name: &str| t.head.iter().position(|h| h == name).unwrap();
        assert_eq!(t.rows[0][col("ctx-size")], "163840");
        assert_eq!(t.rows[0][col("decode t/s")], "35.2 +- 0.3");
        assert_eq!(t.rows[0][col("draft acc")], "80%");
        assert_eq!(t.rows[0][col("prompt / test")], "long");

        let gpu = Filters {
            gpu: Some("r9700".into()),
            ..Default::default()
        };
        assert_eq!(run(&c, &Cmd::Runs, &gpu, &opts()).unwrap().rows.len(), 2);
        let none = Filters {
            gpu: Some("4070".into()),
            ..Default::default()
        };
        assert!(run(&c, &Cmd::Runs, &none, &opts()).unwrap().rows.is_empty());

        let diff = run(&c, &Cmd::Diff(1, 2), &Filters::default(), &opts()).unwrap();
        assert_eq!(
            diff.rows,
            vec![vec!["preset", "ctx-size", "163840", "262144"]]
        );
        assert!(where_clause(&Filters {
            params: vec!["nokey".into()],
            ..Default::default()
        })
        .is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_query_connection_cannot_write() {
        let d = tmp("ro");
        let p = d.join("bench.db");
        seeded(&p);
        let c = db::open_ro(&p).unwrap();
        let r = run(
            &c,
            &Cmd::Sql("DELETE FROM run".into()),
            &Filters::default(),
            &opts(),
        );
        assert!(
            r.is_err()
                || c.query_row("SELECT count(*) FROM run", [], |x| x.get::<_, i64>(0))
                    .unwrap()
                    == 2
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn publish_copies_a_campaign_once_with_its_logs() {
        let d = tmp("pub");
        let (from, to) = (d.join("work.db"), d.join("repo.db"));
        seeded(&from);
        let done = publish(&from, &to, &["camp".into()]).unwrap();
        assert_eq!(done, vec![("camp".to_string(), 2)]);
        let c = db::open_ro(&to).unwrap();
        let n = |sql: &str| c.query_row(sql, [], |x| x.get::<_, i64>(0)).unwrap();
        assert_eq!(n("SELECT count(*) FROM result"), 4);
        assert_eq!(n("SELECT count(*) FROM run_param"), 4);
        assert_eq!(n("SELECT count(*) FROM prompt"), 1);
        assert_eq!(n("SELECT count(*) FROM gpu"), 1);
        assert_eq!(n("SELECT count(*) FROM run WHERE log IS NOT NULL"), 2);
        assert_eq!(n("SELECT count(*) FROM run WHERE points IS NOT NULL"), 2);
        // What the Benchmark tab reads back: the campaign and its points.
        let campaigns = db::campaigns(&c).unwrap();
        assert_eq!((campaigns[0].name.as_str(), campaigns[0].runs), ("camp", 2));
        let (runs, report) = db::campaign_runs(&c, "camp").unwrap();
        assert_eq!(report, None);
        let points = crate::bench::record::points_from_json(&runs[1].0, &runs[1].2);
        assert_eq!((points[0].preset.as_str(), points[0].mean), ("B", 35.2));
        drop(c);
        assert!(publish(&from, &to, &["camp".into()])
            .unwrap_err()
            .contains("already"));
        assert!(publish(&from, &to, &["nope".into()]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn tables_render_in_three_formats() {
        let t = Table {
            head: vec!["a".into(), "n".into()],
            rows: vec![vec!["x, y".into(), "1.5".into()]],
        };
        assert_eq!(
            render(&t, &opts()),
            "a       n\n----  ---\nx, y  1.5\n(1 rows)"
        );
        assert_eq!(
            render(
                &t,
                &Opts {
                    format: Format::Csv,
                    ..opts()
                }
            ),
            "a,n\n\"x, y\",1.5"
        );
        assert!(render(
            &t,
            &Opts {
                format: Format::Json,
                ..opts()
            }
        )
        .contains("\"a\": \"x, y\""));
    }
}
