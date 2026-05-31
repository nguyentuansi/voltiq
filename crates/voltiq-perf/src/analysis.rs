//! Turn measured metrics into findings (the predefined perf rules + leak heuristic),
//! plus a static scan for performance antipatterns common in AI-generated code.

use std::path::Path;

use voltiq_core::{Confidence, Domain, Finding, Location, PerfReport, Severity, Surface};

use crate::osmetrics::Samples;

/// Least-squares slope of `(x, y)` points (units: y per x). 0 if fewer than 2 points.
pub fn linear_slope(points: &[(f64, f64)]) -> f64 {
    let n = points.len() as f64;
    if n < 2.0 {
        return 0.0;
    }
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
    for &(x, y) in points {
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    let denom = n * sxx - sx * sx;
    if denom.abs() < f64::EPSILON {
        return 0.0;
    }
    (n * sxy - sx * sy) / denom
}

/// Evaluate a finished perf run: latency / error-rate / cold-start thresholds and a
/// memory-growth (leak) heuristic from the RSS samples.
pub fn evaluate(
    report: &PerfReport,
    samples: &Samples,
    target: &str,
    warmup_ms: f64,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let loc = || Location::target(target.to_string());

    // Tail-latency amplification: flag when p99 is many× the median, not on an absolute ms.
    // This is endpoint-agnostic — a uniformly-heavy API (high but flat latency) isn't
    // penalised, while a big p99/p50 ratio is the real signal of GC pauses, lock contention
    // or event-loop blocking. (Absolute ms can't be a universal threshold, and on localhost
    // the tail is partly the load generator's own CPU contention.)
    // The error-rate rule moved to `populate_load`, where 5xx/transport failures can be told
    // apart from 4xx (a wrong URL / missing auth, not a load failure).
    if let Some(lat) = &report.latency {
        let ratio = if lat.p50 > 0.0 {
            lat.p99 / lat.p50
        } else {
            0.0
        };
        // Ignore trivially small latencies where the ratio is just noise (0.4 ms → 6 ms).
        if ratio >= 10.0 && lat.p99 >= 50.0 {
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "perf.latency_tail",
                    format!(
                        "Tail latency p99 is {ratio:.0}× the median (p50 {:.0} ms → p99 {:.0} ms)",
                        lat.p50, lat.p99
                    ),
                    if ratio >= 25.0 { Severity::High } else { Severity::Medium },
                    Confidence::High,
                    Surface::Runtime,
                    "Most requests are fast but a tail is far slower — a sign of GC pauses, lock contention, or work blocking the event loop, not steady-state cost.",
                )
                .with_location(loc())
                .with_remediation("Profile the slow tail (event-loop lag, GC, locks); move sync/CPU-heavy work off the request path and bound queues."),
            );
        }
    }

    if let (Some(startup), Some(rt)) = (report.startup_ms, report.runtime.as_deref()) {
        let baseline = crate::detect::cold_start_baseline_ms(rt);
        if startup > baseline {
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "perf.slow_cold_start",
                    format!("Slow cold start: {startup:.0} ms (> {baseline:.0} ms baseline for {rt})"),
                    Severity::Medium,
                    Confidence::Medium,
                    Surface::Runtime,
                    "Time-to-first-response exceeds the expected cold-start baseline for this runtime.",
                )
                .with_location(loc())
                .with_remediation("Defer heavy top-level work, lazy-load modules, and avoid synchronous I/O during boot."),
            );
        }
    }

    // Memory-growth (leak) heuristic over RSS. Note: RSS catches leaks that *touch*
    // memory (retained objects/strings/filled buffers — the common case) but misses
    // off-heap allocations that are never written (e.g. zero-filled Buffer.alloc kept
    // on the kernel zero-page). Those need process.memoryUsage().external / heap
    // snapshots via the inspector path (a depth item).
    //
    // Ignore warmup (JIT/allocation ramp is normal),
    // then require *sustained* growth: a positive slope, the last third meaningfully
    // above the first third, and a non-trivial absolute increase. This avoids flagging
    // a server that simply warms up and then plateaus.
    let post: Vec<(f64, f64)> = samples
        .points
        .iter()
        .copied()
        .filter(|&(t, _)| t >= warmup_ms)
        .collect();
    if post.len() >= 6 {
        let third = (post.len() / 3).max(1);
        let mean = |s: &[(f64, f64)]| s.iter().map(|&(_, y)| y).sum::<f64>() / s.len() as f64;
        let early = mean(&post[..third]);
        let mid = mean(&post[third..(2 * third).min(post.len())]);
        let late = mean(&post[post.len() - third..]);
        let growth_mb = (late - early) / 1_048_576.0;
        // A real leak keeps climbing to the very end; an app that lazy-loads / warms up
        // and then plateaus has late ≈ mid. Require BOTH overall growth (late ≫ early) AND
        // that it's STILL rising in the final third (late > mid) — the earlier early-vs-late
        // test alone flagged warmup-then-plateau as a "leak" (a false positive).
        let still_rising = late > mid * 1.05;
        if early > 0.0 && late > early * 1.25 && growth_mb > 25.0 && still_rising {
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "perf.memory_growth",
                    format!("RSS grew {growth_mb:.1} MB and was still climbing at the end of the run"),
                    Severity::Medium,
                    Confidence::Medium,
                    Surface::Runtime,
                    "Resident memory grew under load and had not plateaued by the end — a possible leak (unbounded cache, retained listeners, accumulating globals). This is an RSS heuristic; confirm with a heap snapshot before acting.",
                )
                .with_location(loc())
                .with_remediation("Capture heap snapshots before/after load and diff retained objects; bound caches and remove listeners/intervals on teardown."),
            );
        }
    }

    findings
}

// ── Static performance antipatterns (no run required) ─────────────────────────

/// Scan a project's source for performance antipatterns common in AI-generated code.
pub fn scan_static_antipatterns(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in source_files(root) {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let path = file.display().to_string();

        // setInterval without a matching clearInterval in the same file.
        if content.contains("setInterval(") && !content.contains("clearInterval(") {
            findings.push(antipattern(
                "perf.uncleared_interval",
                "setInterval without clearInterval",
                Severity::Medium,
                &path,
                line_containing(&content, "setInterval("),
                "A setInterval timer is never cleared. The closure (and anything it captures) is retained for the process lifetime.",
                "Store the timer id and clearInterval() it on shutdown / unmount.",
            ));
        }

        // Synchronous filesystem reads (block the event loop in request paths).
        if let Some(line) = line_containing(&content, "readFileSync(") {
            findings.push(antipattern(
                "perf.sync_fs_in_code",
                "Synchronous filesystem read",
                Severity::Low,
                &path,
                Some(line),
                "Synchronous fs calls block the event loop. In a request path they serialize all traffic.",
                "Use the async fs APIs (fs/promises) or streaming; reserve *Sync calls for startup only.",
            ));
        }
    }
    findings
}

fn antipattern(
    rule_id: &str,
    title: &str,
    severity: Severity,
    file: &str,
    line: Option<u32>,
    description: &str,
    remediation: &str,
) -> Finding {
    let mut f = Finding::new(
        Domain::Performance,
        rule_id.to_string(),
        title.to_string(),
        severity,
        Confidence::Low,
        Surface::Source,
        description.to_string(),
    )
    .with_remediation(remediation.to_string());
    if let Some(l) = line {
        f = f.with_location(Location::file(file, l));
    }
    f
}

fn line_containing(content: &str, needle: &str) -> Option<u32> {
    content
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(needle))
        .map(|(i, _)| i as u32 + 1)
}

/// Source files worth scanning for antipatterns (JS/TS family), skipping deps/builds.
fn source_files(root: &Path) -> Vec<std::path::PathBuf> {
    const EXTS: &[&str] = &[
        "js", "mjs", "cjs", "ts", "mts", "cts", "jsx", "tsx", "svelte", "vue",
    ];
    const SKIP: &[&str] = &[
        "node_modules",
        ".git",
        "dist",
        "build",
        "out",
        ".next",
        ".svelte-kit",
        "target",
        ".turbo",
    ];
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).into_iter().flatten() {
        let p = entry.path();
        if p.components()
            .any(|c| SKIP.contains(&c.as_os_str().to_string_lossy().as_ref()))
        {
            continue;
        }
        if entry.file_type().is_file() {
            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                if EXTS.contains(&ext) {
                    out.push(p.to_path_buf());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn slope_detects_growth() {
        let points: Vec<(f64, f64)> = (0..10)
            .map(|i| (i as f64, 100.0 + 10.0 * i as f64))
            .collect();
        assert!((linear_slope(&points) - 10.0).abs() < 1e-6);
    }

    fn samples_from(mb: &[f64]) -> Samples {
        Samples {
            points: mb
                .iter()
                .enumerate()
                .map(|(i, &m)| (i as f64 * 500.0, m * 1_048_576.0))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn leak_ignores_warmup_then_plateau() {
        // RSS ramps for the first third (lazy-load / warmup) then plateaus — NOT a leak.
        let mut mb = Vec::new();
        for i in 0..30 {
            mb.push(if i < 10 {
                100.0 + 9.0 * i as f64
            } else {
                190.0
            });
        }
        let f = evaluate(&PerfReport::default(), &samples_from(&mb), "t", 0.0);
        assert!(
            !f.iter().any(|x| x.rule_id == "perf.memory_growth"),
            "warmup-then-plateau must not be flagged as a leak"
        );
    }

    #[test]
    fn leak_flags_sustained_growth() {
        // Climbs the whole run, still rising at the end — a leak.
        let mb: Vec<f64> = (0..30).map(|i| 100.0 + 4.0 * i as f64).collect();
        let f = evaluate(&PerfReport::default(), &samples_from(&mb), "t", 0.0);
        assert!(
            f.iter().any(|x| x.rule_id == "perf.memory_growth"),
            "sustained growth must be flagged"
        );
    }

    #[test]
    fn latency_tail_flags_ratio_not_uniform() {
        use voltiq_core::LatencyStats;
        let tail = PerfReport {
            latency: Some(LatencyStats {
                min: 1.0,
                mean: 10.0,
                p50: 5.0,
                p95: 50.0,
                p99: 120.0,
                max: 200.0,
            }),
            ..Default::default()
        };
        assert!(
            evaluate(&tail, &Samples::default(), "t", 0.0)
                .iter()
                .any(|x| x.rule_id == "perf.latency_tail"),
            "p99 = 24× p50 must flag"
        );

        // Uniformly-heavy endpoint (high but flat latency) must NOT flag.
        let uniform = PerfReport {
            latency: Some(LatencyStats {
                min: 400.0,
                mean: 450.0,
                p50: 450.0,
                p95: 480.0,
                p99: 495.0,
                max: 500.0,
            }),
            ..Default::default()
        };
        assert!(
            !evaluate(&uniform, &Samples::default(), "t", 0.0)
                .iter()
                .any(|x| x.rule_id == "perf.latency_tail"),
            "uniformly-heavy latency must not flag (ratio ≈ 1.1)"
        );
    }

    #[test]
    fn flags_uncleared_interval() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("worker.js"),
            "setInterval(() => poll(), 1000);\n",
        )
        .unwrap();
        let findings = scan_static_antipatterns(dir.path());
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "perf.uncleared_interval"));
    }
}
