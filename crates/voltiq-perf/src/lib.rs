//! `voltiq-perf` — runtime performance / memory profiling.
//!
//! Launch mode spawns the target, optionally waits for an HTTP endpoint to become
//! ready (startup time), drives load (throughput + latency percentiles), and samples
//! RSS/CPU over the run (memory-growth / leak heuristic). Attach mode monitors an
//! existing pid and/or load-tests a running URL. A static-only audit flags perf
//! antipatterns without running anything.
//!
//! The V8 inspector / `bun:jsc` heap-snapshot path is a depth item (kept separate);
//! the OS-metric + load-test path here is the reliable, cross-platform core.

pub mod analysis;
pub mod detect;
pub mod load;
pub mod osmetrics;
pub mod portresolve;
pub mod watch;
pub mod web;
pub mod web_interactive;

pub use portresolve::resolve_port;
pub use watch::watch_port;
pub use web::web_perf;
pub use web_interactive::{web_connect, web_interactive, web_lab};

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use voltiq_core::{
    Confidence, Domain, Finding, LatencyStats, Location, Metric, MetricStatus, PerfReport, Series,
    Severity, Surface,
};

use crate::load::LoadResult;
use crate::osmetrics::Samples;

/// Options for a performance run.
#[derive(Debug, Clone)]
pub struct PerfOptions {
    /// Command to launch (everything after `--`). Empty in attach mode.
    pub command: Vec<String>,
    /// Attach target: a pid or `http://...` url. None in launch mode.
    pub attach: Option<String>,
    /// HTTP endpoint to measure readiness + drive load against.
    pub url: Option<String>,
    pub duration_secs: u64,
    pub concurrency: usize,
    pub warmup_secs: u64,
}

impl Default for PerfOptions {
    fn default() -> Self {
        PerfOptions {
            command: Vec::new(),
            attach: None,
            url: None,
            duration_secs: 8,
            concurrency: 10,
            warmup_secs: 1,
        }
    }
}

/// Run a performance measurement, returning the perf section and any findings.
pub fn benchmark(opts: &PerfOptions) -> (PerfReport, Vec<Finding>) {
    if let Some(target) = &opts.attach {
        run_attach(target, opts)
    } else if !opts.command.is_empty() {
        run_launch(opts)
    } else {
        (PerfReport::default(), Vec::new())
    }
}

/// Static-only performance audit (no run): antipatterns over the source tree.
pub fn static_audit(root: &Path) -> Vec<Finding> {
    analysis::scan_static_antipatterns(root)
}

/// Kills the child process when dropped, so a panicking/early-returning run never
/// leaves the target running.
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_launch(opts: &PerfOptions) -> (PerfReport, Vec<Finding>) {
    let runtime = detect::detect_runtime(&opts.command);
    let mut report = PerfReport {
        runtime: runtime.clone(),
        ..Default::default()
    };
    let mut findings = Vec::new();

    let mut cmd = Command::new(&opts.command[0]);
    cmd.args(&opts.command[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "perf.launch_failed",
                    format!("Failed to launch `{}`", opts.command.join(" ")),
                    Severity::High,
                    Confidence::Certain,
                    Surface::Runtime,
                    "The target command could not be spawned.",
                )
                .with_evidence_safe(e.to_string()),
            );
            return (report, findings);
        }
    };
    let pid = child.id();
    let _guard = ChildGuard(child);

    let stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let stop = stop.clone();
        std::thread::spawn(move || osmetrics::sample_until(pid, stop, Duration::from_millis(250)))
    };

    let target_label = opts.url.clone().unwrap_or_else(|| format!("pid {pid}"));

    if let Some(url) = &opts.url {
        report.startup_ms = load::wait_ready(url, Duration::from_secs(20));
        std::thread::sleep(Duration::from_secs(opts.warmup_secs));
        if let Some(res) = load::run_load(
            url,
            opts.concurrency,
            Duration::from_secs(opts.duration_secs),
        ) {
            findings.extend(populate_load(&mut report, &res, &target_label));
        }
    } else {
        std::thread::sleep(Duration::from_secs(opts.duration_secs));
    }

    stop.store(true, Ordering::Relaxed);
    let samples = sampler.join().unwrap_or_default();
    populate_samples(&mut report, &samples);
    findings.extend(analysis::evaluate(&report, &samples, &target_label, 1500.0));

    (report, findings)
}

fn run_attach(target: &str, opts: &PerfOptions) -> (PerfReport, Vec<Finding>) {
    let mut report = PerfReport::default();
    let mut findings = Vec::new();

    if let Ok(pid) = target.parse::<u32>() {
        let stop = Arc::new(AtomicBool::new(false));
        let sampler = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                osmetrics::sample_until(pid, stop, Duration::from_millis(250))
            })
        };
        if let Some(url) = &opts.url {
            if let Some(res) = load::run_load(
                url,
                opts.concurrency,
                Duration::from_secs(opts.duration_secs),
            ) {
                findings.extend(populate_load(&mut report, &res, &format!("pid {pid}")));
            }
        } else {
            std::thread::sleep(Duration::from_secs(opts.duration_secs));
        }
        stop.store(true, Ordering::Relaxed);
        let samples = sampler.join().unwrap_or_default();
        populate_samples(&mut report, &samples);
        findings.extend(analysis::evaluate(
            &report,
            &samples,
            &format!("pid {pid}"),
            1500.0,
        ));
    } else if target.starts_with("http://") {
        if let Some(res) = load::run_load(
            target,
            opts.concurrency,
            Duration::from_secs(opts.duration_secs),
        ) {
            findings.extend(populate_load(&mut report, &res, target));
        }
        findings.extend(analysis::evaluate(
            &report,
            &Samples::default(),
            target,
            1500.0,
        ));
    }

    (report, findings)
}

fn populate_load(report: &mut PerfReport, res: &LoadResult, target: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let loc = || Location::target(target.to_string());

    let mut sorted = res.latencies_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Throughput counts responses actually received (transport failures aren't responses).
    let throughput = if res.wall_secs > 0.0 {
        res.attempts.saturating_sub(res.transport_errors) as f64 / res.wall_secs
    } else {
        0.0
    };
    // A "failure" is a transport error or a 5xx — NOT a 4xx (that's a wrong URL / auth, a
    // config issue, not the server failing under load).
    let failures = res.transport_errors + res.http_5xx;
    let failure_rate = if res.attempts > 0 {
        failures as f64 / res.attempts as f64
    } else {
        0.0
    };
    let rate_4xx = if res.attempts > 0 {
        res.http_4xx as f64 / res.attempts as f64
    } else {
        0.0
    };
    let mean = if sorted.is_empty() {
        0.0
    } else {
        sorted.iter().sum::<f64>() / sorted.len() as f64
    };
    report.latency = Some(LatencyStats {
        min: sorted.first().copied().unwrap_or(0.0),
        mean,
        p50: load::percentile(&sorted, 50.0),
        p95: load::percentile(&sorted, 95.0),
        p99: load::percentile(&sorted, 99.0),
        max: sorted.last().copied().unwrap_or(0.0),
    });
    report.throughput_rps = Some(throughput);
    report.error_rate = Some(failure_rate);
    report.metrics.push(Metric::new(
        "requests",
        res.attempts as f64,
        "count",
        MetricStatus::Info,
    ));
    report.metrics.push(Metric::new(
        "throughput",
        throughput,
        "req/s",
        MetricStatus::Info,
    ));

    if failure_rate > 0.01 {
        findings.push(
            Finding::new(
                Domain::Performance,
                "perf.high_error_rate",
                format!(
                    "{:.1}% of requests failed under load ({} transport, {} 5xx of {})",
                    failure_rate * 100.0,
                    res.transport_errors,
                    res.http_5xx,
                    res.attempts
                ),
                Severity::High,
                Confidence::High,
                Surface::Runtime,
                "Requests failed at the transport level or returned 5xx under modest concurrency — the server is erroring/crashing under load.",
            )
            .with_location(loc())
            .with_remediation("Check for unhandled rejections, connection-pool exhaustion, or crashes under load."),
        );
    }
    // Consistent 4xx isn't a load failure — it almost always means the URL is wrong or
    // needs auth. Surface it (so numbers aren't misread) but don't fail the gate.
    if rate_4xx > 0.5 {
        findings.push(
            Finding::new(
                Domain::Performance,
                "perf.endpoint_4xx",
                format!("{:.0}% of responses were 4xx", rate_4xx * 100.0),
                Severity::Info,
                Confidence::High,
                Surface::Runtime,
                "The load target consistently returns 4xx (e.g. 401/403/404) — almost certainly the wrong URL or missing auth, not a performance failure. Point the load test at a reachable endpoint for meaningful latency/throughput.",
            )
            .with_location(loc()),
        );
    }
    findings
}

pub(crate) fn populate_samples(report: &mut PerfReport, samples: &Samples) {
    if samples.points.is_empty() {
        return;
    }
    let points: Vec<[f64; 2]> = samples
        .points
        .iter()
        .map(|&(t, rss)| [t, rss / 1_048_576.0])
        .collect();
    report.series.push(Series {
        name: "rss".into(),
        unit: "MB".into(),
        points,
    });
    // Headline peak from steady state (>= warmup), so a launcher's transient startup
    // children (npm/pnpm spawning helper processes) don't inflate the number. The full
    // series above still shows the startup spike.
    const WARMUP_MS: f64 = 1500.0;
    let steady_peak = samples
        .points
        .iter()
        .filter(|&&(t, _)| t >= WARMUP_MS)
        .map(|&(_, rss)| rss)
        .fold(0.0_f64, f64::max);
    let peak = if steady_peak > 0.0 {
        steady_peak
    } else {
        samples.peak_rss_bytes
    };
    report.metrics.push(Metric::new(
        "peak_rss",
        peak / 1_048_576.0,
        "MB",
        MetricStatus::Info,
    ));
    report.metrics.push(Metric::new(
        "peak_cpu",
        samples.peak_cpu_pct as f64,
        "%",
        MetricStatus::Info,
    ));
}
