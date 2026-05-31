//! Resolve a listening TCP port to the owning process id, so the user can just say
//! `voltiq watch 8786` instead of hunting for a pid.
//!
//! Linux: parse `/proc/net/tcp{,6}` for a LISTEN socket on the port, then find which
//! `/proc/<pid>/fd/*` points at that socket inode — no external tools needed.
//! Other platforms (and as a fallback): shell out to `lsof`.

/// Find the pid listening on `port`, if any.
pub fn resolve_port(port: u16) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        if let Some(pid) = resolve_linux(port) {
            return Some(pid);
        }
    }
    resolve_lsof(port)
}

fn resolve_lsof(port: u16) -> Option<u32> {
    let out = std::process::Command::new("lsof")
        .args(["-tiTCP", &format!(":{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().parse::<u32>().ok())
}

#[cfg(target_os = "linux")]
fn resolve_linux(port: u16) -> Option<u32> {
    use std::collections::HashSet;

    // Collect inodes of LISTEN sockets bound to `port` (state 0A = TCP_LISTEN).
    let mut inodes: HashSet<String> = HashSet::new();
    for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(content) = std::fs::read_to_string(table) else {
            continue;
        };
        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // cols: sl local_address rem_address st tx:rx tr:when retr uid timeout inode ...
            if cols.len() < 10 || cols[3] != "0A" {
                continue;
            }
            let Some((_, hex_port)) = cols[1].split_once(':') else {
                continue;
            };
            if u16::from_str_radix(hex_port, 16).ok() == Some(port) {
                inodes.insert(cols[9].to_string());
            }
        }
    }
    if inodes.is_empty() {
        return None;
    }

    // Find the pid whose fds include one of those socket inodes.
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return None;
    };
    for entry in procs.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let fd_dir = entry.path().join("fd");
        let Ok(fds) = std::fs::read_dir(&fd_dir) else {
            continue; // not ours / gone
        };
        for fd in fds.flatten() {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                let t = target.to_string_lossy();
                if let Some(inode) = t.strip_prefix("socket:[").and_then(|s| s.strip_suffix(']')) {
                    if inodes.contains(inode) {
                        return Some(pid);
                    }
                }
            }
        }
    }
    None
}
