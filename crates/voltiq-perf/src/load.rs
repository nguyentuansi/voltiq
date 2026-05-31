//! A small, dependency-free HTTP/1.0 load generator for localhost benchmarking.
//!
//! HTTP/1.0 + `Connection: close` means the server closes after each response, so we
//! can read to EOF without parsing keep-alive framing. Good enough for measuring
//! startup, throughput, and latency percentiles against a local dev server.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct LoadResult {
    pub attempts: u64,
    /// No HTTP response at all (connect/read/write failed, or malformed) — a real failure.
    pub transport_errors: u64,
    /// Got a 4xx — usually a wrong URL / missing auth, NOT a load failure.
    pub http_4xx: u64,
    /// Got a 5xx — the server failed under load.
    pub http_5xx: u64,
    /// Latencies (ms) of requests that got a response, ascending-sortable.
    pub latencies_ms: Vec<f64>,
    pub wall_secs: f64,
}

struct Target {
    host: String,
    port: u16,
    path: String,
}

fn parse_http_url(url: &str) -> Option<Target> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().ok()?),
        None => (authority.to_string(), 80u16),
    };
    Some(Target {
        host,
        port,
        path: path.to_string(),
    })
}

/// Perform one request; returns (latency_ms, http_status). `Err(())` means no response at
/// all (transport failure). A `status` of 0 means a response came but the status line
/// couldn't be parsed (treated as a transport-level failure by callers).
fn one_request(t: &Target, timeout: Duration) -> Result<(f64, u16), ()> {
    let start = Instant::now();
    let mut stream = TcpStream::connect((t.host.as_str(), t.port)).map_err(|_| ())?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    let req = format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nConnection: close\r\nUser-Agent: voltiq\r\nAccept: */*\r\n\r\n",
        t.path, t.host
    );
    stream.write_all(req.as_bytes()).map_err(|_| ())?;

    let mut head = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut status: u16 = 0;
    let mut got_status = false;
    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if !got_status {
                    head.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = head.iter().position(|&b| b == b'\n') {
                        let line = String::from_utf8_lossy(&head[..pos]);
                        status = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|code| code.parse::<u16>().ok())
                            .unwrap_or(0);
                        got_status = true;
                    }
                }
            }
            Err(_) => break,
        }
    }
    Ok((start.elapsed().as_secs_f64() * 1000.0, status))
}

/// Poll `url` until it returns a response; returns time-to-first-response (ms).
pub fn wait_ready(url: &str, timeout: Duration) -> Option<f64> {
    let target = parse_http_url(url)?;
    let start = Instant::now();
    while start.elapsed() < timeout {
        // Ready = the server returns ANY HTTP status (even 401/404 means it's up and
        // serving). Requiring 2xx would never see a server whose "/" needs auth.
        if let Ok((_, status)) = one_request(&target, Duration::from_secs(2)) {
            if status > 0 {
                return Some(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

/// Drive `concurrency` workers against `url` for `duration`.
pub fn run_load(url: &str, concurrency: usize, duration: Duration) -> Option<LoadResult> {
    let target = Arc::new(parse_http_url(url)?);
    let deadline = Instant::now() + duration;
    let start = Instant::now();
    let mut handles = Vec::new();
    for _ in 0..concurrency.max(1) {
        let t = target.clone();
        handles.push(std::thread::spawn(move || {
            let mut lat = Vec::new();
            let (mut attempts, mut transport, mut c4, mut c5) = (0u64, 0u64, 0u64, 0u64);
            while Instant::now() < deadline {
                attempts += 1;
                match one_request(&t, Duration::from_secs(5)) {
                    Ok((ms, status)) => {
                        lat.push(ms);
                        match status {
                            0 => transport += 1, // response, but unparseable
                            400..=499 => c4 += 1,
                            500..=599 => c5 += 1,
                            _ => {} // 2xx / 3xx — success
                        }
                    }
                    Err(_) => transport += 1, // no response at all
                }
            }
            (lat, attempts, transport, c4, c5)
        }));
    }
    let (mut latencies, mut attempts, mut transport_errors, mut http_4xx, mut http_5xx) =
        (Vec::new(), 0u64, 0u64, 0u64, 0u64);
    for h in handles {
        if let Ok((lat, a, t, c4, c5)) = h.join() {
            latencies.extend(lat);
            attempts += a;
            transport_errors += t;
            http_4xx += c4;
            http_5xx += c5;
        }
    }
    Some(LoadResult {
        attempts,
        transport_errors,
        http_4xx,
        http_5xx,
        latencies_ms: latencies,
        wall_secs: start.elapsed().as_secs_f64(),
    })
}

/// Percentile of an ascending-sorted slice (nearest-rank).
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0 * (sorted.len() - 1) as f64).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn percentiles_basic() {
        let v: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        // Nearest-rank with (len-1) scaling: p50 of 1..=100 lands at index 50 -> 51.
        assert!((50.0..=51.0).contains(&percentile(&v, 50.0)));
        assert_eq!(percentile(&v, 99.0), 99.0);
        assert_eq!(percentile(&v, 100.0), 100.0);
        assert_eq!(percentile(&[], 50.0), 0.0);
    }

    #[test]
    fn load_against_local_listener() {
        // Tiny HTTP/1.0 server that 200s everything, on an ephemeral port.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let server = std::thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut s, _)) => {
                        let mut buf = [0u8; 1024];
                        let _ = s.read(&mut buf);
                        let _ = s.write_all(b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nok");
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        });

        let url = format!("http://127.0.0.1:{port}/");
        assert!(wait_ready(&url, Duration::from_secs(3)).is_some());
        let res = run_load(&url, 4, Duration::from_millis(600)).unwrap();
        assert!(res.attempts > 0);
        assert!(!res.latencies_ms.is_empty());
        assert_eq!(
            res.transport_errors + res.http_4xx + res.http_5xx,
            0,
            "local listener should 200 everything"
        );

        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = server.join();
    }
}
