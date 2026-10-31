//! `bench-chart` blocks (docs\AGENTS.md, "Charts"): parsed, checked against
//! bench.db, drawn as an inline SVG bar chart (no script: each bar's value,
//! sd and sample count are in its `<title>`, shown on hover).

use crate::data::{stat, Db, Run};
use crate::esc;

pub const METRICS: [&str; 8] = [
    "prefill",
    "decode",
    "prefill-total",
    "decode-total",
    "decode-warm",
    "decode-cold",
    "acceptance",
    "free-mib",
];

pub struct Spec {
    pub title: Option<String>,
    pub campaigns: Vec<String>,
    pub metric: String,
    pub x: String,
    pub series: String,
    pub filter: Vec<(String, String)>,
    pub unit: Option<String>,
    /// Display names of values (`unset=layer`): labels only, filters match the raw value.
    pub rename: Vec<(String, String)>,
}

impl Spec {
    pub fn parse(src: &str) -> Result<Spec, String> {
        let mut kv: Vec<(String, String)> = Vec::new();
        for line in src.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line.split_once('=').ok_or_else(|| format!("not `key = value`: {line}"))?;
            kv.push((k.trim().to_string(), v.trim().to_string()));
        }
        let get = |k: &str| kv.iter().find(|p| p.0 == k).map(|p| p.1.clone());
        for (k, _) in &kv {
            if !["title", "campaign", "metric", "x", "series", "where", "unit", "rename"].contains(&k.as_str()) {
                return Err(format!("unknown key `{k}`"));
            }
        }
        let need = |k: &str| get(k).filter(|v| !v.is_empty()).ok_or_else(|| format!("`{k}` is required"));
        let metric = need("metric")?;
        if !METRICS.contains(&metric.as_str()) {
            return Err(format!("unknown metric `{metric}` (one of {})", METRICS.join(", ")));
        }
        let pairs = |key: &str| -> Result<Vec<(String, String)>, String> {
            let mut out = Vec::new();
            for f in get(key).unwrap_or_default().split(',').map(str::trim).filter(|f| !f.is_empty()) {
                let (k, v) = f.split_once('=').ok_or_else(|| format!("`{key}` needs a=b pairs: {f}"))?;
                out.push((k.trim().to_string(), v.trim().to_string()));
            }
            Ok(out)
        };
        let filter = pairs("where")?;
        let rename = pairs("rename")?;
        Ok(Spec {
            title: get("title"),
            campaigns: need("campaign")?.split(',').map(|c| c.trim().to_string()).collect(),
            metric,
            x: need("x")?,
            series: need("series")?,
            filter,
            unit: get("unit"),
            rename,
        })
    }

    fn unit(&self) -> String {
        self.unit.clone().unwrap_or_else(|| {
            match self.metric.as_str() {
                "acceptance" => "%",
                "free-mib" => "MiB",
                _ => "t/s",
            }
            .to_string()
        })
    }
}

/// One bar: mean, sd, samples.
type Cell = (f64, Option<f64>, usize);

pub struct Data {
    pub xs: Vec<String>,
    pub series: Vec<String>,
    /// [series][x]
    pub cells: Vec<Vec<Option<Cell>>>,
}

pub fn build(spec: &Spec, db: &Db) -> Result<Data, String> {
    let mut runs: Vec<&Run> = Vec::new();
    for name in &spec.campaigns {
        let c = db.campaign(name).ok_or_else(|| format!("no campaign `{name}` in bench.db"))?;
        runs.extend(db.runs_of(c).filter(|r| r.status == "ok"));
    }
    let keys = spec.filter.iter().map(|f| &f.0).chain([&spec.x]).chain((spec.metric != "free-mib").then_some(&spec.series));
    for k in keys {
        if !runs.iter().any(|r| r.param(k).is_some()) {
            return Err(format!("no run of {} has the key `{k}`", spec.campaigns.join(", ")));
        }
    }
    runs.retain(|r| spec.filter.iter().all(|(k, v)| r.value(k) == v));
    let mut xs: Vec<String> = Vec::new();
    let mut series: Vec<String> = Vec::new();
    let mut pools: Vec<(usize, usize, Vec<f64>)> = Vec::new();
    let mut add = |x: &str, s: &str, v: Vec<f64>| {
        if v.is_empty() {
            return;
        }
        let xi = idx(&mut xs, x);
        let si = idx(&mut series, s);
        match pools.iter_mut().find(|p| p.0 == si && p.1 == xi) {
            Some(p) => p.2.extend(v),
            None => pools.push((si, xi, v)),
        }
    };
    for r in &runs {
        if spec.metric == "free-mib" {
            for (dev, free) in &r.free {
                add(r.value(&spec.x), dev, vec![*free as f64]);
            }
        } else {
            add(r.value(&spec.x), r.value(&spec.series), r.samples(&spec.metric));
        }
    }
    if pools.is_empty() {
        return Err(format!("no `{}` data in {} after the filter", spec.metric, spec.campaigns.join(", ")));
    }
    let mut cells = vec![vec![None; xs.len()]; series.len()];
    for (si, xi, v) in pools {
        cells[si][xi] = stat(&v);
    }
    let shown = |v: &mut Vec<String>| {
        for s in v.iter_mut() {
            if let Some((_, n)) = spec.rename.iter().find(|(from, _)| from == s) {
                *s = n.clone();
            }
        }
    };
    shown(&mut xs);
    shown(&mut series);
    Ok(Data { xs, series, cells })
}

fn idx(v: &mut Vec<String>, s: &str) -> usize {
    v.iter().position(|x| x == s).unwrap_or_else(|| {
        v.push(s.to_string());
        v.len() - 1
    })
}

/// A step of 1, 2 or 5 times a power of ten giving about five ticks.
fn tick_step(max: f64) -> f64 {
    let raw = max / 5.0;
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * p).find(|s| *s >= raw).unwrap_or(10.0 * p)
}

fn num(v: f64) -> String {
    if v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") }
}

pub fn svg(spec: &Spec, d: &Data, campaign_href: &dyn Fn(&str) -> String) -> String {
    let unit = spec.unit();
    let title = spec.title.clone().unwrap_or_else(|| format!("{} ({unit})", spec.metric));
    let (w, h, l, r, t, b) = (720.0, 340.0, 56.0, 16.0, 40.0, 44.0);
    let pw = w - l - r;
    let ph = h - t - b;
    let top = d
        .cells
        .iter()
        .flatten()
        .flatten()
        .map(|c| c.0 + c.1.unwrap_or(0.0))
        .fold(0.0, f64::max)
        .max(1e-9);
    let step = tick_step(top * 1.05);
    let ymax = (top * 1.05 / step).ceil() * step;
    let y = |v: f64| t + ph - v / ymax * ph;
    let mut o = format!(
        "<figure class=\"chart\"><svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"{}\">",
        esc(&title)
    );
    // grid and y axis
    let mut v = 0.0;
    while v <= ymax + step / 2.0 {
        o += &format!(
            "<line class=\"grid\" x1=\"{l}\" x2=\"{}\" y1=\"{yy:.1}\" y2=\"{yy:.1}\"/><text class=\"tick\" x=\"{}\" y=\"{:.1}\" text-anchor=\"end\">{}</text>",
            l + pw,
            l - 6.0,
            y(v) + 4.0,
            num(v),
            yy = y(v)
        );
        v += step;
    }
    o += &format!("<text class=\"tick\" x=\"{l}\" y=\"{}\">{}</text>", t - 26.0, esc(&unit));
    // legend
    let mut lx = l + 40.0;
    for (si, s) in d.series.iter().enumerate() {
        o += &format!(
            "<rect class=\"s{}\" x=\"{lx:.1}\" y=\"{}\" width=\"12\" height=\"12\" rx=\"2\"/><text class=\"legend\" x=\"{:.1}\" y=\"{}\">{}</text>",
            si % 6,
            t - 36.0,
            lx + 17.0,
            t - 26.0,
            esc(s)
        );
        lx += 30.0 + 7.0 * s.chars().count() as f64;
    }
    // bars
    let gw = pw / d.xs.len() as f64;
    let bw = (gw * 0.8 / d.series.len() as f64).min(64.0);
    for (xi, x) in d.xs.iter().enumerate() {
        let cx = l + gw * (xi as f64 + 0.5);
        let x0 = cx - bw * d.series.len() as f64 / 2.0;
        for (si, s) in d.series.iter().enumerate() {
            let Some((m, sd, n)) = d.cells[si][xi] else { continue };
            let bx = x0 + bw * si as f64;
            let sdt = sd.map(|s| format!(" ±{}", num(s))).unwrap_or_default();
            o += &format!(
                "<rect class=\"s{}\" x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\"><title>{}, {}: {}{sdt} {} (n = {n})</title></rect>",
                si % 6,
                bx + 1.0,
                y(m),
                bw - 2.0,
                y(0.0) - y(m),
                esc(s),
                esc(x),
                num(m),
                esc(&unit)
            );
            if let Some(sd) = sd.filter(|s| *s > 0.0) {
                let ex = bx + bw / 2.0;
                o += &format!(
                    "<line class=\"err\" x1=\"{ex:.1}\" x2=\"{ex:.1}\" y1=\"{:.1}\" y2=\"{:.1}\"/>",
                    y((m - sd).max(0.0)),
                    y(m + sd)
                );
            }
            o += &format!(
                "<text class=\"val\" x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>",
                bx + bw / 2.0,
                y(m + sd.unwrap_or(0.0)) - 4.0,
                num(m)
            );
        }
        o += &format!(
            "<text class=\"tick\" x=\"{cx:.1}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            t + ph + 18.0,
            esc(x)
        );
    }
    o += &format!("<line class=\"axis\" x1=\"{l}\" x2=\"{}\" y1=\"{:.1}\" y2=\"{:.1}\"/>", l + pw, y(0.0), y(0.0));
    o += "</svg><figcaption>";
    o += &esc(&title);
    o += ". Mean of each bar's samples, ± one sample standard deviation; ";
    o += &spec.campaigns.iter().map(|c| format!("campaign <a href=\"{}\"><code>{}</code></a>", campaign_href(c), esc(c))).collect::<Vec<_>>().join(", ");
    if !spec.filter.is_empty() {
        o += &format!(
            ", {}",
            esc(&spec.filter.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(", "))
        );
    }
    o += ".</figcaption></figure>";
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_parses_with_comments() {
        let s = Spec::parse("campaign = a, b\nmetric = decode ; per request\nx = prompt\nseries = split-mode\nwhere = concurrency=single, temp=0\n").unwrap();
        assert_eq!(s.campaigns, ["a", "b"]);
        assert_eq!(s.filter, [("concurrency".into(), "single".into()), ("temp".into(), "0".into())]);
        assert!(Spec::parse("campaign = a\nmetric = speed\nx = a\nseries = b").is_err());
        assert!(Spec::parse("campaign = a\nmetric = decode\nx = a").is_err());
        assert!(Spec::parse("campaign = a\nmetric = decode\nx = a\nseries = b\ncolour = red").is_err());
    }

    #[test]
    fn ticks_are_round() {
        assert_eq!(tick_step(100.0), 20.0);
        assert_eq!(tick_step(1500.0), 500.0);
        assert_eq!(tick_step(7.0), 2.0);
    }
}
