//! Sample a process tree's RSS / CPU over time via `sysinfo` (cross-platform).
//!
//! We aggregate the target pid **and all its descendants**, because the spawned pid is
//! often a launcher: `npm`/`pnpm`/`yarn` run wrappers and version-manager shims exec
//! the real runtime as a child, and Node `cluster`/`worker_threads` fork more workers.
//! Summing the tree measures the whole app, not just the launcher.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessesToUpdate, System};

#[derive(Default, Clone)]
pub struct Samples {
    /// `(elapsed_ms, rss_bytes)` points for the memory trend series.
    pub points: Vec<(f64, f64)>,
    pub peak_rss_bytes: f64,
    pub peak_cpu_pct: f32,
}

/// All pids in the tree rooted at `root` (root + transitive children) that are alive.
pub fn process_tree(sys: &System, root: Pid) -> HashSet<Pid> {
    let mut set = HashSet::new();
    if sys.process(root).is_some() {
        set.insert(root);
    }
    // Fixed-point expansion over the parent links until no new pid is added.
    // Skip threads: on Linux sysinfo lists a process's threads as tasks whose parent is
    // the main pid, each reporting the *same* RSS — counting them would multiply memory.
    loop {
        let mut added = false;
        for (pid, proc) in sys.processes() {
            if proc.thread_kind().is_some() {
                continue;
            }
            if let Some(parent) = proc.parent() {
                if set.contains(&parent) && set.insert(*pid) {
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    set
}

/// Sum RSS (bytes) and CPU (%) across the process tree.
pub fn tree_usage(sys: &System, tree: &HashSet<Pid>) -> (f64, f32) {
    let mut rss = 0.0;
    let mut cpu = 0.0;
    for pid in tree {
        if let Some(proc) = sys.process(*pid) {
            rss += proc.memory() as f64;
            cpu += proc.cpu_usage();
        }
    }
    (rss, cpu)
}

/// Sample the tree rooted at `pid` every `interval` until `stop` is set or it exits.
pub fn sample_until(pid: u32, stop: Arc<AtomicBool>, interval: Duration) -> Samples {
    let mut sys = System::new();
    let root = Pid::from_u32(pid);
    let start = Instant::now();
    let mut s = Samples::default();
    while !stop.load(Ordering::Relaxed) {
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let tree = process_tree(&sys, root);
        if tree.is_empty() {
            break; // whole tree gone
        }
        let (rss, cpu) = tree_usage(&sys, &tree);
        if s.points.is_empty() && std::env::var("VOLTIQ_DEBUG").is_ok() {
            eprintln!("[voltiq debug] tree size={} root={root}", tree.len());
            for pid in &tree {
                if let Some(pr) = sys.process(*pid) {
                    eprintln!(
                        "[voltiq debug]   pid={pid} name={:?} parent={:?} rss_mb={:.1}",
                        pr.name(),
                        pr.parent(),
                        pr.memory() as f64 / 1_048_576.0
                    );
                }
            }
        }
        let t = start.elapsed().as_secs_f64() * 1000.0;
        s.points.push((t, rss));
        if rss > s.peak_rss_bytes {
            s.peak_rss_bytes = rss;
        }
        if cpu > s.peak_cpu_pct {
            s.peak_cpu_pct = cpu;
        }
        std::thread::sleep(interval);
    }
    s
}

/// One-shot RSS read for a process tree (bytes), or None if the root isn't found.
pub fn rss_of(pid: u32) -> Option<f64> {
    let mut sys = System::new();
    let root = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let tree = process_tree(&sys, root);
    if tree.is_empty() {
        return None;
    }
    Some(tree_usage(&sys, &tree).0)
}
