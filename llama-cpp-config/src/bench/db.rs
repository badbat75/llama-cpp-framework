//! bench.db: where every benchmark lands, with what produced it.
//!
//! One schema for two databases. The WORKING one (`work_path`, beside the
//! reports under the data root) receives every run the tab or a sweep makes;
//! the repo's `benchmarks/bench.db` holds the campaigns the docs cite and
//! receives them only through `bench publish`. The schema is the repo's
//! `benchmarks/schema.sql`, compiled in, so the two can never drift apart.
//!
//! ## The unit is the run, inside a campaign
//!
//! A campaign is the question a set of runs answers (a tab run with several
//! presets, a sweep with its legs); a run is one model load with one
//! configuration; its results are the requests (live) or the llama-bench
//! repetitions (synthetic) sent to it. Every parameter of the run is stored,
//! not only the varied ones: the preset's whole section, server.ini's
//! `[Server]`, the environment llama-server gets, the plan.
//!
//! ## Only finished runs
//!
//! A run is written in ONE transaction, when its preset has been measured to
//! the end (`record::Recorder::commit`). A run that failed or was cancelled is
//! dropped, and a process that dies mid-run leaves nothing: the database holds
//! measurements, not attempts. (`status` is therefore always `ok` here.)
//!
//! ## No migrations
//!
//! `schema.sql` is created with `IF NOT EXISTS` and nothing else: no column is
//! added to an existing file. A schema change before a release rebuilds the
//! repo's database by hand; after one it needs a migration written for it.
//!
//! ## Nothing secret, ever
//!
//! The repo's database is public, and a working database is one `publish`
//! away from it, so `is_secret` keys (API keys, tokens, service URLs) are
//! dropped on the way IN, in every scope, not filtered on the way out.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OptionalExtension};

pub const SCHEMA: &str = include_str!("../../../benchmarks/schema.sql");

/// The database runs are written to and read from: settings.ini `BenchDb`,
/// or `default_path` when that is unset.
pub fn work_path() -> PathBuf {
    crate::settings::load().bench_db_path()
}

/// The working database under the data root.
pub fn default_path() -> PathBuf {
    super::bench_dir().join("bench.db")
}

fn err(path: &Path, e: impl std::fmt::Display) -> String {
    format!("{}: {e}", path.display())
}

/// Open, creating it when missing.
pub fn open(path: &Path) -> Result<Connection, String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| err(dir, e))?;
    }
    let conn = Connection::open(path).map_err(|e| err(path, e))?;
    init(&conn).map_err(|e| err(path, e))?;
    Ok(conn)
}

/// Read-only, for queries: a missing file is an error, not a new database.
pub fn open_ro(path: &Path) -> Result<Connection, String> {
    if !path.is_file() {
        return Err(format!("{}: no such database", path.display()));
    }
    Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| err(path, e))
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    // A second writer (the tab while a sweep runs) waits instead of failing.
    conn.busy_timeout(std::time::Duration::from_secs(10))?;
    conn.execute_batch(SCHEMA)
}

/// Keys never stored: credentials and the endpoints of other services.
pub fn is_secret(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    let compact: String = k.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    compact.contains("apikey")
        || k.contains("secret")
        || k.contains("password")
        || k.contains("auth")
        || k.ends_with("token")
        || k.ends_with("url")
}

// ── Machine, software, model, prompt ─────────────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gpu {
    pub name: String,
    pub vram_mib: i64,
    pub backend: String,
    pub driver: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Machine {
    pub cpu: String,
    pub cores: i64,
    pub threads: i64,
    pub ram_mib: i64,
    pub ram_type: Option<String>,
    pub ram_speed_mts: Option<i64>,
    pub ram_modules: Option<String>,
    pub os: String,
    pub kernel: String,
    pub gpus: Vec<Gpu>,
}

impl Machine {
    /// The text the fingerprint hashes: one `key=value` line per column, then
    /// one `gpu=` line per card sorted by backend and name. A change here
    /// gives every machine a new row: the old ones would no longer be found.
    pub fn canonical(&self) -> String {
        let o = |v: &Option<String>| v.clone().unwrap_or_default();
        let mut s = format!(
            "cpu={}\ncores={}\nthreads={}\nram_mib={}\nram_type={}\nram_speed_mts={}\nram_modules={}\nos={}\nkernel={}\n",
            self.cpu,
            self.cores,
            self.threads,
            self.ram_mib,
            o(&self.ram_type),
            self.ram_speed_mts.map(|v| v.to_string()).unwrap_or_default(),
            o(&self.ram_modules),
            self.os,
            self.kernel
        );
        let mut gpus: Vec<&Gpu> = self.gpus.iter().collect();
        gpus.sort_by(|a, b| (&a.backend, &a.name).cmp(&(&b.backend, &b.name)));
        for g in gpus {
            s.push_str(&format!(
                "gpu={}|{}|{}|{}\n",
                g.backend,
                g.name,
                g.vram_mib,
                o(&g.driver)
            ));
        }
        s
    }

    pub fn fingerprint(&self) -> String {
        crate::sha256::hex(self.canonical().as_bytes())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Software {
    pub llama_cpp_tag: String,
    pub llama_cpp_build: Option<i64>,
    pub llama_cpp_commit: Option<String>,
    pub framework_version: Option<String>,
    pub rocm: Option<String>,
    pub cuda: Option<String>,
    pub vulkan_sdk: Option<String>,
    pub patches: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub file: String,
    pub size_bytes: i64,
    pub name: String,
    pub arch: Option<String>,
    pub quant: Option<String>,
    pub params_b: Option<f64>,
    pub layers: Option<i64>,
    pub experts: Option<String>,
    pub mtp: bool,
}

pub fn machine_id(conn: &Connection, m: &Machine) -> rusqlite::Result<i64> {
    let fp = m.fingerprint();
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM machine WHERE fingerprint = ?1",
            [&fp],
            |r| r.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO machine (fingerprint, cpu, cores, threads, ram_mib, ram_type, ram_speed_mts, \
         ram_modules, os, kernel) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            fp,
            m.cpu,
            m.cores,
            m.threads,
            m.ram_mib,
            m.ram_type,
            m.ram_speed_mts,
            m.ram_modules,
            m.os,
            m.kernel
        ],
    )?;
    let id = conn.last_insert_rowid();
    for g in &m.gpus {
        conn.execute(
            "INSERT OR IGNORE INTO gpu (machine_id, name, vram_mib, backend, driver) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, g.name, g.vram_mib, g.backend, g.driver],
        )?;
    }
    Ok(id)
}

pub fn software_id(conn: &Connection, s: &Software) -> rusqlite::Result<i64> {
    // `IS`, not `=`: NULL columns are part of the identity.
    let find = |conn: &Connection| {
        conn.query_row(
            "SELECT id FROM software WHERE llama_cpp_tag IS ?1 AND llama_cpp_build IS ?2 \
             AND llama_cpp_commit IS ?3 AND framework_version IS ?4 AND rocm IS ?5 \
             AND cuda IS ?6 AND vulkan_sdk IS ?7 AND patches IS ?8",
            params![
                s.llama_cpp_tag,
                s.llama_cpp_build,
                s.llama_cpp_commit,
                s.framework_version,
                s.rocm,
                s.cuda,
                s.vulkan_sdk,
                s.patches
            ],
            |r| r.get(0),
        )
        .optional()
    };
    if let Some(id) = find(conn)? {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO software (llama_cpp_tag, llama_cpp_build, llama_cpp_commit, \
         framework_version, rocm, cuda, vulkan_sdk, patches) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            s.llama_cpp_tag,
            s.llama_cpp_build,
            s.llama_cpp_commit,
            s.framework_version,
            s.rocm,
            s.cuda,
            s.vulkan_sdk,
            s.patches
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// A model is its file name and size; a row recorded earlier (by hand, with a
/// friendlier name) is kept as it is.
pub fn model_id(conn: &Connection, m: &Model) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO model (file, size_bytes, name, arch, quant, params_b, layers, \
         experts, mtp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            m.file,
            m.size_bytes,
            m.name,
            m.arch,
            m.quant,
            m.params_b,
            m.layers,
            m.experts,
            m.mtp as i64
        ],
    )?;
    conn.query_row(
        "SELECT id FROM model WHERE file = ?1 AND size_bytes = ?2",
        params![m.file, m.size_bytes],
        |r| r.get(0),
    )
}

/// A prompt is its text: the same bytes under another name are the same row.
pub fn prompt_id(conn: &Connection, name: &str, text: &str) -> rusqlite::Result<i64> {
    let sha = crate::sha256::hex(text.as_bytes());
    conn.execute(
        "INSERT OR IGNORE INTO prompt (sha256, name, text) VALUES (?1, ?2, ?3)",
        params![sha, name, text],
    )?;
    conn.query_row("SELECT id FROM prompt WHERE sha256 = ?1", [&sha], |r| {
        r.get(0)
    })
}

/// The token count the server reported, the first time one is known.
pub fn prompt_tokens(conn: &Connection, prompt: i64, tokens: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE prompt SET tokens = ?2 WHERE id = ?1 AND tokens IS NULL",
        params![prompt, tokens],
    )?;
    Ok(())
}

// ── Campaigns and runs ───────────────────────────────────────────────────

/// The campaign by name, created when new. An existing one keeps its question
/// unless it had none: runs are ADDED to a campaign, never rewrite it.
pub fn campaign_id(
    conn: &Connection,
    name: &str,
    question: &str,
    started: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO campaign (name, question, started) VALUES (?1, ?2, ?3)",
        params![name, question, started],
    )?;
    conn.execute(
        "UPDATE campaign SET question = ?2 WHERE name = ?1 AND question = '' AND ?2 <> ''",
        params![name, question],
    )?;
    conn.query_row("SELECT id FROM campaign WHERE name = ?1", [name], |r| {
        r.get(0)
    })
}

pub fn set_answer(conn: &Connection, campaign: &str, answer: &str) -> Result<(), String> {
    let n = conn
        .execute(
            "UPDATE campaign SET answer = ?2 WHERE name = ?1",
            params![campaign, answer],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("no campaign `{campaign}`"));
    }
    Ok(())
}

/// The tool's notes on a campaign. `false` when there is no such campaign,
/// which is what a sweep whose every leg failed leaves.
pub fn set_notes(conn: &Connection, campaign: &str, notes: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "UPDATE campaign SET notes = ?2 WHERE name = ?1",
        params![campaign, public_text(notes)],
    )? > 0)
}

/// The configurator's report for a campaign (markdown). `false` when the
/// campaign does not exist, i.e. no run of it finished.
pub fn set_report(conn: &Connection, campaign: &str, report: &str) -> rusqlite::Result<bool> {
    Ok(conn.execute(
        "UPDATE campaign SET report = ?2 WHERE name = ?1",
        params![campaign, public_text(report)],
    )? > 0)
}

/// A campaign as the Benchmark tab lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct CampaignRow {
    pub name: String,
    pub started: String,
    /// `live`, `llama-bench`, or both joined by `+`.
    pub tools: String,
    pub runs: i64,
}

/// Every campaign with a run, newest first (by its first run's start).
pub fn campaigns(conn: &Connection) -> rusqlite::Result<Vec<CampaignRow>> {
    let mut st = conn.prepare(
        "SELECT c.name, min(r.started_utc), group_concat(DISTINCT r.tool), count(r.id) \
         FROM campaign c JOIN run r ON r.campaign_id = c.id \
         GROUP BY c.id ORDER BY min(r.started_utc) DESC, c.id DESC",
    )?;
    let rows = st
        .query_map([], |r| {
            Ok(CampaignRow {
                name: r.get(0)?,
                started: r.get(1)?,
                tools: r
                    .get::<_, Option<String>>(2)?
                    .unwrap_or_default()
                    .replace(',', "+"),
                runs: r.get(3)?,
            })
        })?
        .collect();
    rows
}

/// A campaign's runs in order, as (label, tool, points JSON), and its report.
#[allow(clippy::type_complexity)]
pub fn campaign_runs(
    conn: &Connection,
    campaign: &str,
) -> rusqlite::Result<(Vec<(String, String, String)>, Option<String>)> {
    let mut st = conn.prepare(
        "SELECT r.label, r.tool, coalesce(r.points, '[]') FROM run r \
         JOIN campaign c ON c.id = r.campaign_id WHERE c.name = ?1 ORDER BY r.id",
    )?;
    let runs = st
        .query_map([campaign], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let report = conn
        .query_row(
            "SELECT report FROM campaign WHERE name = ?1",
            [campaign],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok((runs, report))
}

pub struct NewRun<'a> {
    pub campaign: i64,
    pub label: &'a str,
    pub started_utc: &'a str,
    pub ended_utc: &'a str,
    pub machine: i64,
    pub software: i64,
    pub model: i64,
    /// `live` or `llama-bench`.
    pub tool: &'a str,
    pub notes: &'a str,
    pub log: &'a str,
    /// The tab's means for this run, as JSON (`record::points_json`).
    pub points: &'a str,
}

/// Insert a finished run. A label the campaign
/// already has gets ` (2)`, ` (3)`...: running the same preset twice in one
/// campaign is a repetition, not a conflict.
pub fn insert_run(conn: &Connection, r: &NewRun) -> rusqlite::Result<i64> {
    let mut label = r.label.to_string();
    let mut n = 1;
    while conn
        .prepare("SELECT 1 FROM run WHERE campaign_id = ?1 AND label = ?2")?
        .exists(params![r.campaign, label])?
    {
        n += 1;
        label = format!("{} ({n})", r.label);
    }
    conn.execute(
        "INSERT INTO run (campaign_id, label, started_utc, machine_id, software_id, model_id, \
         tool, status, notes, log, ended_utc, points) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ok', nullif(?8, ''), nullif(?9, ''), nullif(?10, ''), \
         nullif(?11, ''))",
        params![
            r.campaign,
            label,
            r.started_utc,
            r.machine,
            r.software,
            r.model,
            r.tool,
            public_text(r.notes),
            public_text(r.log),
            r.ended_utc,
            r.points
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// A value as stored: an absolute path is cut to its file name. The database
/// travels (`publish`), the layout of this machine's disks does not; and what
/// identifies a model, a drafter or a prompt is the file, not where it sits.
pub fn public_value(v: &str) -> String {
    let v = v.trim();
    let p = Path::new(v);
    // Either OS's form on either OS: `Path` on Windows calls `/mnt/x` relative.
    let windows_abs =
        v.len() > 2 && v.as_bytes()[1] == b':' && matches!(v.as_bytes()[2], b'\\' | b'/');
    if p.is_absolute() || windows_abs || v.starts_with('/') {
        if let Some(name) = v.rsplit(['/', '\\']).next().filter(|n| !n.is_empty()) {
            return name.to_string();
        }
    }
    v.to_string()
}

/// Free text as stored (a run's log and notes, a campaign's notes and
/// report): every absolute path cut to its file name, as `public_value` does
/// for a single value, and the value after a secret-like flag or header
/// replaced. The engine's output names the model's directory, the binaries'
/// and the user's home; none of that is a measurement, and the database may
/// be the repo's (settings.ini `BenchDb`).
pub fn public_text(text: &str) -> String {
    use std::sync::LazyLock;
    // A path starts at the beginning of a line or after a separator, so the
    // `/v1/...` of a URL stays as it is; its directory part is everything up
    // to the last separator. A directory with a space in it is cut at the
    // space: the home directory, the one most likely to have one, is
    // replaced as a whole first.
    static PATH: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(^|[\s"'=(\[,<>|])((?:[A-Za-z]:[\\/]|/)(?:[^\s\\/"'`;,()\[\]<>|]+[\\/])+)"#,
        )
        .unwrap()
    });
    static SECRET: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)(--api-key\S*|api[-_]?key|authorization|password|secret)([\s=:]+)(bearer\s+)?[^\s,;]+",
        )
        .unwrap()
    });
    let mut out = text.to_string();
    for home in ["HOME", "USERPROFILE"] {
        if let Ok(h) = std::env::var(home) {
            let h = h.trim_end_matches(['/', '\\']);
            if h.len() > 1 {
                out = out.replace(h, "~");
            }
        }
    }
    let out = PATH.replace_all(&out, "$1");
    SECRET.replace_all(&out, "$1$2$3<redacted>").into_owned()
}

/// Store a scope's parameters, secrets dropped, empty values skipped (an empty
/// INI value is the key left at its default), paths cut to file names.
pub fn add_params(
    conn: &Connection,
    run: i64,
    scope: &str,
    kv: &[(String, String)],
) -> rusqlite::Result<()> {
    for (k, v) in kv {
        if is_secret(k) || v.trim().is_empty() {
            continue;
        }
        conn.execute(
            "INSERT OR REPLACE INTO run_param (run_id, scope, key, value) VALUES (?1, ?2, ?3, ?4)",
            params![run, scope, k, public_value(v)],
        )?;
    }
    Ok(())
}

pub fn add_gpus(conn: &Connection, run: i64, devices: &[(String, String)]) -> rusqlite::Result<()> {
    for (device, gpu) in devices {
        conn.execute(
            "INSERT OR IGNORE INTO run_gpu (run_id, device, gpu) VALUES (?1, ?2, ?3)",
            params![run, device, if gpu.is_empty() { device } else { gpu }],
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub prompt: Option<i64>,
    pub rep: i64,
    /// llama-bench's test label; `None` for a live request.
    pub test: Option<String>,
    pub temperature: Option<f64>,
    pub cache_prompt: Option<bool>,
    pub prompt_tokens: Option<i64>,
    pub prefill_tps: Option<f64>,
    pub gen_tokens: Option<i64>,
    pub decode_tps: Option<f64>,
    pub draft_gen: Option<i64>,
    pub draft_acc: Option<i64>,
    pub cache_n: Option<i64>,
    pub reasoning_tps: Option<f64>,
    pub answer_tps: Option<f64>,
    /// JSON, see schema.sql.
    pub detail: Option<String>,
}

pub fn add_sample(conn: &Connection, run: i64, s: &Sample) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO result (run_id, prompt_id, rep, test, temperature, cache_prompt, \
         prompt_tokens, prefill_tps, gen_tokens, decode_tps, draft_gen, draft_acc, cache_n, \
         reasoning_tps, answer_tps, detail) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            run,
            s.prompt,
            s.rep,
            s.test,
            s.temperature,
            s.cache_prompt.map(|b| b as i64),
            s.prompt_tokens,
            s.prefill_tps,
            s.gen_tokens,
            s.decode_tps,
            s.draft_gen,
            s.draft_acc,
            s.cache_n,
            s.reasoning_tps,
            s.answer_tps,
            s.detail
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        c
    }

    fn machine() -> Machine {
        Machine {
            cpu: "AMD Ryzen 9 9900X 12-Core Processor".into(),
            cores: 12,
            threads: 24,
            ram_mib: 60_000,
            ram_type: Some("DDR5".into()),
            ram_speed_mts: Some(6000),
            ram_modules: Some("2 x 32 GB CMK64GX5M2B6000Z30".into()),
            os: "Ubuntu 26.04.1 LTS".into(),
            kernel: "7.0.0-38-generic".into(),
            gpus: vec![
                Gpu {
                    name: "NVIDIA GeForce RTX 4070 SUPER".into(),
                    vram_mib: 12282,
                    backend: "CUDA".into(),
                    driver: Some("595.99.02".into()),
                },
                Gpu {
                    name: "AMD Radeon AI PRO R9700".into(),
                    vram_mib: 32624,
                    backend: "ROCm".into(),
                    driver: None,
                },
            ],
        }
    }

    #[test]
    fn the_schema_initialises_twice() {
        let c = mem();
        init(&c).unwrap();
    }

    /// The table columns of every table, in order.
    fn columns(c: &Connection) -> Vec<(String, Vec<String>)> {
        let tables: Vec<String> = c
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        tables
            .into_iter()
            .map(|t| {
                let cols = c
                    .prepare(&format!("SELECT name FROM pragma_table_info('{t}')"))
                    .unwrap()
                    .query_map([], |r| r.get(0))
                    .unwrap()
                    .collect::<rusqlite::Result<_>>()
                    .unwrap();
                (t, cols)
            })
            .collect()
    }

    /// Nothing migrates (module header), so the committed database must have
    /// exactly the schema a fresh one gets: a `schema.sql` change without a
    /// rebuild of `benchmarks/bench.db` fails here instead of in the tab.
    #[test]
    fn the_repo_database_matches_the_schema() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/bench.db");
        assert_eq!(columns(&open_ro(&repo).unwrap()), columns(&mem()));
    }

    /// The fingerprint ignores the order the GPUs were detected in, and the
    /// text is pinned: a change makes the same box a new machine row.
    #[test]
    fn the_canonical_text_is_stable() {
        let mut m = machine();
        let a = m.fingerprint();
        m.gpus.reverse();
        assert_eq!(m.fingerprint(), a);
        assert_eq!(
            m.canonical(),
            "cpu=AMD Ryzen 9 9900X 12-Core Processor\ncores=12\nthreads=24\nram_mib=60000\n\
             ram_type=DDR5\nram_speed_mts=6000\nram_modules=2 x 32 GB CMK64GX5M2B6000Z30\n\
             os=Ubuntu 26.04.1 LTS\nkernel=7.0.0-38-generic\n\
             gpu=CUDA|NVIDIA GeForce RTX 4070 SUPER|12282|595.99.02\n\
             gpu=ROCm|AMD Radeon AI PRO R9700|32624|\n"
        );
    }

    #[test]
    fn rows_are_found_again_not_duplicated() {
        let c = mem();
        let m = machine();
        assert_eq!(machine_id(&c, &m).unwrap(), machine_id(&c, &m).unwrap());
        let s = Software {
            llama_cpp_tag: "v0.7.0".into(),
            llama_cpp_build: Some(11669),
            ..Default::default()
        };
        assert_eq!(software_id(&c, &s).unwrap(), software_id(&c, &s).unwrap());
        let other = Software {
            rocm: Some("10.1.0".into()),
            ..s.clone()
        };
        assert_ne!(
            software_id(&c, &s).unwrap(),
            software_id(&c, &other).unwrap()
        );
        let p = prompt_id(&c, "code", "hello").unwrap();
        assert_eq!(prompt_id(&c, "renamed", "hello").unwrap(), p);
    }

    fn a_run(c: &Connection, label: &str) -> i64 {
        let cid = campaign_id(c, "camp", "why?", "2026-10-16").unwrap();
        let mid = machine_id(c, &machine()).unwrap();
        let sid = software_id(c, &Software::default()).unwrap();
        let model = model_id(
            c,
            &Model {
                file: "m.gguf".into(),
                size_bytes: 1,
                name: "m".into(),
                ..Default::default()
            },
        )
        .unwrap();
        insert_run(
            c,
            &NewRun {
                campaign: cid,
                label,
                started_utc: "2026-10-16 18:00:00",
                ended_utc: "",
                points: "",
                machine: mid,
                software: sid,
                model,
                tool: "live",
                notes: "",
                log: "line\n",
            },
        )
        .unwrap()
    }

    /// A repeated label in the same campaign is numbered, not refused.
    #[test]
    fn a_repeated_label_is_numbered() {
        let c = mem();
        let r1 = a_run(&c, "P");
        let r2 = a_run(&c, "P");
        let get = |id: i64| -> (String, String, Option<String>) {
            c.query_row(
                "SELECT label, status, notes FROM run WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
        };
        assert_eq!(get(r1), ("P".into(), "ok".into(), None));
        assert_eq!(get(r2).0, "P (2)");
    }

    #[test]
    fn secrets_never_reach_the_params() {
        let c = mem();
        let r = a_run(&c, "P");
        let kv: Vec<(String, String)> = [
            ("OpencodeApiKey", "sk-1"),
            ("OpencodeBaseUrl", "https://x"),
            ("api-key", "k"),
            ("hf-token", "t"),
            ("Port", "8580"),
            ("ctx-size", "172032"),
            ("blank", " "),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        add_params(&c, r, "server", &kv).unwrap();
        let keys: Vec<String> = c
            .prepare("SELECT key FROM run_param WHERE run_id = ?1 ORDER BY key")
            .unwrap()
            .query_map([r], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(keys, ["Port", "ctx-size"]);
    }

    #[test]
    fn paths_are_cut_to_file_names() {
        assert_eq!(public_value("/mnt/models/m-Q8_0.gguf"), "m-Q8_0.gguf");
        assert_eq!(public_value(r"E:\models\m.gguf"), "m.gguf");
        assert_eq!(
            public_value(r"blk\.15\.ffn_(up|down)=ROCm0"),
            r"blk\.15\.ffn_(up|down)=ROCm0"
        );
        assert_eq!(public_value(" 172032 "), "172032");
        assert_eq!(
            public_text(
                "load_tensors: /mnt/models/qwen/m-Q8_0.gguf\n\
                 E:\\llama.cpp\\bin\\llama-bench.exe -m \"D:\\models\\x.gguf\" --api-key s3cr3t\n\
                 POST http://127.0.0.1:8080/v1/chat/completions n_tokens = 512\n\
                 Authorization: Bearer abc123"
            ),
            "load_tensors: m-Q8_0.gguf\n\
             llama-bench.exe -m \"x.gguf\" --api-key <redacted>\n\
             POST http://127.0.0.1:8080/v1/chat/completions n_tokens = 512\n\
             Authorization: Bearer <redacted>"
        );
    }
}
