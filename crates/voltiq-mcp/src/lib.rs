//! `voltiq-mcp` — exposes voltiq as an MCP server over stdio so it plugs into
//! Claude Code / Codex / Cursor with no heavy install (just point them at the binary).
//!
//! This is a minimal, dependency-light MCP implementation: newline-delimited JSON-RPC
//! 2.0 on stdin/stdout, handling `initialize`, `tools/list`, and `tools/call`. Tool
//! results return the deterministic [`agent_brief`](voltiq_analysis::agent_brief)
//! plus the full report JSON, so the host agent reasons over real evidence.

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use voltiq_analysis::{agent_brief, Analyzer, RulesAnalyzer};
use voltiq_core::{render, Report, Severity, TargetInfo};

const PROTOCOL_VERSION: &str = "2025-06-18";

/// The tools this server exposes.
pub const TOOL_NAMES: &[&str] = &[
    "scan_secrets",
    "scan_client_bundle",
    "audit",
    "perf_benchmark",
    "web_vitals",
];

/// Run the MCP stdio server until stdin closes.
pub fn run_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(resp) = handle(&req) {
            writeln!(out, "{resp}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Handle one JSON-RPC request, returning the response line (None for notifications).
fn handle(req: &Value) -> Option<String> {
    let method = req.get("method")?.as_str()?;
    // Notifications have no `id` and get no response.
    let id = req.get("id").cloned()?;

    let result: Result<Value, (i64, String)> = match method {
        "initialize" => Ok(json!({
            "protocolVersion": req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "voltiq", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_defs() })),
        "tools/call" => call_tool(req.get("params")),
        other => Err((-32601, format!("method not found: {other}"))),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }).to_string(),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
                .to_string()
        }
    })
}

fn path_schema(desc: &str) -> Value {
    json!({
        "type": "object",
        "properties": { "path": { "type": "string", "description": desc } },
        "required": ["path"]
    })
}

fn tool_defs() -> Value {
    json!([
        {
            "name": "scan_secrets",
            "description": "Scan a path for leaked secrets / credentials / env exposure (source, env, git history, client bundles).",
            "inputSchema": path_schema("Directory to scan.")
        },
        {
            "name": "scan_client_bundle",
            "description": "Scan a built front-end for secrets shipped to the browser (alias of scan_secrets; covers client bundles).",
            "inputSchema": path_schema("Project directory containing a build output.")
        },
        {
            "name": "audit",
            "description": "Full static audit: security scan + performance antipatterns. Returns findings, insights, and a brief for you to reason over.",
            "inputSchema": path_schema("Project directory to audit.")
        },
        {
            "name": "perf_benchmark",
            "description": "Launch a Node/Bun command and measure startup, throughput, latency and memory growth.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "array", "items": { "type": "string" }, "description": "Command argv, e.g. [\"bun\",\"run\",\"start\"]." },
                    "url": { "type": "string", "description": "HTTP endpoint to load-test." },
                    "duration_secs": { "type": "integer", "description": "Load duration (default 8)." }
                },
                "required": ["command"]
            }
        },
        {
            "name": "web_vitals",
            "description": "Capture Core Web Vitals + the full CDP report (LCP/CLS/INP with breakdowns, per-navigation waterfall categorized by cause, transfer/compression/cache findings, 1st/3rd-party weight, main-thread split) for a URL. Two modes: interactive (default) opens a REAL browser and BLOCKS until the user CLOSES the window (or max_secs) — tell the user to interact then close it; lab (interactive=false) loads the page once HEADLESS and auto-stops when the network goes idle — no clicks, good for CI / fully-automated runs. NOTE: interactive can run for minutes — the client's MCP tool timeout may need raising.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Page URL to open, e.g. http://localhost:8786/." },
                    "interactive": { "type": "boolean", "description": "true (default): headed, user drives, ends on window-close. false: headless lab, loads once, auto-stops on network-idle." },
                    "throttle": { "type": "boolean", "description": "Throttle to slow-4G + 4× CPU for field-representative numbers (lab only; defaults true in lab, ignored when interactive)." },
                    "runs": { "type": "integer", "description": "Lab only: repeat N times (1–5) and report the median + spread for stability. Default 1." },
                    "prod": { "type": "boolean", "description": "Set true when the URL is a PRODUCTION build/preview (the user built + served it). Frames the report as a real-user verdict instead of a dev measurement and stops downgrading findings as expected dev behavior. Default false." },
                    "max_secs": { "type": "integer", "description": "Safety cap on the session (default 180 interactive / 30 lab). Capture also ends on window-close (interactive) or network-idle (lab)." }
                },
                "required": ["url"]
            }
        }
    ])
}

fn call_tool(params: Option<&Value>) -> Result<Value, (i64, String)> {
    let params = params.ok_or((-32602, "missing params".into()))?;
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "missing tool name".into()))?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let report = match name {
        "scan_secrets" | "scan_client_bundle" => {
            let path = arg_path(&args)?;
            let mut r = Report::new(TargetInfo {
                path: Some(path.clone()),
                ..Default::default()
            });
            r.extend_findings(voltiq_security::scan_path(
                Path::new(&path),
                &voltiq_security::ScanOptions::default(),
            ));
            finalize(r)
        }
        "audit" => {
            let path = arg_path(&args)?;
            let mut r = Report::new(TargetInfo {
                path: Some(path.clone()),
                ..Default::default()
            });
            r.extend_findings(voltiq_security::scan_path(
                Path::new(&path),
                &voltiq_security::ScanOptions::default(),
            ));
            r.extend_findings(voltiq_perf::static_audit(Path::new(&path)));
            finalize(r)
        }
        "perf_benchmark" => {
            let command: Vec<String> = args
                .get("command")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if command.is_empty() {
                return Err((-32602, "command must be a non-empty array".into()));
            }
            let opts = voltiq_perf::PerfOptions {
                command: command.clone(),
                url: args.get("url").and_then(|v| v.as_str()).map(String::from),
                duration_secs: args
                    .get("duration_secs")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8),
                ..Default::default()
            };
            let mut r = Report::new(TargetInfo {
                command: Some(command.join(" ")),
                ..Default::default()
            });
            let (perf, findings) = voltiq_perf::benchmark(&opts);
            r.target.runtime = perf.runtime.clone();
            r.performance = Some(perf);
            r.extend_findings(findings);
            finalize(r)
        }
        "web_vitals" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or((-32602, "missing required argument: url".into()))?;
            // interactive (default): headed browser, user clicks, ends on window-close.
            // lab: headless, loads once, auto-stops when the network goes idle (no clicks).
            let interactive = args
                .get("interactive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            // Lab mode defaults to throttled (slow-4G + 4× CPU) so an automated/CI run
            // gives field-representative numbers; override with "throttle": false.
            let throttle = args
                .get("throttle")
                .and_then(|v| v.as_bool())
                .unwrap_or(!interactive);
            let default_max = if interactive {
                180
            } else if throttle {
                90
            } else {
                30
            };
            let max_secs = args
                .get("max_secs")
                .and_then(|v| v.as_u64())
                .unwrap_or(default_max);
            // Interactive: capture ends on window-close; `stop` caps a forgotten window.
            // Lab: capture ends on network-idle; `stop` caps a never-settling dev server.
            let stop = Arc::new(AtomicBool::new(false));
            {
                let s = stop.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(max_secs));
                    s.store(true, Ordering::Relaxed);
                });
            }
            let runs = args
                .get("runs")
                .and_then(|v| v.as_u64())
                .unwrap_or(1)
                .clamp(1, 5) as usize;
            // Set true when the URL is a production preview (built + served), so the report
            // is framed as a real-user verdict rather than a dev measurement.
            let prod = args.get("prod").and_then(|v| v.as_bool()).unwrap_or(false);
            let (perf, findings) = if interactive {
                voltiq_perf::web_interactive(url, stop, prod)
            } else {
                voltiq_perf::web_lab(url, stop, throttle, runs, prod)
            }
            .map_err(|e| (-32000, e))?;
            let mut r = Report::new(TargetInfo {
                command: Some(format!("web {url}")),
                runtime: perf.runtime.clone(),
                ..Default::default()
            });
            r.performance = Some(perf);
            r.extend_findings(findings);
            finalize(r)
        }
        other => return Err((-32602, format!("unknown tool: {other}"))),
    };

    let brief = agent_brief(&report);
    let report_json = render::to_json(&report, true);
    Ok(json!({
        "content": [
            { "type": "text", "text": brief },
            { "type": "text", "text": format!("```json\n{report_json}\n```") }
        ],
        "isError": false
    }))
}

fn arg_path(args: &Value) -> Result<String, (i64, String)> {
    args.get("path")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or((-32602, "missing required argument: path".into()))
}

/// Recompute the summary and run the deterministic analyzer (host-defer mode).
fn finalize(mut r: Report) -> Report {
    r.recompute_summary(Severity::High);
    RulesAnalyzer.analyze(&mut r);
    r
}
