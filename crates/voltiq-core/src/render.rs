//! Renderers: turn a [`Report`] into JSON, human-readable text, or SARIF 2.1.0.

use std::fmt::Write as _;

use serde_json::{json, Value};

use crate::report::Report;
use crate::severity::Severity;

/// Serialize the report as JSON (the dashboard/MCP contract).
pub fn to_json(report: &Report, pretty: bool) -> String {
    if pretty {
        serde_json::to_string_pretty(report).unwrap_or_default()
    } else {
        serde_json::to_string(report).unwrap_or_default()
    }
}

/// A compact human-readable summary for the terminal.
pub fn to_human(r: &Report) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "voltiq {} — report (schema v{})",
        r.tool.version, r.schema_version
    );
    if let Some(p) = &r.target.path {
        let _ = writeln!(s, "  target : {p}");
    }
    if let Some(c) = &r.target.command {
        let _ = writeln!(s, "  command: {c}");
    }
    if let Some(rt) = &r.target.runtime {
        let _ = writeln!(s, "  runtime: {rt}");
    }
    let _ = writeln!(
        s,
        "  findings: {}  (gate: {})",
        r.summary.total_findings,
        if r.summary.passed { "PASS" } else { "FAIL" }
    );
    // Highest severity first.
    for (sev, n) in r.summary.by_severity.iter().rev() {
        let _ = writeln!(s, "    {:>8}: {}", sev.as_str(), n);
    }
    for f in &r.findings {
        let loc = f
            .location
            .as_ref()
            .and_then(|l| {
                l.file.as_ref().map(|file| match l.line {
                    Some(line) => format!("{file}:{line}"),
                    None => file.clone(),
                })
            })
            .or_else(|| f.location.as_ref().and_then(|l| l.target.clone()))
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "  [{:<8}] {:<30} {} — {}",
            f.severity.as_str(),
            f.rule_id,
            loc,
            f.title
        );
    }
    if let Some(perf) = &r.performance {
        let _ = writeln!(s, "  performance:");
        if let Some(rt) = &perf.runtime {
            let _ = writeln!(s, "    runtime       : {rt}");
        }
        if let Some(v) = perf.startup_ms {
            let _ = writeln!(s, "    startup_ms    : {v:.1}");
        }
        if let Some(v) = perf.throughput_rps {
            let _ = writeln!(s, "    throughput_rps: {v:.1}");
        }
        if let Some(l) = &perf.latency {
            let _ = writeln!(
                s,
                "    latency ms    : p50={:.1} p95={:.1} p99={:.1}",
                l.p50, l.p95, l.p99
            );
        }
        for m in &perf.metrics {
            let _ = writeln!(
                s,
                "    {:<14}: {:.1} {} ({:?})",
                m.name, m.value, m.unit, m.status
            );
        }
    }
    if !r.notes.is_empty() {
        let _ = writeln!(s, "  analysis:");
        for n in &r.notes {
            // Indent each line of (possibly multi-line) notes.
            for (i, line) in n.lines().enumerate() {
                let prefix = if i == 0 { "    - " } else { "      " };
                let _ = writeln!(s, "{prefix}{line}");
            }
        }
    }
    s
}

/// SARIF 2.1.0 for CI code-scanning dashboards.
pub fn to_sarif(r: &Report) -> String {
    let results: Vec<Value> = r
        .findings
        .iter()
        .map(|f| {
            let level = match f.severity {
                Severity::Critical | Severity::High => "error",
                Severity::Medium => "warning",
                Severity::Low | Severity::Info => "note",
            };
            let mut locations = Vec::new();
            if let Some(loc) = &f.location {
                if let Some(file) = &loc.file {
                    locations.push(json!({
                        "physicalLocation": {
                            "artifactLocation": { "uri": file },
                            "region": { "startLine": loc.line.unwrap_or(1).max(1) }
                        }
                    }));
                }
            }
            json!({
                "ruleId": f.rule_id,
                "level": level,
                "message": { "text": format!("{}: {}", f.title, f.description) },
                "locations": locations
            })
        })
        .collect();

    let sarif = json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "voltiq",
                    "version": r.tool.version,
                    "informationUri": "https://github.com/nguyentuansi/voltiq"
                }
            },
            "results": results
        }]
    });
    serde_json::to_string_pretty(&sarif).unwrap_or_default()
}
