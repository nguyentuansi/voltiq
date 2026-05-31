//! `watch` — zero-config, port-based, continuous passive monitoring.
//!
//! Resolve a port to its process, then sample memory/CPU while the user drives the app,
//! printing a live status line until Ctrl-C (or an optional max duration). Re-attaches
//! across dev-server restarts. On stop, returns a perf report + leak verdict.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};
use voltiq_core::{Finding, PerfReport};

use crate::osmetrics::{process_tree, tree_usage, Samples};
use crate::resolve_port;

pub fn watch_port(
    port: u16,
    stop: Arc<AtomicBool>,
    max: Option<Duration>,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let mut pid = resolve_port(port)
        .ok_or_else(|| format!("nothing is listening on port {port} (is the app running?)"))?;

    let mut sys = System::new();
    let start = Instant::now();
    let interval = Duration::from_millis(500);
    let mut samples = Samples::default();
    let mut baseline = 0.0_f64;

    eprintln!("watching :{port} (pid {pid}) — interact with your app; press Ctrl-C to stop.");

    while !stop.load(Ordering::Relaxed) {
        if let Some(m) = max {
            if start.elapsed() >= m {
                break;
            }
        }
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let tree = process_tree(&sys, Pid::from_u32(pid));
        if tree.is_empty() {
            // Dev servers restart on file change; try to re-attach by port.
            match resolve_port(port) {
                Some(np) => {
                    pid = np;
                    eprint!("\r  :{port} restarted → pid {pid}                                          \n");
                    continue;
                }
                None => {
                    eprintln!("\n  :{port} process exited.");
                    break;
                }
            }
        }
        let (rss, cpu) = tree_usage(&sys, &tree);
        if baseline == 0.0 {
            baseline = rss;
        }
        let t = start.elapsed().as_secs_f64() * 1000.0;
        samples.points.push((t, rss));
        if rss > samples.peak_rss_bytes {
            samples.peak_rss_bytes = rss;
        }
        if cpu > samples.peak_cpu_pct {
            samples.peak_cpu_pct = cpu;
        }
        eprint!(
            "\r  :{port} pid {pid}  {:6.1}s   rss {:8.1} MB (Δ {:+7.1})   cpu {:4.0}%   peak {:8.1} MB   ",
            start.elapsed().as_secs_f64(),
            rss / 1_048_576.0,
            (rss - baseline) / 1_048_576.0,
            cpu,
            samples.peak_rss_bytes / 1_048_576.0
        );
        let _ = std::io::stderr().flush();
        std::thread::sleep(interval);
    }
    eprintln!();

    let mut report = PerfReport::default();
    crate::populate_samples(&mut report, &samples);
    // The app is already warm when we attach, so use a short warmup.
    let findings =
        crate::analysis::evaluate(&report, &samples, &format!(":{port} (pid {pid})"), 500.0);
    Ok((report, findings))
}
