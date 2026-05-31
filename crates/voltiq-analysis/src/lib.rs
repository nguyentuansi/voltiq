//! `voltiq-analysis` — the hybrid AI layer.
//!
//! - [`RulesAnalyzer`] — always-on, deterministic. Enriches findings with an
//!   "investigate" hint and writes cross-cutting insights into `report.notes`. This is
//!   the fallback that needs no network and no key.
//! - [`agent_brief`] — renders the report as a compact markdown brief. In MCP
//!   (host-defer) mode this is what the tool hands back so Claude Code / Codex reason
//!   over real evidence and propose fixes — no API key, no cost.
//! - [`LlmAnalyzer`] — opt-in, standalone, BYO-key. Sends the brief to an LLM and appends
//!   a prioritized plan to `report.notes`. Routes through any OpenAI-compatible endpoint
//!   (LiteLLM proxy, OpenRouter, Ollama, …) via `VOLTIQ_LLM_BASE_URL`, else native
//!   Anthropic. Falls back to the deterministic path if no key / on any error.

use serde_json::json;
use voltiq_core::{MetricStatus, Report, Severity};

/// Produces analyzed/enriched output from a raw [`Report`].
pub trait Analyzer {
    fn analyze(&self, report: &mut Report);
}

/// Deterministic, always-available analyzer.
#[derive(Debug, Default, Clone)]
pub struct RulesAnalyzer;

impl Analyzer for RulesAnalyzer {
    fn analyze(&self, report: &mut Report) {
        for f in &mut report.findings {
            if !f.metadata.contains_key("investigate") {
                f.metadata
                    .insert("investigate".into(), json!(investigate_hint(&f.rule_id)));
            }
        }
        report.notes.extend(insights(report));
    }
}

/// Per-rule "what to investigate" hint, by rule-id prefix.
fn investigate_hint(rule_id: &str) -> &'static str {
    if rule_id.starts_with("client.") {
        "Shipped to every visitor's browser — treat as a public disclosure; rotate and move server-side."
    } else if rule_id.starts_with("secret.") {
        "Confirm whether this credential is live (try `--verify`); rotate it and load it from the environment / a secrets manager."
    } else if rule_id.starts_with("env.") || rule_id.starts_with("git.") {
        "Assume the secret is compromised; rotate it and purge the file from git history."
    } else if rule_id == "perf.memory_growth" {
        "Capture heap snapshots before/after load and diff retained objects; bound caches and clear listeners/intervals on teardown."
    } else if rule_id.starts_with("perf.") {
        "Profile the hot path; move blocking/sync work off the event loop."
    } else {
        "Review the finding's evidence and remediation."
    }
}

/// Deterministic cross-cutting insights derived from the finding set.
fn insights(report: &Report) -> Vec<String> {
    let mut out = Vec::new();
    let count = |pred: &dyn Fn(&str) -> bool| {
        report
            .findings
            .iter()
            .filter(|f| pred(f.rule_id.as_str()))
            .count()
    };

    let client = count(&|r| r.starts_with("client."));
    if client > 0 {
        out.push(format!(
            "{client} finding(s) are in the built client bundle — publicly downloadable by anyone. Prioritise these."
        ));
    }
    let secrets =
        count(&|r| r.starts_with("secret.") || r.starts_with("env.") || r.starts_with("git."));
    if secrets > 0 {
        out.push(format!(
            "{secrets} credential/secret exposure(s). Rotate first, then remove from source/history."
        ));
    }
    if report
        .findings
        .iter()
        .any(|f| f.rule_id == "perf.memory_growth")
    {
        out.push("Sustained memory growth detected under load — likely a leak.".into());
    }
    let criticals = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Critical)
        .count();
    if criticals > 0 {
        out.push(format!(
            "{criticals} critical finding(s) — block release until resolved."
        ));
    }
    if report.summary.passed && report.findings.is_empty() {
        out.push("No findings — nothing blocking.".into());
    }
    out
}

/// Render the report as a compact markdown brief for a host agent to reason over.
pub fn agent_brief(report: &Report) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    let target = report
        .target
        .path
        .as_deref()
        .or(report.target.command.as_deref())
        .unwrap_or("—");
    let _ = writeln!(s, "# voltiq scan brief");
    let _ = writeln!(
        s,
        "- target: `{target}`\n- gate: {}\n- findings: {}",
        if report.summary.passed {
            "PASS"
        } else {
            "FAIL"
        },
        report.summary.total_findings
    );
    if let Some(perf) = &report.performance {
        let _ = write!(s, "- performance:");
        if let Some(v) = perf.startup_ms {
            let _ = write!(s, " startup={v:.0}ms");
        }
        if let Some(v) = perf.throughput_rps {
            let _ = write!(s, " throughput={v:.0}rps");
        }
        if let Some(l) = &perf.latency {
            let _ = write!(s, " p99={:.1}ms", l.p99);
        }
        let _ = writeln!(s);
        if !perf.metrics.is_empty() {
            let _ = writeln!(s, "\n## metrics");
            for m in &perf.metrics {
                let val = if m.unit.is_empty() {
                    format!("{:.3}", m.value)
                } else {
                    format!("{:.0} {}", m.value, m.unit)
                };
                let flag = match m.status {
                    MetricStatus::Warn => " ⚠",
                    MetricStatus::Fail => " ✗",
                    _ => "",
                };
                let _ = writeln!(s, "- {}: {val}{flag}", m.name);
            }
        }
    }

    // Findings, highest severity first, capped.
    let mut findings: Vec<_> = report.findings.iter().collect();
    findings.sort_by_key(|f| std::cmp::Reverse(f.severity));
    let _ = writeln!(s, "\n## findings");
    for f in findings.iter().take(20) {
        let loc = f
            .location
            .as_ref()
            .and_then(|l| {
                l.file
                    .as_ref()
                    .map(|file| match l.line {
                        Some(n) => format!("{file}:{n}"),
                        None => file.clone(),
                    })
                    .or_else(|| l.target.clone())
            })
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "- **[{}]** `{}` {} — {}",
            f.severity.as_str(),
            f.rule_id,
            loc,
            f.title
        );
        if !f.description.trim().is_empty() {
            // Collapse to one line, capped — the detail carries the evidence (waterfall,
            // breakdown, slowest requests) the model needs to locate the fix.
            let d: String = f
                .description
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let d: String = d.chars().take(240).collect();
            let _ = writeln!(s, "  - detail: {d}");
        }
        if let Some(rem) = &f.remediation {
            let _ = writeln!(s, "  - fix: {rem}");
        }
        if let Some(hint) = f.metadata.get("investigate").and_then(|v| v.as_str()) {
            let _ = writeln!(s, "  - investigate: {hint}");
        }
    }
    if !report.notes.is_empty() {
        let _ = writeln!(s, "\n## insights");
        for n in &report.notes {
            let _ = writeln!(s, "- {n}");
        }
    }
    let _ = writeln!(
        s,
        "\n## task\nFor each finding above, locate the responsible code/config in this repo \
         and propose a concrete edit (file + change). Lead with Critical/High and anything \
         credential- or exposure-related. For findings noted as a dev build, say that a \
         production build differs rather than proposing a fix."
    );
    s
}

/// Opt-in LLM analyzer (standalone, BYO key). Provider-agnostic shape; Anthropic by
/// default. Requires `ANTHROPIC_API_KEY`; otherwise records a note and stays
/// deterministic.
#[derive(Debug, Clone)]
pub struct LlmAnalyzer {
    pub model: String,
}

impl Default for LlmAnalyzer {
    fn default() -> Self {
        LlmAnalyzer {
            model: std::env::var("VOLTIQ_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-6".to_string()),
        }
    }
}

impl Analyzer for LlmAnalyzer {
    fn analyze(&self, report: &mut Report) {
        // Always run the deterministic pass first (the fallback / baseline).
        RulesAnalyzer.analyze(report);

        let prompt = Self::prompt(report);
        // Route through an OpenAI-compatible endpoint (a LiteLLM proxy, OpenRouter, Groq,
        // local Ollama/vLLM, …) when VOLTIQ_LLM_BASE_URL is set; else native Anthropic.
        let base = std::env::var("VOLTIQ_LLM_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());
        let result = match base {
            Some(base) => self.call_openai(&base, &prompt),
            None => self.call_anthropic(&prompt),
        };
        match result {
            Ok(text) => report
                .notes
                .push(format!("AI analysis ({}):\n{text}", self.model)),
            Err(note) => report.notes.push(note),
        }
    }
}

impl LlmAnalyzer {
    /// The reviewer prompt wrapping the report brief.
    fn prompt(report: &Report) -> String {
        format!(
            "You are a senior security + performance reviewer. Given this scan brief, \
             produce a short, prioritized remediation plan (markdown, under 250 words). \
             Lead with anything publicly exposed or credential-related.\n\n{}",
            agent_brief(report)
        )
    }

    /// OpenAI-compatible chat-completions — works with any router (LiteLLM, OpenRouter,
    /// Groq, Ollama, vLLM, …). Key from `VOLTIQ_LLM_API_KEY` or `OPENAI_API_KEY`
    /// (optional — keyless local proxies work too).
    fn call_openai(&self, base: &str, prompt: &str) -> Result<String, String> {
        let url = format!("{}/chat/completions", base.trim_end_matches('/'));
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }],
        });
        let mut req = ureq::post(&url).set("content-type", "application/json");
        if let Ok(key) =
            std::env::var("VOLTIQ_LLM_API_KEY").or_else(|_| std::env::var("OPENAI_API_KEY"))
        {
            if !key.is_empty() {
                req = req.set("authorization", &format!("Bearer {key}"));
            }
        }
        match req.send_json(body) {
            Ok(resp) => resp
                .into_json::<serde_json::Value>()
                .map_err(|e| format!("AI analysis: bad response ({e})."))
                .and_then(|v| {
                    v["choices"][0]["message"]["content"]
                        .as_str()
                        .map(|s| s.to_string())
                        .ok_or_else(|| "AI analysis: unexpected response shape.".to_string())
                }),
            Err(e) => Err(format!(
                "AI analysis skipped: request to {url} failed ({e})."
            )),
        }
    }

    /// Native Anthropic Messages API (the default when no base URL is configured).
    fn call_anthropic(&self, prompt: &str) -> Result<String, String> {
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            return Err(
                "AI analysis skipped: set VOLTIQ_LLM_BASE_URL (OpenAI-compatible, e.g. a LiteLLM proxy) or ANTHROPIC_API_KEY — deterministic rules still applied."
                    .into(),
            );
        };
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }],
        });
        match ureq::post("https://api.anthropic.com/v1/messages")
            .set("x-api-key", &key)
            .set("anthropic-version", "2023-06-01")
            .set("content-type", "application/json")
            .send_json(body)
        {
            Ok(resp) => resp
                .into_json::<serde_json::Value>()
                .map_err(|e| format!("AI analysis: bad response ({e})."))
                .and_then(|v| {
                    v["content"][0]["text"]
                        .as_str()
                        .map(|s| s.to_string())
                        .ok_or_else(|| "AI analysis: unexpected response shape.".to_string())
                }),
            Err(e) => Err(format!("AI analysis skipped: request failed ({e}).")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voltiq_core::{Confidence, Domain, Finding, Location, Severity, Surface, TargetInfo};

    fn sample() -> Report {
        let mut r = Report::new(TargetInfo {
            path: Some("proj".into()),
            ..Default::default()
        });
        r.add_finding(
            Finding::new(
                Domain::Security,
                "client.supabase_service_role",
                "service_role JWT in bundle",
                Severity::Critical,
                Confidence::High,
                Surface::ClientBundle,
                "privileged key shipped to browser",
            )
            .with_location(Location::file("dist/app.js", 1))
            .with_remediation("Use the anon key in the browser."),
        );
        r.recompute_summary(Severity::High);
        r
    }

    #[test]
    fn rules_enrich_and_insight() {
        let mut r = sample();
        RulesAnalyzer.analyze(&mut r);
        assert!(r.findings[0].metadata.contains_key("investigate"));
        assert!(r.notes.iter().any(|n| n.contains("client bundle")));
        assert!(r.notes.iter().any(|n| n.contains("critical")));
    }

    #[test]
    fn brief_has_sections() {
        let mut r = sample();
        RulesAnalyzer.analyze(&mut r);
        let brief = agent_brief(&r);
        assert!(brief.contains("# voltiq scan brief"));
        assert!(brief.contains("## findings"));
        assert!(brief.contains("client.supabase_service_role"));
        assert!(brief.contains("investigate:"));
    }
}
