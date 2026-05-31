//! Local run history: every measurement is persisted as a JSON report under
//! `~/.voltiq/runs/` (override with `$VOLTIQ_HOME`), so runs can be listed
//! (`voltiq runs`) and diffed over time (`voltiq compare`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use voltiq_core::process::now_unix_ms;
use voltiq_core::{Finding, Report};

/// Where runs are stored: `$VOLTIQ_HOME/runs`, else `~/.voltiq/runs`.
pub fn runs_dir() -> PathBuf {
    if let Ok(h) = std::env::var("VOLTIQ_HOME") {
        if !h.is_empty() {
            return PathBuf::from(h).join("runs");
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".voltiq").join("runs")
}

/// A short, filesystem-safe slug for a report's target (used in the run id).
fn slug(report: &Report) -> String {
    let raw = report
        .target
        .command
        .as_deref()
        .or(report.target.path.as_deref())
        .unwrap_or("run");
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if s.is_empty() {
        s = "run".into();
    }
    s.chars().take(48).collect()
}

/// Persist a report as a run. Returns the run id (the filename stem) on success.
pub fn save(report: &Report) -> Option<String> {
    let dir = runs_dir();
    fs::create_dir_all(&dir).ok()?;
    let id = format!("{}-{}", report.generated_at_unix_ms, slug(report));
    let path = dir.join(format!("{id}.json"));
    fs::write(&path, voltiq_core::render::to_json(report, true)).ok()?;
    Some(id)
}

/// All saved runs as `(id, report)`, newest first.
pub fn list() -> Vec<(String, Report)> {
    let mut out: Vec<(String, Report)> = Vec::new();
    if let Ok(rd) = fs::read_dir(runs_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("json") {
                continue;
            }
            let Ok(txt) = fs::read_to_string(&p) else {
                continue;
            };
            let Ok(r) = serde_json::from_str::<Report>(&txt) else {
                continue;
            };
            let id = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            out.push((id, r));
        }
    }
    out.sort_by(|a, b| b.1.generated_at_unix_ms.cmp(&a.1.generated_at_unix_ms));
    out
}

/// Resolve a run by id prefix (or any substring), newest match first.
fn find(id: &str) -> Option<(String, Report)> {
    let runs = list();
    runs.iter()
        .find(|(rid, _)| rid.starts_with(id))
        .or_else(|| runs.iter().find(|(rid, _)| rid.contains(id)))
        .cloned()
}

/// Load a saved report by id prefix, or the newest run if `id` is None.
pub fn report_for(id: Option<&str>) -> Result<Report, String> {
    match id {
        Some(x) => find(x)
            .map(|(_, r)| r)
            .ok_or_else(|| format!("no run matching '{x}'")),
        None => list()
            .into_iter()
            .next()
            .map(|(_, r)| r)
            .ok_or_else(|| "no saved runs yet — run a measurement first.".to_string()),
    }
}

fn target_of(r: &Report) -> &str {
    r.target
        .command
        .as_deref()
        .or(r.target.path.as_deref())
        .unwrap_or("—")
}

/// A coarse "X ago" string from two epoch-ms timestamps.
fn ago(then_ms: u64, now_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    if s < 60 {
        format!("{s}s ago")
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else {
        format!("{}d ago", s / 86_400)
    }
}

/// Pull a metric value by exact name from `performance.metrics`.
fn metric(r: &Report, name: &str) -> Option<f64> {
    r.performance
        .as_ref()?
        .metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.value)
}

/// A compact one-line "key metrics" summary for the runs list.
fn key_metrics(r: &Report) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = metric(r, "LCP") {
        parts.push(format!("LCP {v:.0}ms"));
    }
    if let Some(v) = metric(r, "INP") {
        parts.push(format!("INP {v:.0}ms"));
    }
    if let Some(p) = &r.performance {
        if let Some(v) = p.startup_ms {
            parts.push(format!("startup {v:.0}ms"));
        }
        if let Some(v) = p.throughput_rps {
            parts.push(format!("{v:.0} rps"));
        }
    }
    if let Some(v) = metric(r, "requests") {
        parts.push(format!("reqs {v:.0}"));
    }
    let n = r.summary.total_findings;
    parts.push(format!("{n} finding{}", if n == 1 { "" } else { "s" }));
    parts.join(" · ")
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{head}…")
    }
}

/// Human-readable listing of saved runs (for `voltiq runs`).
pub fn list_human() -> String {
    let runs = list();
    if runs.is_empty() {
        return format!(
            "no saved runs yet — they accumulate in {} after each measurement.",
            runs_dir().display()
        );
    }
    let now = now_unix_ms();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} saved run(s) in {}  (newest first):",
        runs.len(),
        runs_dir().display()
    );
    for (id, r) in &runs {
        let gate = if r.summary.passed { "PASS" } else { "FAIL" };
        let _ = writeln!(
            s,
            "  {:<26} {:>8}  {:4}  {:<26} {}",
            truncate(id, 26),
            ago(r.generated_at_unix_ms, now),
            gate,
            truncate(target_of(r), 26),
            key_metrics(r),
        );
    }
    let _ = write!(
        s,
        "\ncompare two: voltiq compare <id-a> <id-b>  (no args = newest two)"
    );
    s
}

/// Stable identity for a finding across runs: rule + where it fired (not the title,
/// which can embed run-specific numbers like elapsed ms).
fn finding_key(f: &Finding) -> String {
    let loc = f
        .location
        .as_ref()
        .map(|l| match (&l.file, l.line, &l.target) {
            (Some(file), Some(line), _) => format!("{file}:{line}"),
            (Some(file), None, _) => file.clone(),
            (None, _, Some(t)) => t.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    format!("{} @ {loc}", f.rule_id)
}

/// Is a lower value better for this metric? `None` = neutral (a count).
fn lower_is_better(name: &str) -> Option<bool> {
    let n = name.to_ascii_lowercase();
    if n.contains("throughput") || n.contains("rps") || n.contains("req / s") {
        return Some(false);
    }
    if n.contains("reqs") || n == "requests" || n == "navigations" {
        return None;
    }
    Some(true)
}

/// All comparable metrics of a report (perf scalars + the metrics[] array).
fn metrics_map(r: &Report) -> BTreeMap<String, f64> {
    let mut m = BTreeMap::new();
    if let Some(p) = &r.performance {
        if let Some(v) = p.startup_ms {
            m.insert("startup_ms".into(), v);
        }
        if let Some(v) = p.throughput_rps {
            m.insert("throughput_rps".into(), v);
        }
        if let Some(v) = p.error_rate {
            m.insert("error_rate".into(), v);
        }
        if let Some(l) = &p.latency {
            m.insert("latency_p99_ms".into(), l.p99);
        }
        for met in &p.metrics {
            m.insert(met.name.clone(), met.value);
        }
    }
    m
}

/// Diff two reports (ordered older → newer) into a human-readable report.
fn diff(older_id: &str, older: &Report, newer_id: &str, newer: &Report) -> String {
    let now = now_unix_ms();
    let mut s = String::new();
    let _ = writeln!(s, "comparing runs (older → newer):");
    let _ = writeln!(
        s,
        "  A  {:<26} {:>8}  {}",
        truncate(older_id, 26),
        ago(older.generated_at_unix_ms, now),
        target_of(older)
    );
    let _ = writeln!(
        s,
        "  B  {:<26} {:>8}  {}",
        truncate(newer_id, 26),
        ago(newer.generated_at_unix_ms, now),
        target_of(newer)
    );
    if target_of(older) != target_of(newer) {
        let _ = writeln!(
            s,
            "  note: different targets — deltas may not be comparable."
        );
    }

    // Metrics.
    let (ma, mb) = (metrics_map(older), metrics_map(newer));
    let mut names: Vec<&String> = ma.keys().chain(mb.keys()).collect();
    names.sort();
    names.dedup();
    let _ = writeln!(s, "\nmetrics (A → B):");
    let mut any = false;
    for name in names {
        let (a, b) = (ma.get(name), mb.get(name));
        let cell = |v: Option<&f64>| v.map(|x| format!("{x:.0}")).unwrap_or_else(|| "—".into());
        let tag = match (a, b) {
            (Some(a), Some(b)) => {
                let d = b - a;
                if d.abs() < f64::EPSILON {
                    "  (no change)".to_string()
                } else {
                    let dir = match lower_is_better(name) {
                        Some(true) => {
                            if d < 0.0 {
                                "better ▼"
                            } else {
                                "worse ▲"
                            }
                        }
                        Some(false) => {
                            if d > 0.0 {
                                "better ▲"
                            } else {
                                "worse ▼"
                            }
                        }
                        None => "",
                    };
                    format!("  Δ {:+.0}  {dir}", d)
                }
            }
            (None, Some(_)) => "  (new)".to_string(),
            (Some(_), None) => "  (gone)".to_string(),
            (None, None) => String::new(),
        };
        let _ = writeln!(s, "  {:<18} {:>8} → {:<8}{}", name, cell(a), cell(b), tag);
        any = true;
    }
    if !any {
        let _ = writeln!(s, "  (no metrics recorded)");
    }

    // Findings.
    let ka: BTreeMap<String, &Finding> =
        older.findings.iter().map(|f| (finding_key(f), f)).collect();
    let kb: BTreeMap<String, &Finding> =
        newer.findings.iter().map(|f| (finding_key(f), f)).collect();
    let new: Vec<&&Finding> = kb
        .iter()
        .filter(|(k, _)| !ka.contains_key(*k))
        .map(|(_, f)| f)
        .collect();
    let gone: Vec<&&Finding> = ka
        .iter()
        .filter(|(k, _)| !kb.contains_key(*k))
        .map(|(_, f)| f)
        .collect();
    let _ = writeln!(
        s,
        "\nfindings: A={} B={}  (+{} new, -{} resolved)",
        older.findings.len(),
        newer.findings.len(),
        new.len(),
        gone.len()
    );
    for f in &new {
        let _ = writeln!(
            s,
            "  + [{}] {} — {}",
            f.severity.as_str(),
            f.rule_id,
            f.title
        );
    }
    for f in &gone {
        let _ = writeln!(
            s,
            "  - [{}] {} — {} (resolved)",
            f.severity.as_str(),
            f.rule_id,
            f.title
        );
    }
    s
}

/// `voltiq compare [a] [b]`: diff two runs. With no ids, compares the newest two; with
/// two ids, those (resolved by prefix); ordered older → newer regardless.
pub fn compare_cmd(a: Option<&str>, b: Option<&str>) -> Result<String, String> {
    let (ra, rb) = match (a, b) {
        (Some(a), Some(b)) => {
            let ra = find(a).ok_or_else(|| format!("no run matching '{a}'"))?;
            let rb = find(b).ok_or_else(|| format!("no run matching '{b}'"))?;
            (ra, rb)
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err("compare needs two run ids, or none (to use the newest two).".into());
        }
        (None, None) => {
            let runs = list();
            if runs.len() < 2 {
                return Err(format!(
                    "need at least two saved runs to compare (have {}). Run more measurements first.",
                    runs.len()
                ));
            }
            (runs[0].clone(), runs[1].clone())
        }
    };
    // Order older → newer.
    let (older, newer) = if ra.1.generated_at_unix_ms <= rb.1.generated_at_unix_ms {
        (ra, rb)
    } else {
        (rb, ra)
    };
    Ok(diff(&older.0, &older.1, &newer.0, &newer.1))
}
