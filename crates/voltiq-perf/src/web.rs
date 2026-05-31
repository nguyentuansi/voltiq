//! Browser / "Chromium-like" web performance via Lighthouse.
//!
//! Server-process RSS can't tell you page-load or Core Web Vitals (LCP/CLS/INP/FCP) —
//! those are defined by the rendering engine. So we drive headless Chrome through
//! Lighthouse (auto-fetched by `npx`) and map its audits into a voltiq report, so web
//! findings flow through the same JSON / SARIF / dashboard pipeline. Requires Node and a
//! Chrome/Chromium browser on the machine.

use std::process::Command;

use serde_json::Value;
use voltiq_core::{
    Confidence, Domain, Finding, Location, Metric, MetricStatus, PerfReport, Severity, Surface,
};

const CHROME_CANDIDATES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "chrome",
];

pub(crate) fn find_chrome() -> Option<String> {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        if !p.is_empty() {
            return Some(p);
        }
    }
    for c in CHROME_CANDIDATES {
        if let Ok(out) = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {c}"))
            .output()
        {
            if out.status.success() {
                let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !p.is_empty() {
                    return Some(p);
                }
            }
        }
    }
    // Puppeteer / Playwright bundled Chromium — common on dev machines even when no
    // Chrome is on PATH. Pick the newest version directory.
    if let Ok(home) = std::env::var("HOME") {
        let bases: &[(String, &[&str])] = &[
            (
                format!("{home}/.cache/puppeteer/chrome"),
                &["chrome-linux64/chrome", "chrome-linux/chrome"],
            ),
            (
                format!("{home}/.cache/ms-playwright"),
                &["chrome-linux/chrome"],
            ),
        ];
        for (base, rels) in bases {
            let Ok(entries) = std::fs::read_dir(base) else {
                continue;
            };
            let mut dirs: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            dirs.sort(); // ascending; iterate newest-first below
            for dir in dirs.into_iter().rev() {
                for rel in *rels {
                    let cand = dir.join(rel);
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    None
}

/// Run a Lighthouse performance audit of `url` and map it to a report.
///
/// `desktop` uses Lighthouse's desktop preset (no mobile emulation, far lighter
/// throttling) — numbers much closer to what you see in DevTools on your own machine.
/// The default (mobile, throttled) models a mid-tier phone — the industry perf target.
pub fn web_perf(url: &str, desktop: bool) -> Result<(PerfReport, Vec<Finding>), String> {
    let chrome = find_chrome();
    let mut cmd = Command::new("npx");
    cmd.args([
        "-y",
        "lighthouse",
        url,
        "--quiet",
        "--output=json",
        "--only-categories=performance",
        "--chrome-flags=--headless=new --no-sandbox --disable-gpu",
    ]);
    if desktop {
        cmd.arg("--preset=desktop");
    }
    if let Some(c) = &chrome {
        cmd.env("CHROME_PATH", c);
    }

    let out = cmd.output().map_err(|e| {
        format!("could not run lighthouse via npx ({e}). Install Node.js and a Chrome/Chromium browser, then retry.")
    })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(3).collect();
        return Err(format!(
            "lighthouse failed (need Node + Chrome installed): {}",
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        ));
    }

    let json: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("could not parse lighthouse output: {e}"))?;
    Ok(build_report(url, &json, desktop))
}

fn fmt_val(v: f64, unit: &str) -> String {
    match unit {
        "ms" => format!("{v:.0} ms"),
        "" => format!("{v:.3}"),
        u => format!("{v:.0} {u}"),
    }
}

fn remediation_for(key: &str) -> &'static str {
    match key {
        "largest-contentful-paint" => "Optimize the LCP element: preload the hero image/font, cut render-blocking CSS/JS, use a CDN.",
        "total-blocking-time" => "Break up long tasks, defer/code-split heavy JS, move work off the main thread.",
        "cumulative-layout-shift" => "Set explicit width/height on images/embeds and reserve space for dynamic content.",
        "first-contentful-paint" => "Reduce server response time and render-blocking resources.",
        "speed-index" => "Reduce above-the-fold work and render-blocking resources.",
        "interactive" => "Reduce main-thread work and JS execution time before interactivity.",
        _ => "See the Lighthouse report for guidance.",
    }
}

fn build_report(url: &str, lh: &Value, desktop: bool) -> (PerfReport, Vec<Finding>) {
    let audits = &lh["audits"];
    let num = |k: &str| audits[k]["numericValue"].as_f64();

    let mut report = PerfReport {
        runtime: Some(format!(
            "chromium (lighthouse, {})",
            if desktop { "desktop" } else { "mobile" }
        )),
        ..Default::default()
    };
    let mut findings = Vec::new();

    // (audit key, label, unit, "good" threshold, "poor" threshold)
    let vitals: &[(&str, &str, &str, f64, f64)] = &[
        ("first-contentful-paint", "FCP", "ms", 1800.0, 3000.0),
        ("largest-contentful-paint", "LCP", "ms", 2500.0, 4000.0),
        ("total-blocking-time", "TBT", "ms", 200.0, 600.0),
        ("cumulative-layout-shift", "CLS", "", 0.1, 0.25),
        ("speed-index", "SpeedIndex", "ms", 3400.0, 5800.0),
        ("interactive", "TTI", "ms", 3800.0, 7300.0),
    ];

    for &(key, label, unit, good, poor) in vitals {
        let Some(v) = num(key) else { continue };
        let status = if v <= good {
            MetricStatus::Pass
        } else if v <= poor {
            MetricStatus::Warn
        } else {
            MetricStatus::Fail
        };
        report
            .metrics
            .push(Metric::new(label, v, unit, status).with_threshold(good));
        if status != MetricStatus::Pass {
            let sev = if v > poor {
                Severity::High
            } else {
                Severity::Medium
            };
            findings.push(
                Finding::new(
                    Domain::Performance,
                    format!("web.{}", key.replace('-', "_")),
                    format!("{label} is {} (good ≤ {})", fmt_val(v, unit), fmt_val(good, unit)),
                    sev,
                    Confidence::High,
                    Surface::Runtime,
                    format!("{label} exceeds the 'good' Core Web Vitals threshold in a headless Chrome run."),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation(remediation_for(key)),
            );
        }
    }

    if let Some(score) = lh["categories"]["performance"]["score"].as_f64() {
        let status = if score >= 0.9 {
            MetricStatus::Pass
        } else if score >= 0.5 {
            MetricStatus::Warn
        } else {
            MetricStatus::Fail
        };
        report
            .metrics
            .push(Metric::new("Lighthouse", score * 100.0, "/100", status));
    }

    (report, findings)
}
