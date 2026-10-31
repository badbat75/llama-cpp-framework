//! bench.db, read only: campaigns, their runs, and the per-run samples a chart
//! or a campaign page shows. The schema is benchmarks\schema.sql.

use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::path::Path;

pub struct Campaign {
    pub id: i64,
    pub name: String,
    pub question: String,
    pub started: String,
    pub answer: Option<String>,
}

pub struct Run {
    pub id: i64,
    pub campaign_id: i64,
    pub label: String,
    pub status: String,
    pub model: String,
    pub quant: String,
    pub llama_cpp: String,
    pub gpus: Vec<(String, String)>,
    /// (scope, key, value)
    pub params: Vec<(String, String, String)>,
    /// (device, free MiB)
    pub free: Vec<(String, i64)>,
    pub reqs: Vec<Req>,
}

pub struct Req {
    pub rep: i64,
    pub prompt_tokens: i64,
    pub prefill: Option<f64>,
    pub gen: i64,
    pub decode: Option<f64>,
    pub draft_gen: i64,
    pub draft_acc: i64,
    /// A request that prefilled its whole prompt (no cache hit).
    pub cold: bool,
    /// Seconds from the common start, for requests that ran together.
    pub sent: Option<f64>,
}

pub struct Db {
    pub campaigns: Vec<Campaign>,
    pub runs: Vec<Run>,
    pub models: Vec<String>,
    pub gpus: Vec<String>,
}

/// Scopes in the order a key is looked up: the benchmark's own parameters,
/// then the preset, the server, the environment.
const SCOPES: [&str; 4] = ["bench", "preset", "server", "env"];

/// What a run without a key reads as: the llama.cpp default.
pub const UNSET: &str = "unset";

impl Db {
    pub fn open(path: &Path) -> Result<Db, String> {
        let e = |e: rusqlite::Error| format!("{}: {e}", path.display());
        let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(e)?;
        let campaigns = c
            .prepare("SELECT id, name, question, started, answer FROM campaign ORDER BY started DESC, id DESC")
            .map_err(e)?
            .query_map([], |r| {
                Ok(Campaign {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    question: r.get(2)?,
                    started: r.get(3)?,
                    answer: r.get(4)?,
                })
            })
            .map_err(e)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(e)?;
        let mut runs = c
            .prepare(
                "SELECT r.id, r.campaign_id, r.label, r.status, m.name, coalesce(m.quant, ''), s.llama_cpp_tag
                 FROM run r JOIN model m ON m.id = r.model_id JOIN software s ON s.id = r.software_id
                 ORDER BY r.id",
            )
            .map_err(e)?
            .query_map([], |r| {
                Ok(Run {
                    id: r.get(0)?,
                    campaign_id: r.get(1)?,
                    label: r.get(2)?,
                    status: r.get(3)?,
                    model: r.get(4)?,
                    quant: r.get(5)?,
                    llama_cpp: r.get(6)?,
                    gpus: Vec::new(),
                    params: Vec::new(),
                    free: Vec::new(),
                    reqs: Vec::new(),
                })
            })
            .map_err(e)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(e)?;
        let index: BTreeMap<i64, usize> = runs.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
        let mut st = c.prepare("SELECT run_id, scope, key, value FROM run_param ORDER BY run_id, scope, key").map_err(e)?;
        let mut rows = st.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            if let Some(&i) = index.get(&r.get::<_, i64>(0).map_err(e)?) {
                runs[i].params.push((r.get(1).map_err(e)?, r.get(2).map_err(e)?, r.get(3).map_err(e)?));
            }
        }
        let mut st = c.prepare("SELECT run_id, device, gpu FROM run_gpu ORDER BY run_id, device").map_err(e)?;
        let mut rows = st.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            if let Some(&i) = index.get(&r.get::<_, i64>(0).map_err(e)?) {
                runs[i].gpus.push((r.get(1).map_err(e)?, r.get(2).map_err(e)?));
            }
        }
        let mut st = c
            .prepare("SELECT run_id, device, free_mib FROM run_device WHERE free_mib IS NOT NULL ORDER BY run_id, device")
            .map_err(e)?;
        let mut rows = st.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            if let Some(&i) = index.get(&r.get::<_, i64>(0).map_err(e)?) {
                runs[i].free.push((r.get(1).map_err(e)?, r.get(2).map_err(e)?));
            }
        }
        // Request rows only: a llama-bench row (test set) is not a request.
        let mut st = c
            .prepare(
                "SELECT run_id, rep, coalesce(prompt_tokens, 0), prefill_tps, coalesce(gen_tokens, 0), decode_tps,
                        coalesce(draft_gen, 0), coalesce(draft_acc, 0), cache_n, detail
                 FROM result WHERE test IS NULL ORDER BY run_id, id",
            )
            .map_err(e)?;
        let mut rows = st.query([]).map_err(e)?;
        while let Some(r) = rows.next().map_err(e)? {
            let Some(&i) = index.get(&r.get::<_, i64>(0).map_err(e)?) else { continue };
            let detail: Option<String> = r.get(9).map_err(e)?;
            let sent = detail
                .and_then(|d| serde_json::from_str::<serde_json::Value>(&d).ok())
                .and_then(|v| v.get("sent_s").and_then(|s| s.as_f64()));
            let cache_n: Option<i64> = r.get(8).map_err(e)?;
            runs[i].reqs.push(Req {
                rep: r.get(1).map_err(e)?,
                prompt_tokens: r.get(2).map_err(e)?,
                prefill: r.get(3).map_err(e)?,
                gen: r.get(4).map_err(e)?,
                decode: r.get(5).map_err(e)?,
                draft_gen: r.get(6).map_err(e)?,
                draft_acc: r.get(7).map_err(e)?,
                cold: cache_n.unwrap_or(0) == 0,
                sent,
            });
        }
        let models = col(&c, "SELECT DISTINCT name FROM model ORDER BY name").map_err(e)?;
        let gpus = col(&c, "SELECT DISTINCT name FROM gpu ORDER BY name").map_err(e)?;
        Ok(Db { campaigns, runs, models, gpus })
    }

    pub fn campaign(&self, name: &str) -> Option<&Campaign> {
        self.campaigns.iter().find(|c| c.name == name)
    }

    pub fn runs_of(&self, campaign: &Campaign) -> impl Iterator<Item = &Run> {
        let id = campaign.id;
        self.runs.iter().filter(move |r| r.campaign_id == id)
    }
}

fn col(c: &Connection, sql: &str) -> rusqlite::Result<Vec<String>> {
    c.prepare(sql)?.query_map([], |r| r.get(0))?.collect()
}

impl Run {
    /// A parameter's value, looked up scope by scope; `label` is the run's label.
    pub fn param(&self, key: &str) -> Option<&str> {
        if key == "label" {
            return Some(&self.label);
        }
        SCOPES
            .iter()
            .find_map(|s| self.params.iter().find(|p| p.0 == *s && p.1 == key))
            .or_else(|| self.params.iter().find(|p| p.1 == key))
            .map(|p| p.2.as_str())
    }

    pub fn value(&self, key: &str) -> &str {
        self.param(key).unwrap_or(UNSET)
    }

    /// The spec a run measured under: `custom` when it records none.
    pub fn spec(&self) -> String {
        match (self.param("spec"), self.param("spec-version")) {
            (Some(s), Some(v)) if s != "custom" => format!("{s}-{v}"),
            (Some(s), _) => s.to_string(),
            _ => "custom".to_string(),
        }
    }

    /// The samples of one metric (docs\AGENTS.md, "Charts"), one per request
    /// or, for a total, one per repetition.
    pub fn samples(&self, metric: &str) -> Vec<f64> {
        let rs = self.reqs.iter();
        match metric {
            "prefill" => rs.filter(|r| r.cold).filter_map(|r| r.prefill).collect(),
            "decode" => rs.filter_map(|r| r.decode).collect(),
            "decode-warm" => rs.filter(|r| !r.cold).filter_map(|r| r.decode).collect(),
            "decode-cold" => rs.filter(|r| r.cold).filter_map(|r| r.decode).collect(),
            "acceptance" => rs
                .filter(|r| r.draft_gen > 0)
                .map(|r| r.draft_acc as f64 * 100.0 / r.draft_gen as f64)
                .collect(),
            "prefill-total" | "decode-total" => {
                let mut reps: BTreeMap<i64, Vec<&Req>> = BTreeMap::new();
                for r in rs {
                    reps.entry(r.rep).or_default().push(r);
                }
                reps.values()
                    .filter_map(|g| {
                        let (p, d) = totals(g)?;
                        if metric == "prefill-total" { p } else { d }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// The configurator's `bench::concurrent_totals`, recomputed: prefill total =
/// the cold requests' prompt tokens over the window from the first send to the
/// last cold prefill's end; decode total = every generated token over the
/// window from the first decode start to the last decode end. One request
/// alone: its own rates.
fn totals(g: &[&Req]) -> Option<(Option<f64>, Option<f64>)> {
    if let [r] = g {
        return Some((r.prefill.filter(|_| r.cold), r.decode));
    }
    let secs = |n: i64, t: Option<f64>| match t {
        Some(t) if t > 0.0 => n as f64 / t,
        _ => 0.0,
    };
    let sent: Vec<f64> = g.iter().map(|r| r.sent).collect::<Option<_>>()?;
    let t0 = sent.iter().cloned().fold(f64::INFINITY, f64::min);
    let pre_end = |(r, s): (&&Req, &f64)| s + secs(r.prompt_tokens, r.prefill);
    let cold: Vec<_> = g.iter().zip(&sent).filter(|(r, _)| r.cold).collect();
    let prefill = if cold.is_empty() {
        None
    } else {
        let tokens: i64 = cold.iter().map(|(r, _)| r.prompt_tokens).sum();
        let end = cold.iter().map(|&(r, s)| pre_end((r, s))).fold(f64::NEG_INFINITY, f64::max);
        (end > t0).then(|| tokens as f64 / (end - t0))
    };
    let start = g.iter().zip(&sent).map(pre_end).fold(f64::INFINITY, f64::min);
    let end = g
        .iter()
        .zip(&sent)
        .map(|(r, s)| pre_end((r, s)) + secs(r.gen, r.decode))
        .fold(f64::NEG_INFINITY, f64::max);
    let generated: i64 = g.iter().map(|r| r.gen).sum();
    let decode = (generated > 0 && end > start).then(|| generated as f64 / (end - start));
    Some((prefill, decode))
}

/// Mean, sample standard deviation (None for one sample), count.
pub fn stat(v: &[f64]) -> Option<(f64, Option<f64>, usize)> {
    if v.is_empty() {
        return None;
    }
    let n = v.len() as f64;
    let m = v.iter().sum::<f64>() / n;
    let sd = (v.len() > 1).then(|| (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (n - 1.0)).sqrt());
    Some((m, sd, v.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(rep: i64, cold: bool, sent: f64, pt: i64, pp: f64, gen: i64, dec: f64) -> Req {
        Req { rep, prompt_tokens: pt, prefill: Some(pp), gen, decode: Some(dec), draft_gen: 0, draft_acc: 0, cold, sent: Some(sent) }
    }

    #[test]
    fn two_decodes_share_one_window() {
        // Both warm, sent together: 4 tokens each prefilled instantly, 100 and 50
        // tokens at 10 t/s: the window is 10 s, 150 tokens.
        let a = req(1, false, 0.0, 0, 1.0, 100, 10.0);
        let b = req(1, false, 0.0, 0, 1.0, 50, 10.0);
        let (p, d) = totals(&[&a, &b]).unwrap();
        assert_eq!(p, None);
        assert!((d.unwrap() - 15.0).abs() < 1e-9);
    }

    #[test]
    fn one_request_is_its_own_total() {
        let a = req(1, true, 0.0, 1000, 500.0, 10, 20.0);
        assert_eq!(totals(&[&a]), Some((Some(500.0), Some(20.0))));
    }
}
