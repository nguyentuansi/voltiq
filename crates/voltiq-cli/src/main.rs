//! The `voltiq` CLI: `scan`, `perf`, `watch`, `web`, `audit`, `serve`, `mcp`,
//! `runs`, `compare`.

mod runs;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::{Parser, Subcommand, ValueEnum};
use voltiq_core::{render, Report, Severity, TargetInfo};

#[derive(Parser)]
#[command(
    name = "voltiq",
    version,
    about = "AI-first performance + security scanner for Node.js / Bun apps"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Output format.
    #[arg(long, global = true, value_enum, default_value_t = Format::Human)]
    format: Format,

    /// Pretty-print JSON output.
    #[arg(long, global = true)]
    pretty: bool,

    /// Fail (non-zero exit) if any finding is at or above this severity.
    #[arg(long, global = true, value_enum, default_value_t = SeverityArg::High)]
    fail_on: SeverityArg,

    /// Use an LLM (BYO key, ANTHROPIC_API_KEY) for analysis instead of deterministic rules only.
    #[arg(long, global = true)]
    ai: bool,
}

/// Run the analysis layer over a report: deterministic rules always, plus an LLM pass
/// when `--ai` is set.
fn analyze(report: &mut voltiq_core::Report, ai: bool) {
    use voltiq_analysis::Analyzer;
    if ai {
        voltiq_analysis::LlmAnalyzer::default().analyze(report);
    } else {
        voltiq_analysis::RulesAnalyzer.analyze(report);
    }
}

#[derive(ValueEnum, Clone, Copy)]
enum Format {
    Human,
    Json,
    Sarif,
    /// Agent-ready markdown brief — for a local agent (Claude Code / Codex) to read and act on.
    Brief,
}

#[derive(ValueEnum, Clone, Copy)]
enum SeverityArg {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::Info => Severity::Info,
            SeverityArg::Low => Severity::Low,
            SeverityArg::Medium => Severity::Medium,
            SeverityArg::High => Severity::High,
            SeverityArg::Critical => Severity::Critical,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Scan a path for leaked secrets / credentials / env exposure.
    Scan {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Opt-in: verify found secrets against provider APIs (network, read-only).
        #[arg(long)]
        verify: bool,
    },
    /// Measure performance/memory of a Node/Bun app (command after `--`).
    Perf {
        /// The command to launch, e.g. `voltiq perf --url http://localhost:3000 -- bun run start`.
        #[arg(last = true)]
        cmd: Vec<String>,
        /// Attach to an already-running target (pid or http:// url) instead of launching.
        #[arg(long)]
        attach: Option<String>,
        /// HTTP endpoint to measure readiness + drive load against.
        #[arg(long)]
        url: Option<String>,
        /// Load duration in seconds.
        #[arg(long, default_value_t = 8)]
        duration: u64,
        /// Concurrent load workers.
        #[arg(long, default_value_t = 10)]
        concurrency: usize,
    },
    /// Watch a running app by port: passively capture memory/CPU while you use it.
    Watch {
        /// The port your app listens on, e.g. `voltiq watch 8786`.
        port: u16,
        /// Stop automatically after N seconds (default: run until Ctrl-C).
        #[arg(long = "for")]
        for_secs: Option<u64>,
    },
    /// Measure browser/page performance of a URL. DEFAULT: opens a real browser, captures
    /// your clicks, and shows the report when you CLOSE the window. (One capture mode at a
    /// time: default/interactive · --lab · --connect · --lighthouse.)
    Web {
        /// The page URL, e.g. `voltiq web http://localhost:8786/`.
        url: String,
        /// Explicitly request the default mode: open a real browser, capture YOUR clicks,
        /// and show the report when you close the window (or Ctrl-C). This IS the default.
        #[arg(long, conflicts_with_all = ["connect", "lab", "lighthouse", "desktop"])]
        interactive: bool,
        /// Headless lab mode: load the page once with no window and auto-stop when the
        /// network goes idle (CI / automated — captures the full CDP report, no clicks).
        #[arg(long, conflicts_with_all = ["connect", "lighthouse", "desktop"])]
        lab: bool,
        /// Connect to a browser already running with --remote-debugging-port=<PORT> and
        /// capture vitals from the tab you drive, live, until Ctrl-C (no window spawned).
        #[arg(long, conflicts_with_all = ["lighthouse", "desktop"])]
        connect: Option<u16>,
        /// Lighthouse audit (needs Node + Chrome): a one-shot lab score instead of a live
        /// capture. The old default — now opt-in.
        #[arg(long)]
        lighthouse: bool,
        /// Lighthouse desktop preset (implies --lighthouse; lighter throttling).
        #[arg(long)]
        desktop: bool,
        /// Throttle to slow-4G + 4× CPU so numbers predict the field (lab mode only).
        #[arg(long, requires = "lab")]
        throttle: bool,
        /// Lab mode: repeat the capture N times and report the median + spread.
        #[arg(long, default_value_t = 1, requires = "lab")]
        runs: usize,
        /// Open a localhost dashboard of the run in your browser after capture (any mode;
        /// the default interactive mode already opens it on close).
        #[arg(long)]
        serve: bool,
        /// Terminal only — don't open the dashboard after the default interactive capture.
        #[arg(long, conflicts_with = "serve")]
        no_serve: bool,
        /// Treat the URL as a PRODUCTION build (you started your own prod preview, e.g.
        /// `bun run build && bun run preview`). Flips the report from "dev, re-measure prod"
        /// to a real-user verdict and stops downgrading findings as expected dev behavior.
        /// Pair with --throttle for field-representative numbers.
        #[arg(long)]
        prod: bool,
    },
    /// Full audit: security scan + performance.
    Audit {
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Write a self-contained HTML report to this path.
        #[arg(long)]
        html: Option<PathBuf>,
    },
    /// Audit a path and serve the live dashboard.
    Serve {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value = "127.0.0.1:7878")]
        addr: String,
    },
    /// Run as an MCP server for Claude Code / Codex / Cursor.
    Mcp {
        /// Serve over streamable HTTP at this address instead of stdio.
        #[arg(long)]
        http: Option<String>,
    },
    /// List saved measurement runs (stored under ~/.voltiq/runs).
    Runs,
    /// Diff two saved runs (no args = the newest two).
    Compare {
        /// First run id (prefix); resolved against ~/.voltiq/runs.
        a: Option<String>,
        /// Second run id (prefix).
        b: Option<String>,
    },
    /// Print an agent-ready brief of a saved run (no arg = newest) for a local agent to act on.
    Explain {
        /// Run id (prefix); resolved against ~/.voltiq/runs. Defaults to the newest run.
        run: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let fail_on: Severity = cli.fail_on.into();

    // One Ctrl-C handler for the whole process (ctrlc allows only one): it flips `stop`,
    // which the capture loops watch, and which we reuse to gracefully stop `--serve`.
    let stop = Arc::new(AtomicBool::new(false));
    {
        let s = stop.clone();
        let _ = ctrlc::set_handler(move || s.store(true, Ordering::Relaxed));
    }
    // If set by `web --serve`, pop up a localhost dashboard after the run is rendered.
    let mut serve_after: Option<String> = None;

    let report: Option<Report> = match &cli.command {
        Command::Scan { path, verify } => {
            let mut r = Report::new(TargetInfo {
                path: Some(path.display().to_string()),
                ..Default::default()
            });
            let opts = voltiq_security::ScanOptions {
                verify: *verify,
                ..Default::default()
            };
            r.extend_findings(voltiq_security::scan_path(path, &opts));
            r.recompute_summary(fail_on);
            analyze(&mut r, cli.ai);
            Some(r)
        }
        Command::Perf {
            cmd,
            attach,
            url,
            duration,
            concurrency,
        } => {
            let mut r = Report::new(TargetInfo {
                command: (!cmd.is_empty()).then(|| cmd.join(" ")),
                ..Default::default()
            });
            let (perf, findings) = voltiq_perf::benchmark(&voltiq_perf::PerfOptions {
                command: cmd.clone(),
                attach: attach.clone(),
                url: url.clone(),
                duration_secs: *duration,
                concurrency: *concurrency,
                ..Default::default()
            });
            r.target.runtime = perf.runtime.clone();
            r.performance = Some(perf);
            r.extend_findings(findings);
            r.recompute_summary(fail_on);
            analyze(&mut r, cli.ai);
            Some(r)
        }
        Command::Watch { port, for_secs } => {
            let max = for_secs.map(std::time::Duration::from_secs);
            match voltiq_perf::watch_port(*port, stop.clone(), max) {
                Ok((perf, findings)) => {
                    let mut r = Report::new(TargetInfo {
                        command: Some(format!("watch :{port}")),
                        runtime: perf.runtime.clone(),
                        ..Default::default()
                    });
                    r.performance = Some(perf);
                    r.extend_findings(findings);
                    r.recompute_summary(fail_on);
                    analyze(&mut r, cli.ai);
                    Some(r)
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Command::Web {
            url,
            desktop,
            interactive: _,
            connect,
            lab,
            lighthouse,
            throttle,
            runs,
            serve,
            no_serve,
            prod,
        } => {
            // Capture-mode precedence; the final `else` is the DEFAULT: open a browser,
            // capture the user's clicks, and show the report when they close the window.
            let result = if let Some(port) = connect {
                voltiq_perf::web_connect(*port, url, stop.clone(), *prod)
            } else if *lab {
                voltiq_perf::web_lab(url, stop.clone(), *throttle, *runs, *prod)
            } else if *lighthouse || *desktop {
                voltiq_perf::web_perf(url, *desktop)
            } else {
                // DEFAULT (also `--interactive`): auto-start a real browser → close it → report.
                voltiq_perf::web_interactive(url, stop.clone(), *prod)
            };
            match result {
                Ok((perf, findings)) => {
                    let mut r = Report::new(TargetInfo {
                        command: Some(format!("web {url}")),
                        runtime: perf.runtime.clone(),
                        ..Default::default()
                    });
                    r.performance = Some(perf);
                    r.extend_findings(findings);
                    r.recompute_summary(fail_on);
                    analyze(&mut r, cli.ai);
                    // The default (interactive, human-at-screen) mode pops the dashboard on
                    // close; `--serve` forces it for any mode; `--no-serve` opts out. Lab /
                    // lighthouse / connect stay terminal-only unless `--serve` (CI-safe).
                    let interactive_default =
                        connect.is_none() && !*lab && !*lighthouse && !*desktop;
                    if *serve || (interactive_default && !*no_serve) {
                        // Ephemeral localhost port so it never collides with `serve`.
                        serve_after = Some("127.0.0.1:0".to_string());
                    }
                    Some(r)
                }
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        Command::Audit { path, html } => {
            let mut r = Report::new(TargetInfo {
                path: Some(path.display().to_string()),
                ..Default::default()
            });
            r.extend_findings(voltiq_security::scan_path(
                path,
                &voltiq_security::ScanOptions::default(),
            ));
            r.extend_findings(voltiq_perf::static_audit(path));
            r.recompute_summary(fail_on);
            analyze(&mut r, cli.ai);
            if let Some(out) = html {
                match std::fs::write(out, voltiq_server::render_html(&r)) {
                    Ok(()) => eprintln!("wrote HTML report to {}", out.display()),
                    Err(e) => eprintln!("failed to write {}: {e}", out.display()),
                }
            }
            Some(r)
        }
        Command::Serve { path, addr } => {
            let mut r = Report::new(TargetInfo {
                path: Some(path.display().to_string()),
                ..Default::default()
            });
            r.extend_findings(voltiq_security::scan_path(
                path,
                &voltiq_security::ScanOptions::default(),
            ));
            r.extend_findings(voltiq_perf::static_audit(path));
            r.recompute_summary(fail_on);
            analyze(&mut r, cli.ai);
            if let Err(e) = voltiq_server::serve_blocking(r, addr) {
                eprintln!("serve error: {e}");
                return ExitCode::FAILURE;
            }
            None
        }
        Command::Mcp { http } => {
            if let Some(a) = http {
                eprintln!("`voltiq mcp --http {a}` (streamable HTTP) is not implemented yet; use stdio (`voltiq mcp`).");
                return ExitCode::FAILURE;
            }
            if let Err(e) = voltiq_mcp::run_stdio() {
                eprintln!("mcp error: {e}");
                return ExitCode::FAILURE;
            }
            None
        }
        Command::Runs => {
            println!("{}", runs::list_human());
            return ExitCode::SUCCESS;
        }
        Command::Compare { a, b } => match runs::compare_cmd(a.as_deref(), b.as_deref()) {
            Ok(s) => {
                println!("{s}");
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        },
        Command::Explain { run } => match runs::report_for(run.as_deref()) {
            Ok(r) => {
                println!("{}", voltiq_analysis::agent_brief(&r));
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        },
    };

    let Some(report) = report else {
        return ExitCode::SUCCESS;
    };

    // Persist every measurement so it can be listed (`voltiq runs`) and diffed
    // (`voltiq compare`) later.
    if let Some(id) = runs::save(&report) {
        eprintln!("run saved: {id}  ·  voltiq runs · voltiq compare");
    }

    let out = match cli.format {
        Format::Human => render::to_human(&report),
        Format::Json => render::to_json(&report, cli.pretty),
        Format::Sarif => render::to_sarif(&report),
        Format::Brief => voltiq_analysis::agent_brief(&report),
    };
    println!("{out}");

    // `web --serve`: render the run as a themed page and host it on localhost until
    // Ctrl-C. Reset `stop` first so the Ctrl-C that ended capture doesn't also end this.
    if let Some(addr) = serve_after {
        stop.store(false, Ordering::Relaxed);
        let html = voltiq_server::render_html(&report);
        if let Err(e) = voltiq_server::serve_html_until(html, &addr, stop.clone(), true) {
            eprintln!("serve error: {e}");
        }
    }

    if report.summary.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
