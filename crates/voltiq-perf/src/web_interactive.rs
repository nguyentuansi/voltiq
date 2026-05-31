//! Interactive web-vitals capture: open a real (headed) Chrome at the URL, inject a
//! web-vitals collector *before* navigation, let the user drive the page, and read
//! LCP / CLS / INP / FCP from their session live until Ctrl-C or the window closes.
//!
//! Uses the Chrome DevTools Protocol over a websocket (no npm packages) against an
//! installed Chrome (incl. Puppeteer/Playwright caches). Set `VOLTIQ_WEB_HEADLESS=1`
//! to run without a window (CI / no display).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;
use voltiq_core::{Finding, PerfReport};

use crate::web::find_chrome;

/// Collector injected before navigation; accumulates web-vitals onto `window.__voltiq`.
const COLLECTOR: &str = r#"
(() => {
  if (window.__voltiq) return;
  const v = window.__voltiq = { fcp:null, lcp:null, cls:0, inp:null, interactions:0 };
  const r1 = x => Math.round(x*10)/10;
  // A short, human selector for a DOM node (tag#id.class), for LCP/INP/CLS culprits.
  const desc = (el) => { try { if(!el||!el.tagName) return null; let s=el.tagName.toLowerCase();
      if(el.id) s+='#'+el.id; else if(typeof el.className==='string'&&el.className.trim()) s+='.'+el.className.trim().split(/\s+/)[0];
      return s.slice(0,60); } catch(e){ return null; } };
  const obs = (type, cb, extra) => { try { new PerformanceObserver(cb).observe(Object.assign({type, buffered:true}, extra||{})); } catch(e){} };
  obs('paint', l => { for (const e of l.getEntries()) if (e.name==='first-contentful-paint') v.fcp = e.startTime; });
  obs('largest-contentful-paint', l => { const es=l.getEntries(); const last=es[es.length-1];
      if (last){ v.lcp = last.renderTime || last.startTime; v.lcp_url = last.url||null; v.lcp_el = desc(last.element); } });
  (() => { let cls=0, sv=0, se=[], worst=0; obs('layout-shift', l => { for (const e of l.getEntries()) {
      if (e.hadRecentInput) continue;
      const first=se[0], last=se[se.length-1];
      if (se.length && e.startTime - last.startTime < 1000 && e.startTime - first.startTime < 5000) { sv += e.value; se.push(e); }
      else { sv = e.value; se = [e]; }
      if (sv > cls) { cls = sv; v.cls = cls; }
      if (e.value > worst) { worst = e.value; const src=(e.sources||[])[0]; v.cls_el = src ? desc(src.node) : null; }
    } }); })();
  (() => { let worst=0; obs('event', l => { for (const e of l.getEntries()) {
      if (e.interactionId) { v.interactions++; if (e.duration > worst) { worst = e.duration; v.inp = worst;
        v.inp_type = e.name; v.inp_target = desc(e.target);
        v.inp_input = Math.max(0, r1(e.processingStart - e.startTime));
        v.inp_proc = Math.max(0, r1(e.processingEnd - e.processingStart));
        v.inp_present = Math.max(0, r1(e.startTime + e.duration - e.processingEnd));
      } }
    } }, { durationThreshold: 16 }); })();
  (() => { let n=0, t=0; obs('longtask', l => { for (const e of l.getEntries()) { n++; t += e.duration; } v.longtask_count = n; v.longtask_ms = Math.round(t); }); })();
  // Live DOM snapshot, recomputed each poll (the DOM doesn't exist at inject time):
  // render-blocking head resources, and image natural-vs-displayed size for oversizing.
  window.__voltiq_dom = function(){
    var r = {};
    try { r.blocking = [].slice.call(document.head.querySelectorAll('link[rel=stylesheet][href],script[src]:not([async]):not([defer]):not([type=module])')).map(function(e){return e.href||e.src;}).filter(Boolean).slice(0,40); } catch(e){}
    try { r.images = [].slice.call(document.images).filter(function(i){return i.currentSrc&&i.naturalWidth;}).map(function(i){var b=i.getBoundingClientRect();return {src:i.currentSrc,nw:i.naturalWidth,nh:i.naturalHeight,dw:Math.round(b.width),dh:Math.round(b.height)};}).slice(0,60); } catch(e){}
    return r;
  };
})();
"#;

struct TmpDir(PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Ask the OS for a free localhost port, then hand it to Chrome via a fixed
/// `--remote-debugging-port` (a fixed port works with `flatpak run`, where we can't
/// read DevToolsActivePort, and avoids the relaunch/port-rebind race).
fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()
        .map(|a| a.port())
}

/// Flatpak app ids for Chromium-based browsers that can be driven via CDP.
const FLATPAK_CHROMIUM_IDS: &[&str] = &[
    "com.google.Chrome",
    "org.chromium.Chromium",
    "io.github.ungoogled_software.ungoogled_chromium",
    "com.github.Eloston.ungoogled_chromium",
    "com.microsoft.Edge",
    "com.brave.Browser",
];

fn which_bin(c: &str) -> Option<String> {
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {c}"))
        .output()
        .ok()?;
    if out.status.success() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() {
            return Some(p);
        }
    }
    None
}

fn flatpak_has(id: &str) -> bool {
    Command::new("flatpak")
        .args(["info", id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// The argv prefix used to launch a Chromium for CDP: either a binary path, or
/// `["flatpak", "run", "<id>"]`. Prefers a real installed Chrome, then a flatpak
/// Chromium (works on sandboxed setups), then a bundled Puppeteer/Playwright Chromium.
fn find_chrome_launcher() -> Option<Vec<String>> {
    if let Ok(p) = std::env::var("CHROME_PATH") {
        if !p.is_empty() {
            return Some(vec![p]);
        }
    }
    for c in [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "chrome",
    ] {
        if let Some(p) = which_bin(c) {
            return Some(vec![p]);
        }
    }
    for id in FLATPAK_CHROMIUM_IDS {
        if flatpak_has(id) {
            return Some(vec!["flatpak".into(), "run".into(), (*id).into()]);
        }
    }
    // Bundled Puppeteer/Playwright Chromium (last — flaky when spawned for headed CDP).
    find_chrome().map(|p| vec![p])
}

/// Blocking entry point: open Chrome, capture until `stop` / window close, return a report.
pub fn web_interactive(
    url: &str,
    stop: Arc<AtomicBool>,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_launch(url, stop, prod))
}

/// Connect to a browser already running with `--remote-debugging-port=<port>` and
/// capture vitals from your interaction live, without launching or closing anything.
pub fn web_connect(
    port: u16,
    url: &str,
    stop: Arc<AtomicBool>,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_connect(port, url, stop, prod))
}

/// Headless lab capture: launch a throwaway headless Chrome, load the URL once, and
/// auto-stop when the network goes idle (no interaction). For CI / automated agents.
/// `stop` still acts as a hard cap if the page never settles (e.g. a dev server's HMR).
pub fn web_lab(
    url: &str,
    stop: Arc<AtomicBool>,
    throttle: bool,
    runs: usize,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let runs = runs.max(1);
    // Safety cap: lab mode normally stops on network-idle, but an app whose network never
    // settles (a chatty dev server) must not hang. Whichever fires first wins. Throttling
    // and multi-run both take longer, so scale the cap.
    {
        let s = stop.clone();
        let cap = (if throttle { 120 } else { 45 }) * runs as u64;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(cap));
            s.store(true, Ordering::Relaxed);
        });
    }
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_lab(url, stop, throttle, runs, prod))
}

fn launch_chrome(
    launcher: &[String],
    port: u16,
    headless: bool,
) -> Result<(Child, TmpDir), String> {
    let dir = std::env::temp_dir().join(format!("voltiq-cdp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let is_flatpak = launcher.first().map(|s| s == "flatpak").unwrap_or(false);

    // Switches must precede the positional URL. Use a fixed debug port.
    let mut cmd = Command::new(&launcher[0]);
    cmd.args(&launcher[1..]);
    cmd.arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", dir.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if !is_flatpak {
        // Bundled/binary Chromium can't start its sandbox on AppArmor-restricted distros
        // and relaunches itself as a new process without --no-zygote; flatpak Chromium
        // manages its own sandbox/process model, so don't pass these there.
        cmd.arg("--no-sandbox").arg("--no-zygote");
    }
    if headless || std::env::var("VOLTIQ_WEB_HEADLESS").is_ok() {
        cmd.arg("--headless=new").arg("--disable-gpu");
    }
    // Launch at about:blank (a real URL on the cmdline can be forwarded to an
    // already-running browser, leaving our debug instance with no page); we navigate to
    // the target over CDP instead.
    let log_path = dir.join("chrome-stderr.log");
    cmd.arg("about:blank")
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    match std::fs::File::create(&log_path) {
        Ok(f) => {
            cmd.stderr(Stdio::from(f));
        }
        Err(_) => {
            cmd.stderr(Stdio::null());
        }
    }
    // A bundled/binary Chrome must run in its own process group (like a shell `&`) or its
    // relaunch logic exits immediately; flatpak already isolates the app.
    #[cfg(unix)]
    {
        if !is_flatpak {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("failed to launch Chrome ({}): {e}", launcher.join(" ")))?;

    // Wait until the fixed debug port is actually serving (~25s; flatpak cold start is slow).
    let tmp = TmpDir(dir);
    for _ in 0..500 {
        if http_get(port, "/json/version").is_ok() {
            return Ok((child, tmp));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let tail = std::fs::read_to_string(&log_path)
        .ok()
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .iter()
                .rev()
                .take(6)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n    ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "(no output captured)".into());
    Err(format!(
        "Chrome did not expose a working debugging port on :{port} within 25s.\n  launcher: {}\n  chrome said:\n    {tail}",
        launcher.join(" ")
    ))
}

/// Blocking GET to the CDP HTTP endpoint. Reads the body by `Content-Length` (Chrome's
/// DevTools server keeps the connection alive and ignores `Connection: close`, so we
/// can't wait for EOF). Requires HTTP/1.1 + a `Host` with the port (DNS-rebinding guard).
fn http_get(port: u16, path: &str) -> Result<String, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok();
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;

    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let mut header_end: Option<usize> = None;
    let mut content_len: Option<usize> = None;
    loop {
        if header_end.is_none() {
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(pos + 4);
                let head = String::from_utf8_lossy(&buf[..pos]);
                content_len = head.lines().find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                });
            }
        }
        if let (Some(he), Some(cl)) = (header_end, content_len) {
            if buf.len() >= he + cl {
                return Ok(String::from_utf8_lossy(&buf[he..he + cl]).to_string());
            }
        }
        match s.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) => return Err(e.to_string()),
        }
    }
    match header_end {
        Some(he) => Ok(String::from_utf8_lossy(&buf[he..]).to_string()),
        None => Err("incomplete HTTP response".into()),
    }
}

/// Single-shot: the websocket URL of the first `page` target, if available now.
fn page_ws_url(port: u16) -> Option<String> {
    let body = http_get(port, "/json/list").ok()?;
    let targets: Value = serde_json::from_str(&body).ok()?;
    targets
        .as_array()?
        .iter()
        .find(|t| t["type"] == "page")
        .and_then(|t| t["webSocketDebuggerUrl"].as_str())
        .map(|s| s.to_string())
}

type Ws = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

/// One network request observed during the session (times are ms since session start).
#[derive(Default, Clone)]
struct Req {
    url: String,
    method: String,
    status: i64,
    kind: String, // category: api, script, css, doc, img, font, ws, media, other
    start_ms: f64,
    end_ms: f64,
    failed: bool,
    bytes: f64,            // encoded transfer size (loadingFinished.encodedDataLength)
    encoding: String,      // content-encoding header ("" = none → uncompressed)
    cache_control: String, // cache-control header ("" = none)
    has_validator: bool,   // has ETag / Last-Modified → can revalidate (304) even if max-age=0
    from_cache: bool,
    ttfb_ms: f64,      // server wait (response.timing: receiveHeadersStart − sendEnd)
    initiator: String, // what requested it: a URL, or "parser"/"script"/"other"
}
impl Req {
    fn dur_ms(&self) -> f64 {
        (self.end_ms - self.start_ms).max(0.0)
    }
}

/// Case-insensitive lookup of a header value from a CDP response `headers` object.
fn header_ci(headers: &Value, name: &str) -> String {
    headers
        .as_object()
        .and_then(|o| {
            o.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .and_then(|(_, val)| val.as_str())
        })
        .unwrap_or("")
        .to_string()
}

/// The host[:port] of a URL (empty for data:/blob:/relative).
fn host_of(u: &str) -> String {
    u.split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Human-readable byte size (B / KB / MB).
fn human_bytes(b: f64) -> String {
    if b >= 1_048_576.0 {
        format!("{:.1} MB", b / 1_048_576.0)
    } else if b >= 1024.0 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{b:.0} B")
    }
}

/// Map a CDP resource type to a short, friendly category.
fn req_kind(cdp_type: &str) -> &'static str {
    match cdp_type {
        "XHR" | "Fetch" | "EventSource" => "api",
        "Script" => "script",
        "Stylesheet" => "css",
        "Document" => "doc",
        "Image" => "img",
        "Font" => "font",
        "Media" => "media",
        "WebSocket" => "ws",
        "Manifest" => "manifest",
        "" => "other",
        _ => "other",
    }
}

/// One navigation segment: its URL, when it started/finished loading (ms since session
/// start), the requests it fired, and its latest web-vitals snapshot.
#[derive(Default, Clone)]
struct Nav {
    url: String,
    started_ms: f64,
    loaded_ms: f64,
    reqs: std::collections::HashMap<String, Req>,
    vitals: Value,
}
impl Nav {
    /// How long this navigation was active (load event, else last request finish).
    fn span_ms(&self) -> f64 {
        let last_req = self.reqs.values().map(|r| r.end_ms).fold(0.0_f64, f64::max);
        self.loaded_ms.max(last_req) - self.started_ms
    }
}

/// Everything a capture session yields: the navigation timeline plus the latest and the
/// FIRST snapshot of CDP `Performance.getMetrics`. The durations are cumulative since the
/// renderer started, so we keep the first reading as a baseline and report `latest - base`
/// — important for `--connect`, where the tab was already running before we attached.
#[derive(Default)]
struct Capture {
    navs: Vec<Nav>,
    mainthread: std::collections::HashMap<String, f64>,
    mainthread_base: std::collections::HashMap<String, f64>,
    /// Unused JavaScript by script URL → (total_bytes, unused_bytes), from precise coverage.
    js_coverage: Vec<(String, f64, f64)>,
    /// Main-thread self-time (ms) by script URL, from the sampling CPU profile.
    cpu_by_script: Vec<(String, f64)>,
    /// True if the run was measured under network + CPU throttling.
    throttled: bool,
    /// True if the user asserted this URL is a PRODUCTION build (`--prod`) — flips the
    /// report's framing from "dev, re-measure prod" to a real-user verdict, and stops
    /// downgrading findings as "expected dev behavior".
    prod: bool,
}

/// Event-driven capture: subscribe to Page + Network, segment by navigation, and poll
/// web-vitals — so we can show *what* made a navigation slow (its request waterfall),
/// not just the final page's numbers. Returns the navigation timeline.
async fn capture_session(
    ws: &mut Ws,
    navigate_to: Option<&str>,
    label: &str,
    stop: Arc<AtomicBool>,
    auto_idle: Option<Duration>,
    throttle: bool,
) -> Capture {
    let mut next_id = 0u64;
    let mut send = |method: &str, params: Value| -> (u64, String) {
        next_id += 1;
        (
            next_id,
            json!({"id": next_id, "method": method, "params": params}).to_string(),
        )
    };

    let mut setup = vec![
        ("Page.enable", json!({})),
        ("Network.enable", json!({})),
        // Measure real transfer: a warm reload would be served from cache (encodedDataLength
        // 0, 304s), hiding true bytes / compression. We restore caching when done.
        ("Network.setCacheDisabled", json!({ "cacheDisabled": true })),
        ("Runtime.enable", json!({})),
        ("Performance.enable", json!({})),
        // Deep capture: precise JS coverage (unused code) + a sampling CPU profile
        // (main-thread time by script). Drained after the loop.
        ("Profiler.enable", json!({})),
        ("Profiler.setSamplingInterval", json!({ "interval": 250 })),
        (
            "Profiler.startPreciseCoverage",
            json!({ "callCount": false, "detailed": true }),
        ),
        ("Profiler.start", json!({})),
        (
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": COLLECTOR }),
        ),
        ("Runtime.evaluate", json!({ "expression": COLLECTOR })),
    ];
    // Throttle to a mid-tier mobile + slow-4G profile so numbers predict the field, not a
    // native-speed localhost run. (Lab only — you can't fairly throttle a human clicking.)
    if throttle {
        setup.push((
            "Network.emulateNetworkConditions",
            json!({
                "offline": false,
                "latency": 150,
                "downloadThroughput": 1_638_400 / 8, // ~1.6 Mbps (slow 4G)
                "uploadThroughput": 768_000 / 8,
            }),
        ));
        setup.push(("Emulation.setCPUThrottlingRate", json!({ "rate": 4 })));
    }
    if let Some(url) = navigate_to {
        setup.push(("Page.navigate", json!({ "url": url })));
    }
    for (method, params) in setup {
        let (_id, payload) = send(method, params);
        if ws.send(Message::text(payload)).await.is_err() {
            return Capture::default();
        }
    }

    if auto_idle.is_some() {
        eprintln!(
            "measuring {label} (headless lab — loads once, auto-stops when the network goes idle)…"
        );
    } else {
        eprintln!("watching {label} — interact with the page; Ctrl-C to finish.");
        eprintln!(
            "streaming each captured event below (time · type · status · duration · request);\n\
             the animated line at the bottom shows live totals (spins while requests load)."
        );
    }

    let start = Instant::now();
    let mut navs: Vec<Nav> = vec![Nav {
        url: navigate_to.unwrap_or(label).to_string(),
        ..Default::default()
    }];
    let mut last_poll = Instant::now() - Duration::from_secs(2);
    let mut vitals_id = 0u64;
    let mut perf_id = 0u64;
    let mut mainthread: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut mainthread_base: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut last_status = String::new();
    let mut frame = 0usize;
    let mut last_longtask = 0i64;
    let mut last_sec = u64::MAX;
    let mut last_net_activity = Instant::now();
    // Deep-capture (unused JS + main-thread-by-script) snapshotted periodically so it
    // survives the user closing the window — which kills the socket before the final drain
    // can run. Without this, the default (interactive) mode silently loses both findings.
    let mut last_coverage: Vec<(String, f64, f64)> = Vec::new();
    let mut cpu_accum: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut last_deep = Instant::now();
    let mut deep_cov_id = 0u64;
    let mut deep_cpu_id = 0u64;

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if last_poll.elapsed() > Duration::from_millis(1000) {
            let (id, payload) = send(
                "Runtime.evaluate",
                json!({"expression": "JSON.stringify(Object.assign({},window.__voltiq||{},window.__voltiq_dom?window.__voltiq_dom():{}))", "returnByValue": true}),
            );
            vitals_id = id;
            if ws.send(Message::text(payload)).await.is_err() {
                break;
            }
            // CDP main-thread breakdown (cumulative ScriptDuration / LayoutDuration / …).
            let (pid, ppayload) = send("Performance.getMetrics", json!({}));
            perf_id = pid;
            if ws.send(Message::text(ppayload)).await.is_err() {
                break;
            }
            last_poll = Instant::now();
        }

        // Interactive only: the session ends when the user CLOSES the window, which kills
        // the socket before the post-loop drain can run. Snapshot deep-capture data while
        // the connection is alive — coverage via the non-resetting best-effort read
        // (cumulative); the CPU profile stopped+restarted and its per-script self-time
        // summed across intervals. (Lab ends on network-idle with the page still alive, so
        // its single post-loop drain already works — skip the churn there.)
        if auto_idle.is_none()
            && last_deep.elapsed() > Duration::from_secs(3)
            && start.elapsed() > Duration::from_secs(2)
        {
            let (cid, cpayload) = send("Profiler.getBestEffortCoverage", json!({}));
            deep_cov_id = cid;
            if ws.send(Message::text(cpayload)).await.is_err() {
                break;
            }
            let (pid2, ppayload2) = send("Profiler.stop", json!({}));
            deep_cpu_id = pid2;
            if ws.send(Message::text(ppayload2)).await.is_err() {
                break;
            }
            let (_sid, spayload) = send("Profiler.start", json!({}));
            let _ = ws.send(Message::text(spayload)).await;
            last_deep = Instant::now();
        }

        let mut log_line: Option<String> = None;
        let mut redraw = false;

        match tokio::time::timeout(Duration::from_millis(120), ws.next()).await {
            Ok(Some(Ok(msg))) => {
                if let Ok(txt) = msg.to_text() {
                    if let Ok(v) = serde_json::from_str::<Value>(txt) {
                        if v["id"].as_u64() == Some(vitals_id) {
                            if let Some(s) = v["result"]["result"]["value"].as_str() {
                                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                                    let lt = parsed
                                        .get("longtask_count")
                                        .and_then(|x| x.as_i64())
                                        .unwrap_or(0);
                                    let lt_ms = parsed
                                        .get("longtask_ms")
                                        .and_then(|x| x.as_f64())
                                        .unwrap_or(0.0);
                                    if let Some(n) = navs.last_mut() {
                                        n.vitals = parsed;
                                    }
                                    if lt > last_longtask {
                                        let t = start.elapsed().as_secs_f64();
                                        log_line = Some(format!(
                                            "{t:6.1}s  ⚠ main-thread blocked: {lt} long task(s), {lt_ms:.0}ms total"
                                        ));
                                        last_longtask = lt;
                                    }
                                }
                            }
                            redraw = true;
                        } else if v["id"].as_u64() == Some(perf_id) {
                            if let Some(arr) = v["result"]["metrics"].as_array() {
                                for m in arr {
                                    if let (Some(name), Some(val)) =
                                        (m["name"].as_str(), m["value"].as_f64())
                                    {
                                        mainthread.insert(name.to_string(), val);
                                    }
                                }
                                // First full reading = baseline (cumulative counters may
                                // already be non-zero when we attach to a running tab).
                                if mainthread_base.is_empty() {
                                    mainthread_base = mainthread.clone();
                                }
                            }
                        } else if v["id"].as_u64() == Some(deep_cov_id) {
                            let cov = parse_coverage(&v["result"]["result"]);
                            if !cov.is_empty() {
                                last_coverage = cov;
                            }
                        } else if v["id"].as_u64() == Some(deep_cpu_id) {
                            for (u, ms) in parse_cpu(&v["result"]["profile"]) {
                                *cpu_accum.entry(u).or_insert(0.0) += ms;
                            }
                        } else if let Some(method) = v["method"].as_str() {
                            let now = start.elapsed().as_secs_f64() * 1000.0;
                            if method.starts_with("Network.") {
                                last_net_activity = Instant::now();
                            }
                            log_line = handle_event(method, &v["params"], &mut navs, now);
                            redraw = true;
                        }
                    }
                }
            }
            Ok(Some(Err(_))) | Ok(None) => {
                stop.store(true, Ordering::Relaxed);
                break;
            }
            Err(_) => {
                // Idle tick: spin the cursor while requests are still loading; otherwise
                // just advance the clock once a second (always in place — never scrolls).
                let in_flight = navs
                    .last()
                    .map(|n| {
                        let done = n
                            .reqs
                            .values()
                            .filter(|r| r.end_ms > r.start_ms || r.failed)
                            .count();
                        n.reqs.len().saturating_sub(done)
                    })
                    .unwrap_or(0);
                if in_flight > 0 {
                    frame = frame.wrapping_add(1);
                    redraw = true;
                } else {
                    let sec = start.elapsed().as_secs();
                    if sec != last_sec {
                        last_sec = sec;
                        redraw = true;
                    }
                }
            }
        }

        // Lab mode: once the page has loaded and the network has been quiet for the idle
        // window (nothing in flight), the load is complete — stop automatically.
        if let Some(idle) = auto_idle {
            let in_flight = navs
                .last()
                .map(|n| {
                    let done = n
                        .reqs
                        .values()
                        .filter(|r| r.end_ms > r.start_ms || r.failed)
                        .count();
                    n.reqs.len().saturating_sub(done)
                })
                .unwrap_or(0);
            let loaded = navs.iter().any(|n| n.loaded_ms > 0.0);
            if loaded
                && in_flight == 0
                && last_net_activity.elapsed() > idle
                && start.elapsed() > Duration::from_millis(1200)
            {
                break;
            }
        }

        let printed = log_line.is_some();
        if let Some(line) = log_line {
            // Clear the sticky status line, print the captured event above it, newline.
            eprint!("\r\x1b[K{line}\n");
        }
        if printed || redraw {
            let status = status_line(&navs, start.elapsed(), frame);
            if printed {
                // We're on a fresh line just below the event — (re)draw the sticky status.
                eprint!("{status}");
                let _ = std::io::stderr().flush();
                last_status = status;
            } else if status != last_status {
                eprint!("\r\x1b[K{status}");
                let _ = std::io::stderr().flush();
                last_status = status;
            }
        }
    }
    // Restore normal caching on the tab we touched (important for --connect: don't leave
    // the user's tab with the cache disabled).
    let (_rid, rpayload) = send(
        "Network.setCacheDisabled",
        json!({ "cacheDisabled": false }),
    );
    let _ = ws.send(Message::text(rpayload)).await;

    // Final deep-capture read while the connection is (hopefully) still alive — most
    // accurate on the Ctrl-C path. `takePreciseCoverage` returns the full-session precise
    // coverage (we never reset it during the loop; the periodic snapshots used the
    // non-resetting `getBestEffortCoverage`). If the window was CLOSED to end the session
    // the socket is already gone and these sends fail — we then fall back to the periodic
    // snapshots (`last_coverage` / `cpu_accum`), so unused-JS + main-thread-by-script
    // survive closing the window, not just Ctrl-C.
    let (cov_id, covp) = send("Profiler.takePreciseCoverage", json!({}));
    let _ = ws.send(Message::text(covp)).await;
    let (prof_id, profp) = send("Profiler.stop", json!({}));
    let _ = ws.send(Message::text(profp)).await;
    let (mut got_cov, mut got_prof) = (false, false);
    let drain_start = Instant::now();
    while (!got_cov || !got_prof) && drain_start.elapsed() < Duration::from_secs(8) {
        match tokio::time::timeout(Duration::from_millis(900), ws.next()).await {
            Ok(Some(Ok(msg))) => {
                if let Ok(txt) = msg.to_text() {
                    if let Ok(v) = serde_json::from_str::<Value>(txt) {
                        match v["id"].as_u64() {
                            x if x == Some(cov_id) => {
                                let cov = parse_coverage(&v["result"]["result"]);
                                if !cov.is_empty() {
                                    last_coverage = cov;
                                }
                                got_cov = true;
                            }
                            x if x == Some(prof_id) => {
                                for (u, ms) in parse_cpu(&v["result"]["profile"]) {
                                    *cpu_accum.entry(u).or_insert(0.0) += ms;
                                }
                                got_prof = true;
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => {}
        }
    }
    let _ = (got_cov, got_prof);
    let mut cpu_by_script: Vec<(String, f64)> = cpu_accum.into_iter().collect();
    cpu_by_script.sort_by(|a, b| b.1.total_cmp(&a.1));
    eprintln!();
    Capture {
        navs,
        mainthread,
        mainthread_base,
        js_coverage: last_coverage,
        cpu_by_script,
        throttled: throttle,
        prod: false,
    }
}

/// Parse `Profiler.takePreciseCoverage` → per-script (url, total_bytes, unused_bytes).
/// In V8 block coverage, a `count==0` range is the authoritative "this span did not run"
/// (an enclosing `count>0` function range only means the function was *entered*). So unused
/// = the union of the count==0 ranges; total = the script's full extent (max endOffset).
fn parse_coverage(arr: &Value) -> Vec<(String, f64, f64)> {
    let mut out = Vec::new();
    let Some(scripts) = arr.as_array() else {
        return out;
    };
    for s in scripts {
        let url = s["url"].as_str().unwrap_or("");
        if url.is_empty() || url.starts_with("chrome") || url.starts_with("extension") {
            continue;
        }
        let mut total = 0u64;
        let mut unused: Vec<(u64, u64)> = Vec::new();
        if let Some(funcs) = s["functions"].as_array() {
            for f in funcs {
                if let Some(ranges) = f["ranges"].as_array() {
                    for r in ranges {
                        let so = r["startOffset"].as_u64().unwrap_or(0);
                        let eo = r["endOffset"].as_u64().unwrap_or(0);
                        total = total.max(eo);
                        if r["count"].as_u64().unwrap_or(1) == 0 && eo > so {
                            unused.push((so, eo));
                        }
                    }
                }
            }
        }
        if total == 0 {
            continue;
        }
        // Union of the not-executed ranges.
        unused.sort_by_key(|&(a, _)| a);
        let (mut merged, mut cs, mut ce, mut started) = (0u64, 0u64, 0u64, false);
        for (s0, e0) in unused {
            if !started {
                cs = s0;
                ce = e0;
                started = true;
            } else if s0 <= ce {
                ce = ce.max(e0);
            } else {
                merged += ce - cs;
                cs = s0;
                ce = e0;
            }
        }
        if started {
            merged += ce - cs;
        }
        out.push((url.to_string(), total as f64, merged.min(total) as f64));
    }
    out
}

/// Parse a `Profiler.stop` CPU profile → main-thread self-time (ms) by script URL.
fn parse_cpu(profile: &Value) -> Vec<(String, f64)> {
    let mut node_url: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
    if let Some(nodes) = profile["nodes"].as_array() {
        for n in nodes {
            if let Some(id) = n["id"].as_u64() {
                node_url.insert(id, n["callFrame"]["url"].as_str().unwrap_or("").to_string());
            }
        }
    }
    let mut by_url: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    if let (Some(samples), Some(deltas)) = (
        profile["samples"].as_array(),
        profile["timeDeltas"].as_array(),
    ) {
        for (i, s) in samples.iter().enumerate() {
            let id = s.as_u64().unwrap_or(0);
            let dt = deltas.get(i).and_then(|d| d.as_f64()).unwrap_or(0.0); // µs
            if let Some(url) = node_url.get(&id) {
                if !url.is_empty() && !url.starts_with("chrome") {
                    *by_url.entry(url.clone()).or_insert(0.0) += dt / 1000.0; // ms
                }
            }
        }
    }
    let mut v: Vec<(String, f64)> = by_url.into_iter().collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

/// Apply one CDP event to the navigation timeline. Returns a human-readable log line for
/// events worth streaming to the user (navigations, finished/failed requests, repeat
/// loops); returns `None` for bookkeeping-only events (`requestWillBeSent`, `responseReceived`).
fn handle_event(method: &str, p: &Value, navs: &mut Vec<Nav>, now_ms: f64) -> Option<String> {
    let t = now_ms / 1000.0;
    match method {
        // A real (hard) or soft (SPA pushState) top-level navigation starts a segment.
        "Page.frameNavigated" => {
            if p["frame"]["parentId"].as_str().is_none() {
                let url = p["frame"]["url"].as_str().unwrap_or("").to_string();
                navs.push(Nav {
                    url: url.clone(),
                    started_ms: now_ms,
                    ..Default::default()
                });
                return Some(format!("{t:6.1}s  ── navigate → {} ──", short_url(&url)));
            }
            None
        }
        "Page.navigatedWithinDocument" => {
            let url = p["url"].as_str().unwrap_or("").to_string();
            navs.push(Nav {
                url: url.clone(),
                started_ms: now_ms,
                ..Default::default()
            });
            Some(format!(
                "{t:6.1}s  ── route → {} (in-app) ──",
                short_url(&url)
            ))
        }
        "Page.loadEventFired" => {
            if let Some(n) = navs.last_mut() {
                n.loaded_ms = now_ms;
            }
            None
        }
        "Network.requestWillBeSent" => {
            let url = p["request"]["url"].as_str().unwrap_or("");
            if url.starts_with("data:") || url.starts_with("blob:") {
                return None;
            }
            let id = p["requestId"].as_str()?;
            // What requested this — for the critical-request-chain depth.
            let init = &p["initiator"];
            let initiator = init["url"]
                .as_str()
                .map(String::from)
                .or_else(|| {
                    init["stack"]["callFrames"][0]["url"]
                        .as_str()
                        .map(String::from)
                })
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| init["type"].as_str().unwrap_or("other").to_string());
            let n = navs.last_mut()?;
            n.reqs.insert(
                id.to_string(),
                Req {
                    url: url.to_string(),
                    method: p["request"]["method"].as_str().unwrap_or("GET").to_string(),
                    kind: req_kind(p["type"].as_str().unwrap_or("")).to_string(),
                    start_ms: now_ms,
                    end_ms: now_ms,
                    initiator,
                    ..Default::default()
                },
            );
            // A single URL re-fetched many times in one navigation is the classic
            // signature of a render loop / stuck spinner / missing cache — surface it.
            let same = n.reqs.values().filter(|r| r.url == url).count();
            if same == REPEAT_ALERT {
                return Some(format!(
                    "{t:6.1}s  ↻ repeated ×{REPEAT_ALERT}: {}  (render loop / stuck spinner?)",
                    short_url(url)
                ));
            }
            None
        }
        "Network.responseReceived" => {
            if let Some(id) = p["requestId"].as_str() {
                let resp = &p["response"];
                // Read headers/cache flags before borrowing `navs` mutably.
                let encoding = header_ci(&resp["headers"], "content-encoding");
                let cache_control = header_ci(&resp["headers"], "cache-control");
                // ETag / Last-Modified let a "max-age=0" asset still revalidate (304, no
                // body) — so it's not really "uncacheable".
                let has_validator = !header_ci(&resp["headers"], "etag").is_empty()
                    || !header_ci(&resp["headers"], "last-modified").is_empty();
                let from_cache = resp["fromDiskCache"].as_bool().unwrap_or(false)
                    || resp["fromPrefetchCache"].as_bool().unwrap_or(false);
                let status = resp["status"].as_i64().unwrap_or(0);
                let ty = p["type"].as_str().map(req_kind);
                // Server wait (TTFB): from the request being sent to the first response
                // byte — receiveHeadersStart − sendEnd in the ResourceTiming (ms).
                let timing = &resp["timing"];
                let send_end = timing["sendEnd"].as_f64().unwrap_or(0.0);
                let recv = timing["receiveHeadersStart"]
                    .as_f64()
                    .or_else(|| timing["receiveHeadersEnd"].as_f64())
                    .unwrap_or(0.0);
                let ttfb = if recv > 0.0 {
                    (recv - send_end).max(0.0)
                } else {
                    0.0
                };
                if let Some(r) = navs.iter_mut().rev().find_map(|n| n.reqs.get_mut(id)) {
                    r.status = status;
                    // `type` on the response is more accurate than on the request.
                    if let Some(k) = ty {
                        r.kind = k.to_string();
                    }
                    r.encoding = encoding;
                    r.cache_control = cache_control;
                    r.has_validator = has_validator;
                    r.from_cache = from_cache;
                    if ttfb > 0.0 {
                        r.ttfb_ms = ttfb;
                    }
                }
            }
            None
        }
        "Network.loadingFinished" => {
            let id = p["requestId"].as_str()?;
            let bytes = p["encodedDataLength"].as_f64().unwrap_or(0.0);
            let r = navs.iter_mut().rev().find_map(|n| n.reqs.get_mut(id))?;
            r.end_ms = now_ms;
            if bytes > 0.0 {
                r.bytes = bytes;
            }
            Some(format!(
                "{t:6.1}s  {:7} {:>3}  {:6.0}ms  {:>8}  {} {}",
                r.kind,
                r.status,
                r.dur_ms(),
                human_bytes(r.bytes),
                r.method,
                short_url(&r.url)
            ))
        }
        "Network.loadingFailed" => {
            let id = p["requestId"].as_str()?;
            // A canceled load (often our own reload aborting the previous page's in-flight
            // requests, or the user navigating away) is not an app failure — log it as an
            // abort and don't count it against the page.
            let canceled = p["canceled"].as_bool().unwrap_or(false);
            let r = navs.iter_mut().rev().find_map(|n| n.reqs.get_mut(id))?;
            r.end_ms = now_ms;
            if canceled {
                return Some(format!(
                    "{t:6.1}s  {:7} abort {:5.0}ms  {} {}",
                    r.kind,
                    r.dur_ms(),
                    r.method,
                    short_url(&r.url)
                ));
            }
            r.failed = true;
            Some(format!(
                "{t:6.1}s  {:7} FAIL  {:6.0}ms  {} {}",
                r.kind,
                r.dur_ms(),
                r.method,
                short_url(&r.url)
            ))
        }
        _ => None,
    }
}

/// Just the path of a URL, capped for display.
fn short_url(u: &str) -> String {
    let path = u
        .splitn(4, '/')
        .nth(3)
        .map(|p| format!("/{p}"))
        .unwrap_or_else(|| u.to_string());
    let path = path.split('?').next().unwrap_or(&path).to_string();
    if path.chars().count() > 34 {
        let tail: String = path
            .chars()
            .rev()
            .take(33)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("…{tail}")
    } else {
        path
    }
}

/// Per-category request summary: (kind, count, total_ms), sorted by total time desc.
/// (total_ms is cumulative; requests run in parallel, so it's a "where time went" hint.)
fn kind_summary(reqs: &std::collections::HashMap<String, Req>) -> Vec<(String, usize, f64)> {
    let mut by: std::collections::HashMap<&str, (usize, f64)> = std::collections::HashMap::new();
    for r in reqs.values() {
        let e = by.entry(r.kind.as_str()).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += r.dur_ms();
    }
    let mut v: Vec<(String, usize, f64)> = by
        .into_iter()
        .map(|(k, (c, t))| (k.to_string(), c, t))
        .collect();
    v.sort_by(|a, b| b.2.total_cmp(&a.2));
    v
}

/// The *load window* of a navigation: from its start until the network first goes idle
/// (a gap of `gap_ms` with no new request starting). Returns (active_span_ms, the requests
/// in that window). This excludes think-time stragglers — most importantly the *next*
/// navigation's request, which fires (requestWillBeSent) before its frameNavigated event
/// and so lands in this segment, otherwise inflating the span to "time the user lingered".
fn nav_load(nav: &Nav, gap_ms: f64) -> (f64, Vec<&Req>) {
    let mut reqs: Vec<&Req> = nav.reqs.values().collect();
    reqs.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    let mut busy_end = nav.started_ms;
    let mut window: Vec<&Req> = Vec::new();
    for r in reqs {
        if r.start_ms <= busy_end + gap_ms {
            busy_end = busy_end.max(r.end_ms);
            window.push(r);
        } else {
            break; // first idle gap → the load is over; the rest is interaction/think-time
        }
    }
    if nav.loaded_ms > 0.0 && nav.loaded_ms <= busy_end + gap_ms {
        busy_end = busy_end.max(nav.loaded_ms);
    }
    ((busy_end - nav.started_ms).max(0.0), window)
}

/// Classify what dominated a navigation's load → (short cause label, remediation advice).
fn slow_cause(window: &[&Req]) -> (&'static str, &'static str) {
    let script_n = window.iter().filter(|r| r.kind == "script").count();
    let img_time: f64 = window
        .iter()
        .filter(|r| r.kind == "img")
        .map(|r| r.dur_ms())
        .sum();
    let api_time: f64 = window
        .iter()
        .filter(|r| r.kind == "api")
        .map(|r| r.dur_ms())
        .sum();
    let slowest = window
        .iter()
        .max_by(|a, b| a.dur_ms().total_cmp(&b.dur_ms()));
    if script_n >= 80 {
        return (
            "module waterfall",
            "Likely a dev build serving JS per-module. Re-measure a production build — bundling collapses this.",
        );
    }
    if let Some(s) = slowest {
        if matches!(s.kind.as_str(), "api" | "doc") && s.dur_ms() >= 800.0 {
            return (
                "slow server response",
                "A single server response dominated — profile / cache that endpoint.",
            );
        }
    }
    if img_time >= 1000.0 {
        return (
            "heavy images",
            "Resize/compress images, use AVIF/WebP, and lazy-load offscreen ones.",
        );
    }
    if api_time >= 1000.0 {
        return (
            "slow API",
            "API calls dominated — cache results or speed up the backend.",
        );
    }
    (
        "many requests",
        "Reduce request count / bundle assets; inspect the waterfall.",
    )
}

/// Braille spinner frames for the animated status line (advances while requests are in flight).
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// A single URL fetched this many times in one navigation is flagged as a likely loop.
const REPEAT_ALERT: usize = 10;

/// The animated sticky status line: a spinner, the live request totals (done/total and
/// how many are still in flight), the per-type breakdown, and the latest vitals.
fn status_line(navs: &[Nav], elapsed: Duration, frame: usize) -> String {
    let spin = SPINNER[frame % SPINNER.len()];
    let Some(nav) = navs.last() else {
        return format!("{spin} {:5.1}s  (starting…)", elapsed.as_secs_f64());
    };
    let total = nav.reqs.len();
    let done = nav
        .reqs
        .values()
        .filter(|r| r.end_ms > r.start_ms || r.failed)
        .count();
    let in_flight = total.saturating_sub(done);
    let g = |k: &str| nav.vitals.get(k).and_then(|x| x.as_f64());
    let fmt = |x: Option<f64>| x.map(|n| format!("{n:.0}ms")).unwrap_or_else(|| "—".into());
    // Top request categories (api/script/css/img/…) by cumulative time.
    let breakdown = kind_summary(&nav.reqs)
        .into_iter()
        .take(4)
        .map(|(k, c, _)| format!("{k} {c}"))
        .collect::<Vec<_>>()
        .join(" ");
    let flight = if in_flight > 0 {
        format!(" ↻{in_flight} loading")
    } else {
        String::new()
    };
    format!(
        "{spin} {:5.1}s  {}  net {}/{}{}  [{}]  LCP {}  INP {}",
        elapsed.as_secs_f64(),
        short_url(&nav.url),
        done,
        total,
        flight,
        breakdown,
        fmt(g("lcp")),
        fmt(g("inp")),
    )
}

/// Connect to a freshly-launched Chrome on a fixed `port` (retry while it starts).
async fn connect_page(port: u16) -> Result<Ws, String> {
    let mut last_err = String::from("no attempt");
    for _ in 0..100 {
        if let Some(ws_url) = page_ws_url(port) {
            match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                Ok(stream) => match tokio_tungstenite::client_async(&ws_url, stream).await {
                    Ok((ws, _)) => return Ok(ws),
                    Err(e) => last_err = format!("ws handshake: {e}"),
                },
                Err(e) => last_err = format!("tcp connect :{port}: {e}"),
            }
        } else {
            last_err = "no page target yet".into();
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(format!("could not connect to Chrome via CDP ({last_err})"))
}

fn same_origin(a: &str, b: &str) -> bool {
    let origin = |s: &str| s.split('/').take(3).collect::<Vec<_>>().join("/");
    origin(a) == origin(b)
}

/// All `page` targets on `url` (exact prefix or same origin), as (ws_url, "title — url").
/// Only tabs ON the app are returned — we never attach to (or navigate) unrelated tabs.
fn matching_tabs(port: u16, url: &str) -> Vec<(String, String)> {
    let Ok(body) = http_get(port, "/json/list") else {
        return Vec::new();
    };
    let Ok(targets) = serde_json::from_str::<Value>(&body) else {
        return Vec::new();
    };
    targets
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    if t["type"] != "page" {
                        return None;
                    }
                    let tu = t["url"].as_str()?;
                    if !(tu.starts_with(url) || same_origin(tu, url)) {
                        return None;
                    }
                    let ws = t["webSocketDebuggerUrl"].as_str()?;
                    let title = t["title"].as_str().unwrap_or("");
                    Some((ws.to_string(), format!("{title} — {tu}")))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Connect to an already-running browser's debug port and attach to the tab on `url`.
/// If several tabs match, the first is used and all matches are listed.
async fn connect_existing(port: u16, url: &str) -> Result<Ws, String> {
    if http_get(port, "/json/version").is_err() {
        return Err(format!(
            "nothing is serving the DevTools protocol on :{port}.\n  Start your browser with --remote-debugging-port={port}, e.g.:\n    flatpak run io.github.ungoogled_software.ungoogled_chromium --user-data-dir=/tmp/vrt-debug --remote-debugging-port={port}\n  then open {url} in it and retry."
        ));
    }
    for _ in 0..30 {
        let tabs = matching_tabs(port, url);
        if let Some((ws_url, label)) = tabs.first() {
            if tabs.len() > 1 {
                eprintln!(
                    "note: {} tabs are on {url}; attaching to the first:",
                    tabs.len()
                );
                for (_, l) in &tabs {
                    eprintln!("  - {l}");
                }
            }
            eprintln!("attached to tab: {label}");
            let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .map_err(|e| format!("tcp connect :{port}: {e}"))?;
            return tokio_tungstenite::client_async(ws_url, stream)
                .await
                .map(|(ws, _)| ws)
                .map_err(|e| format!("ws handshake: {e}"));
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    Err(format!(
        "no tab on {url} found on :{port} — voltiq won't touch your other tabs. Open {url} in that browser, then retry."
    ))
}

/// Launch a throwaway Chrome and capture; `headless` + `auto_idle` select interactive
/// (`web --interactive`) vs headless lab (`web --lab`).
async fn run_launch_core(
    url: &str,
    stop: Arc<AtomicBool>,
    headless: bool,
    auto_idle: Option<Duration>,
    throttle: bool,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let launcher = find_chrome_launcher().ok_or(
        "no Chrome/Chromium found. Install Google Chrome/Chromium (or a flatpak one), or set CHROME_PATH.",
    )?;
    let port = free_port().ok_or("could not allocate a local debug port")?;
    let (mut child, _tmp) = launch_chrome(&launcher, port, headless)?;
    let mut ws = match connect_page(port).await {
        Ok(ws) => ws,
        Err(e) => {
            let _ = child.kill();
            return Err(e);
        }
    };
    let mut cap = capture_session(&mut ws, Some(url), url, stop, auto_idle, throttle).await;
    cap.prod = prod;
    // Close just our throwaway instance.
    let close = json!({"id": 999999, "method": "Browser.close", "params": {}}).to_string();
    let _ = ws.send(Message::text(close)).await;
    let _ = child.kill();
    let _ = child.wait();
    Ok(build_report(url, &cap))
}

/// `web --interactive`: headed browser, capture until the user closes it / Ctrl-C.
async fn run_launch(
    url: &str,
    stop: Arc<AtomicBool>,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    run_launch_core(url, stop, false, None, false, prod).await
}

/// `web --lab`: headless, load once, auto-stop when the network goes idle (CI / automated).
/// With `runs > 1`, repeats the capture and reports the MEDIAN run + the LCP spread, so a
/// single noisy measurement isn't mistaken for the truth.
async fn run_lab(
    url: &str,
    stop: Arc<AtomicBool>,
    throttle: bool,
    runs: usize,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    use voltiq_core::{Confidence, Domain, Finding, Location, Severity, Surface};
    let idle = Some(Duration::from_secs(2));
    if runs <= 1 {
        return run_launch_core(url, stop, true, idle, throttle, prod).await;
    }
    let lcp_of = |r: &PerfReport| r.metrics.iter().find(|m| m.name == "LCP").map(|m| m.value);
    let mut results: Vec<(f64, (PerfReport, Vec<Finding>))> = Vec::new();
    for i in 0..runs {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        eprintln!("── run {}/{runs} ──", i + 1);
        let rf = run_launch_core(url, stop.clone(), true, idle, throttle, prod).await?;
        let lcp = lcp_of(&rf.0).unwrap_or(0.0);
        results.push((lcp, rf));
    }
    if results.is_empty() {
        return Err("no successful lab runs".into());
    }
    results.sort_by(|a, b| a.0.total_cmp(&b.0));
    let lcps: Vec<f64> = results.iter().map(|r| r.0).collect();
    let (lo, hi) = (lcps[0], lcps[lcps.len() - 1]);
    let mid = results.len() / 2;
    let med_lcp = lcps[mid];
    let (_, (perf, mut findings)) = results.swap_remove(mid);
    findings.push(
        Finding::new(
            Domain::Performance,
            "web.run_variance",
            format!(
                "Median of {} runs — LCP {med_lcp:.0} ms (range {lo:.0}–{hi:.0} ms)",
                lcps.len()
            ),
            Severity::Info,
            Confidence::High,
            Surface::Runtime,
            format!(
                "These numbers are the MEDIAN of {} lab runs; LCP varied {lo:.0}–{hi:.0} ms across runs. A wide spread means the measurement is noisy — re-run or widen `--runs` before trusting a single number.",
                lcps.len()
            ),
        )
        .with_location(Location::target(url.to_string())),
    );
    Ok((perf, findings))
}

/// Connect to an already-running browser (started with --remote-debugging-port) and
/// capture interactively (`web --connect <port>`). Does not close the browser.
async fn run_connect(
    port: u16,
    url: &str,
    stop: Arc<AtomicBool>,
    prod: bool,
) -> Result<(PerfReport, Vec<Finding>), String> {
    let mut ws = connect_existing(port, url).await?;
    // Reload the matched app tab so load metrics (LCP/FCP) are captured from the start;
    // INP/CLS then accrue from your interaction. We only touch the tab on `url`.
    eprintln!("(reloading the matched tab to capture load metrics)");
    let mut cap = capture_session(&mut ws, Some(url), url, stop, None, false).await;
    cap.prod = prod;
    Ok(build_report(url, &cap))
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(|x| x.as_f64())
}

/// Build the report from the navigation timeline: aggregate Web Vitals (worst across
/// navigations) + flag slow navigations with their request waterfall, plus transfer
/// size / compression / cache hygiene, 1st-vs-3rd-party weight, vital breakdowns, and the
/// CDP main-thread category split.
/// Format the top `n` requests as "shorturl size, …" with NO repeated display label. A
/// module fetched across several navigations (or two URLs that abbreviate to the same name)
/// is one culprit file, not several — without this dedup a "heaviest/largest" list shows the
/// same file 2–3×. Expects `sorted` already in priority order; keeps the first (top) hit.
fn top_files(sorted: &[&Req], n: usize) -> String {
    let mut seen = std::collections::HashSet::new();
    sorted
        .iter()
        .map(|r| (short_url(&r.url), r.bytes))
        .filter(|(u, _)| seen.insert(u.clone()))
        .take(n)
        .map(|(u, b)| format!("{u} {}", human_bytes(b)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Root-cause detail + remediation for a failing Core Web Vital, derived from the worst
/// navigation's collected vitals — so the headline finding says WHAT caused the bad score
/// (which interaction/element, the phase split), not merely that it crossed the threshold.
fn vital_cause(key: &str, v: &Value) -> Option<(String, String)> {
    match key {
        "inp" => {
            let idl = num(v, "inp_input").unwrap_or(0.0);
            let proc = num(v, "inp_proc").unwrap_or(0.0);
            let pres = num(v, "inp_present").unwrap_or(0.0);
            let typ = v
                .get("inp_type")
                .and_then(|x| x.as_str())
                .unwrap_or("interaction");
            let target = v
                .get("inp_target")
                .and_then(|x| x.as_str())
                .unwrap_or("an element");
            let (phase, advice) = if idl >= proc && idl >= pres {
                ("input delay", "The main thread was busy when the input arrived — break up long tasks (code-split, defer/yield) so the handler can start sooner.")
            } else if proc >= pres {
                ("event processing", "The event handler itself is slow — profile it, defer non-urgent work (requestIdleCallback / a worker), and avoid synchronous layout reads inside it.")
            } else {
                ("presentation delay", "Rendering after the handler is slow — shrink the layout/paint the interaction triggers (smaller DOM updates, avoid large reflows, prefer CSS transforms).")
            };
            Some((
                format!("Worst interaction: {typ} on {target} — input delay {idl:.0} ms · processing {proc:.0} ms · presentation {pres:.0} ms; {phase} dominates"),
                advice.to_string(),
            ))
        }
        "lcp" => {
            let el = v.get("lcp_el").and_then(|x| x.as_str())?;
            let lurl = v
                .get("lcp_url")
                .and_then(|x| x.as_str())
                .filter(|s| !s.is_empty());
            let src = lurl
                .map(|u| format!(" ({})", short_url(u)))
                .unwrap_or_default();
            Some((
                format!("Largest Contentful Paint element: {el}{src} — the slowest thing to render above the fold"),
                "Make this element load first: preload / modulepreload its resource, set fetchpriority=\"high\" on the LCP image, serve it sized + compressed, and cut render-blocking CSS/JS ahead of it.".to_string(),
            ))
        }
        "cls" => {
            let el = v.get("cls_el").and_then(|x| x.as_str())?;
            Some((
                format!("Largest layout shift moved {el} — content jumped after it had already rendered"),
                "Reserve its space up front: set explicit width/height (or aspect-ratio / min-height), size images and ad/embed/iframe slots, and avoid inserting content above what's already on screen.".to_string(),
            ))
        }
        _ => None, // FCP has no element-level cause — render-blocking resources finding covers it.
    }
}

fn build_report(url: &str, cap: &Capture) -> (PerfReport, Vec<Finding>) {
    use voltiq_core::{
        Confidence, Domain, Location, Metric, MetricStatus, Series, Severity, Surface,
    };
    let navs = &cap.navs;
    let mainthread = &cap.mainthread;
    let mainthread_base = &cap.mainthread_base;

    let mut report = PerfReport {
        runtime: Some(if cap.throttled {
            "chromium (throttled: slow-4G + 4× CPU)".into()
        } else {
            "chromium (unthrottled localhost)".into()
        }),
        ..Default::default()
    };
    let mut findings = Vec::new();

    // Worst value of a vital across all navigations.
    let agg = |key: &str| -> Option<f64> {
        navs.iter()
            .filter_map(|n| num(&n.vitals, key))
            .fold(None, |acc, x| Some(acc.map_or(x, |a: f64| a.max(x))))
    };
    // The worst nav's full vitals object for a key — carries the root-cause fields
    // (interaction target, INP phase split, LCP/CLS element) used to explain a failure.
    let worst_vitals = |key: &str| -> Option<&Value> {
        navs.iter()
            .map(|n| &n.vitals)
            .filter(|v| num(v, key).is_some())
            .max_by(|a, b| {
                num(a, key)
                    .unwrap_or(0.0)
                    .total_cmp(&num(b, key).unwrap_or(0.0))
            })
    };
    // Vitals whose headline finding already carries the cause — so the standalone breakdown
    // finding isn't emitted twice (the data is now attached to the failure itself).
    let mut diagnosed: Vec<&str> = Vec::new();
    for &(key, label, unit, good, poor) in &[
        ("fcp", "FCP", "ms", 1800.0, 3000.0),
        ("lcp", "LCP", "ms", 2500.0, 4000.0),
        ("inp", "INP", "ms", 200.0, 500.0),
        ("cls", "CLS", "", 0.1, 0.25),
    ] {
        let Some(val) = agg(key) else { continue };
        let status = if val <= good {
            MetricStatus::Pass
        } else if val <= poor {
            MetricStatus::Warn
        } else {
            MetricStatus::Fail
        };
        report
            .metrics
            .push(Metric::new(label, val, unit, status).with_threshold(good));
        if status != MetricStatus::Pass {
            let sev = if val > poor {
                Severity::High
            } else {
                Severity::Medium
            };
            let shown = if unit.is_empty() {
                format!("{val:.3}")
            } else {
                format!("{val:.0} {unit}")
            };
            let bound = if unit.is_empty() {
                format!("good ≤ {good}, poor > {poor}")
            } else {
                format!("good ≤ {good:.0} {unit}, poor > {poor:.0} {unit}")
            };
            // Attach the captured root cause (which interaction/element, the phase split) so
            // the failure is self-explanatory, plus a concrete fix.
            let cause = worst_vitals(key).and_then(|v| vital_cause(key, v));
            let desc = match &cause {
                // Cause on an indented line so the report highlights it as the root cause.
                Some((detail, _)) => format!(
                    "{label} exceeded the 'good' Core Web Vitals threshold ({bound}).\n  {detail}."
                ),
                None => {
                    format!("{label} exceeded the 'good' Core Web Vitals threshold ({bound}).")
                }
            };
            let mut f = Finding::new(
                Domain::Performance,
                format!("web.{key}"),
                format!("{label} {shown} (worst across navigations)"),
                sev,
                Confidence::High,
                Surface::Runtime,
                desc,
            )
            .with_location(Location::target(url.to_string()));
            if let Some((_, rem)) = cause {
                f = f.with_remediation(rem);
                diagnosed.push(key);
            }
            findings.push(f);
        }
    }

    // Per-navigation: flag slow ones with their slowest requests (the "what made this
    // slow" view), and record totals.
    let mut total_reqs = 0usize;
    let mut nav_count = 0usize;
    for nav in navs {
        if nav.reqs.is_empty() && nav.span_ms() <= 0.0 {
            continue;
        }
        nav_count += 1;
        total_reqs += nav.reqs.len();
        // Measure the load window (network-busy time), not wall-clock until the next
        // navigation — so think-time between routes doesn't read as a slow page.
        let (span, window) = nav_load(nav, 800.0);
        let mut by_dur: Vec<&Req> = window
            .iter()
            .copied()
            .filter(|r| r.dur_ms() > 0.0)
            .collect();
        by_dur.sort_by(|a, b| b.dur_ms().total_cmp(&a.dur_ms()));
        let mut seen_slow = std::collections::HashSet::new();
        let top: Vec<String> = by_dur
            .iter()
            .filter(|r| seen_slow.insert(short_url(&r.url)))
            .take(3)
            .map(|r| {
                let st = if r.failed {
                    "FAILED".to_string()
                } else {
                    r.status.to_string()
                };
                let sz = if r.bytes > 0.0 {
                    format!(", {}", human_bytes(r.bytes))
                } else {
                    String::new()
                };
                // Split the time into server-wait (TTFB) vs the rest, when known.
                let wait = if r.ttfb_ms > 1.0 {
                    format!(", {:.0}ms server-wait", r.ttfb_ms)
                } else {
                    String::new()
                };
                format!(
                    "{} {} {:.0}ms ({st}{sz}{wait})",
                    r.method,
                    short_url(&r.url),
                    r.dur_ms()
                )
            })
            .collect();
        // Repeated-request loop (the "stuck spinner" signature): one URL fetched many times
        // in a single navigation. Surface it as a finding so it persists in the report /
        // brief / dashboard — previously it only appeared in the live stream.
        {
            let mut freq: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            let mut worst_url = "";
            let mut worst_n = 0usize;
            for r in nav.reqs.values() {
                let c = freq.entry(r.url.as_str()).or_insert(0);
                *c += 1;
                if *c > worst_n {
                    worst_n = *c;
                    worst_url = r.url.as_str();
                }
            }
            if worst_n >= REPEAT_ALERT {
                // Distinguish a tight render-loop from intentional polling: only flag when
                // the repeats are bunched (firing rapidly). A 5s timer that fires 10× over
                // 50s isn't a loop; 10× within a second is.
                let mut starts: Vec<f64> = nav
                    .reqs
                    .values()
                    .filter(|r| r.url == worst_url)
                    .map(|r| r.start_ms)
                    .collect();
                starts.sort_by(|a, b| a.total_cmp(b));
                let avg_gap = if starts.len() >= 2 {
                    (starts[starts.len() - 1] - starts[0]) / (starts.len() - 1) as f64
                } else {
                    f64::INFINITY
                };
                if avg_gap < 600.0 {
                    findings.push(
                        Finding::new(
                            Domain::Performance,
                            "web.repeated_request",
                            format!("{} was requested {worst_n}× in quick succession", short_url(worst_url)),
                            if worst_n >= 30 { Severity::High } else { Severity::Medium },
                            Confidence::High,
                            Surface::Runtime,
                            format!("The same URL ({worst_url}) was fetched {worst_n} times (~{avg_gap:.0} ms apart) while loading {} — the classic signature of a render loop, a reactive effect with a bad dependency array, or a missing request cache/dedupe.", nav.url),
                        )
                        .with_location(Location::target(nav.url.clone()))
                        .with_remediation("Find the effect/subscription re-firing this request; dedupe in-flight requests and cache the result."),
                    );
                }
            }
        }
        if span > 1500.0 {
            let (cause, advice) = slow_cause(&window);
            // Images usually load async / below-the-fold and rarely block interactivity, so
            // don't escalate an image-heavy nav to High on span alone; render/JS/API causes
            // that actually block the page keep the span-based severity.
            let sev = if cause == "heavy images" {
                Severity::Medium
            } else if span > 5000.0 {
                Severity::High
            } else {
                Severity::Medium
            };
            // Counts per type within the load window (counts — not the misleading sum of
            // parallel request durations the old "by type" showed).
            let mut counts: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for r in &window {
                *counts.entry(r.kind.as_str()).or_insert(0) += 1;
            }
            let mut cv: Vec<(&str, usize)> = counts.into_iter().collect();
            cv.sort_by_key(|x| std::cmp::Reverse(x.1));
            let by_type = cv
                .iter()
                .take(6)
                .map(|(k, c)| format!("{k} {c}"))
                .collect::<Vec<_>>()
                .join(", ");
            let win_bytes: f64 = window.iter().map(|r| r.bytes).sum();
            let bytes_str = if win_bytes > 0.0 {
                format!(", {}", human_bytes(win_bytes))
            } else {
                String::new()
            };
            let mut f = Finding::new(
                Domain::Performance,
                "web.slow_navigation",
                format!(
                    "[{cause}] {} — {:.0} ms to load ({} reqs{bytes_str})",
                    short_url(&nav.url),
                    span,
                    window.len()
                ),
                sev,
                Confidence::High,
                Surface::Runtime,
                format!(
                    "Loading {} took {:.0} ms of network activity across {} requests (cause: {cause}).\n  by type: {}\n  slowest: {}",
                    nav.url,
                    span,
                    window.len(),
                    by_type,
                    if top.is_empty() { "—".to_string() } else { top.join("; ") }
                ),
            )
            .with_location(Location::target(nav.url.clone()))
            .with_remediation(advice);
            f.metadata
                .insert("cause".into(), Value::String(cause.to_string()));
            findings.push(f);
        }
    }
    report.metrics.push(Metric::new(
        "navigations",
        nav_count as f64,
        "count",
        MetricStatus::Info,
    ));
    report.metrics.push(Metric::new(
        "requests",
        total_reqs as f64,
        "count",
        MetricStatus::Info,
    ));
    // Session-wide request breakdown by category (api / script / css / img / …).
    let mut by_cat: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for nav in navs {
        for r in nav.reqs.values() {
            *by_cat.entry(r.kind.as_str()).or_insert(0) += 1;
        }
    }
    let mut cats: Vec<(&str, usize)> = by_cat.into_iter().collect();
    cats.sort_by_key(|x| std::cmp::Reverse(x.1));
    for (kind, count) in cats {
        report.metrics.push(Metric::new(
            format!("{kind} reqs"),
            count as f64,
            "count",
            MetricStatus::Info,
        ));
    }
    if let Some(ms) = agg("longtask_ms") {
        let status = if ms > 200.0 {
            MetricStatus::Warn
        } else {
            MetricStatus::Info
        };
        report
            .metrics
            .push(Metric::new("long-task time", ms, "ms", status));
    }

    // ---- Transfer size / compression / cache hygiene -------------------------------
    let all: Vec<&Req> = navs.iter().flat_map(|n| n.reqs.values()).collect();
    let total_bytes: f64 = all.iter().map(|r| r.bytes).sum();
    // A dev server (Vite/SvelteKit) serves unminified, uncompressed, per-module JS — so
    // note that, lest a dev measurement read as a production regression.
    // `--prod` asserts this IS a production build, so we never treat it as dev (no "looks
    // like a dev server" hint, no downgrading findings as expected-dev-behavior). Otherwise
    // we auto-detect a dev server from its tell-tale URLs.
    let dev_mode = !cap.prod
        && all.iter().any(|r| {
            let u = r.url.as_str();
            u.contains("/.vite/")
                || u.contains("/@vite/")
                || u.contains("/@fs/")
                || u.contains("/node_modules/.vite/")
        });
    let dev_hint = if dev_mode {
        "\n  NOTE: this looks like a dev server — production builds minify + compress; re-measure a prod build."
    } else {
        ""
    };
    // Abnormally high request count for a single navigation — surfaced REGARDLESS of
    // localhost speed. On localhost hundreds of module requests load fast, but on a real
    // network each one adds latency and they serialize into seconds; it's the signature of
    // an un-bundled build. This is the abnormality a fast local run would otherwise hide.
    {
        // Page/session totals (SvelteKit splits a load across several nav segments, so a
        // per-nav count under-reports the real module-waterfall size).
        let n_reqs = all.len();
        let n_script = all.iter().filter(|r| r.kind == "script").count();
        // Trip on a high total OR a high script count — the latter is the un-bundled
        // signature (a bundled prod app has well under ~40 script requests).
        if n_reqs >= 100 || n_script >= 80 {
            let sev = if dev_mode {
                Severity::Low
            } else {
                Severity::Medium
            };
            // Name the heaviest modules + the directory most of them come from — the actual
            // files driving the waterfall, so the fix targets something concrete.
            let mut heavy: Vec<&Req> = all
                .iter()
                .copied()
                .filter(|r| r.kind == "script" && r.bytes > 0.0)
                .collect();
            heavy.sort_by(|a, b| b.bytes.total_cmp(&a.bytes));
            let heavy_list = top_files(&heavy, 3);
            let heavy_line = if heavy_list.is_empty() {
                String::new()
            } else {
                format!("\n  heaviest modules: {heavy_list}")
            };
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.request_waterfall",
                    format!("{n_reqs} requests for this page ({n_script} JS modules)"),
                    sev,
                    Confidence::High,
                    Surface::Runtime,
                    format!(
                        "This page fired {n_reqs} separate requests, {n_script} of them JS modules. Fast on localhost, but on a real network each request adds round-trip latency — hundreds of module requests serialize into seconds. This is the classic signature of an un-bundled build.{heavy_line}{dev_hint}"
                    ),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation(
                    "Ship a production build: bundling + code-splitting collapses hundreds of dev module requests into a handful of chunks (often hundreds → tens).",
                ),
            );
        }
    }
    if total_bytes > 0.0 {
        report.metrics.push(Metric::new(
            "transfer",
            total_bytes / 1024.0,
            "KB",
            MetricStatus::Info,
        ));
    }
    // Text-ish assets shipped with no content-encoding are sent uncompressed.
    let mut uncompressed: Vec<&Req> = all
        .iter()
        .copied()
        .filter(|r| {
            matches!(r.kind.as_str(), "script" | "css" | "api" | "doc")
                && r.bytes > 0.0
                && r.encoding.is_empty()
                && !r.from_cache
        })
        .collect();
    let uncompressed_bytes: f64 = uncompressed.iter().map(|r| r.bytes).sum();
    if uncompressed_bytes > 256.0 * 1024.0 {
        uncompressed.sort_by(|a, b| b.bytes.total_cmp(&a.bytes));
        let top = top_files(&uncompressed, 3);
        // Dev servers serve uncompressed by design — flag it (still useful) but don't fail
        // the gate over it; it only matters for a production build.
        let sev = if dev_mode {
            Severity::Low
        } else if uncompressed_bytes > 2.0 * 1_048_576.0 {
            Severity::High
        } else {
            Severity::Medium
        };
        findings.push(
            Finding::new(
                Domain::Performance,
                "web.uncompressed_assets",
                format!(
                    "{} of text/JS/CSS uncompressed — est. save ~{} with gzip ({} responses)",
                    human_bytes(uncompressed_bytes),
                    human_bytes(uncompressed_bytes * 0.72),
                    uncompressed.len()
                ),
                sev,
                Confidence::High,
                Surface::Runtime,
                format!(
                    "These responses have no content-encoding (gzip/brotli).\n  largest: {top}{dev_hint}"
                ),
            )
            .with_location(Location::target(url.to_string()))
            .with_remediation(
                "Enable gzip/brotli on the server for text, JS, CSS, JSON and SVG responses.",
            ),
        );
    }
    // Static assets with no/weak cache-control can't be reused across loads.
    let weak_cache = |cc: &str| {
        let c = cc.to_ascii_lowercase();
        cc.is_empty() || c.contains("no-store") || c.contains("no-cache") || c.contains("max-age=0")
    };
    let mut uncached: Vec<&Req> = all
        .iter()
        .copied()
        .filter(|r| {
            matches!(r.kind.as_str(), "script" | "css" | "img" | "font")
                && r.bytes > 0.0
                && !r.from_cache
                && weak_cache(&r.cache_control)
                // ETag / Last-Modified still allow an efficient 304 revalidation, so an
                // asset with a validator isn't really "uncacheable".
                && !r.has_validator
        })
        .collect();
    let uncached_bytes: f64 = uncached.iter().map(|r| r.bytes).sum();
    if uncached_bytes > 256.0 * 1024.0 {
        uncached.sort_by(|a, b| b.bytes.total_cmp(&a.bytes));
        let top = top_files(&uncached, 3);
        // Dev servers don't set cache headers by design — flag but don't fail the gate.
        let sev = if dev_mode {
            Severity::Low
        } else {
            Severity::Medium
        };
        findings.push(
            Finding::new(
                Domain::Performance,
                "web.uncacheable_assets",
                format!(
                    "{} of static assets have no usable cache lifetime ({} responses)",
                    human_bytes(uncached_bytes),
                    uncached.len()
                ),
                sev,
                Confidence::High,
                Surface::Runtime,
                format!(
                    "Static assets with no validator (ETag/Last-Modified) and missing / no-store / max-age=0 cache-control are re-fetched in full every load.\n  largest: {top}{dev_hint}"
                ),
            )
            .with_location(Location::target(url.to_string()))
            .with_remediation(
                "Serve hashed static assets with a long immutable cache-control (e.g. max-age=31536000, immutable); add ETag/Last-Modified otherwise.",
            ),
        );
    }

    // ---- Heavy JavaScript payload (production only — dev ships unbundled modules) --
    if !dev_mode {
        let js_bytes: f64 = all
            .iter()
            .filter(|r| r.kind == "script")
            .map(|r| r.bytes)
            .sum();
        if js_bytes > 1_048_576.0 {
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.heavy_javascript",
                    format!("{} of JavaScript transferred", human_bytes(js_bytes)),
                    if js_bytes > 3.0 * 1_048_576.0 {
                        Severity::Medium
                    } else {
                        Severity::Low
                    },
                    Confidence::High,
                    Surface::Runtime,
                    "Large JS payloads delay interactivity (download + parse + execute), especially on mobile. Code-split, tree-shake, and defer non-critical bundles.",
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation("Code-split by route, lazy-load below-the-fold components, drop unused dependencies, and ship modern (ES) builds."),
            );
        }
    }

    // ---- 1st- vs 3rd-party weight --------------------------------------------------
    let page_host = host_of(url);
    let mut first_bytes = 0.0_f64;
    let mut third_bytes = 0.0_f64;
    let mut third_reqs = 0usize;
    let mut by_host: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for r in &all {
        let h = host_of(&r.url);
        if h.is_empty() {
            continue;
        }
        *by_host.entry(h.clone()).or_insert(0.0) += r.bytes;
        if h == page_host {
            first_bytes += r.bytes;
        } else {
            third_bytes += r.bytes;
            third_reqs += 1;
        }
    }
    if first_bytes > 0.0 {
        report.metrics.push(Metric::new(
            "1st-party",
            first_bytes / 1024.0,
            "KB",
            MetricStatus::Info,
        ));
    }
    if third_bytes > 0.0 {
        report.metrics.push(Metric::new(
            "3rd-party",
            third_bytes / 1024.0,
            "KB",
            MetricStatus::Info,
        ));
        report.metrics.push(Metric::new(
            "3rd-party reqs",
            third_reqs as f64,
            "count",
            MetricStatus::Info,
        ));
    }
    if third_bytes > 500.0 * 1024.0 {
        let mut hosts: Vec<(&String, &f64)> = by_host
            .iter()
            .filter(|(h, _)| **h != page_host && !h.is_empty())
            .collect();
        hosts.sort_by(|a, b| b.1.total_cmp(a.1));
        let top = hosts
            .iter()
            .take(4)
            .map(|(h, b)| format!("{h} {}", human_bytes(**b)))
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(
            Finding::new(
                Domain::Performance,
                "web.third_party_weight",
                format!(
                    "{} from {third_reqs} third-party requests",
                    human_bytes(third_bytes)
                ),
                Severity::Low,
                Confidence::High,
                Surface::Runtime,
                format!("Third-party origins by transfer size: {top}."),
            )
            .with_location(Location::target(url.to_string())),
        );
    }

    // ---- Vital breakdowns (INP / LCP / layout-shift culprit) -----------------------
    // (`worst_vitals` is defined above.) The standalone breakdown finding is suppressed when
    // the vital already FAILED — that finding now carries the cause (see `diagnosed`) — but
    // the breakdown metrics are always pushed, and the finding still fires for a passing
    // vital (e.g. INP between 100 ms and the 200 ms "good" line).
    if let Some(v) = worst_vitals("inp") {
        let inp = num(v, "inp").unwrap_or(0.0);
        let idl = num(v, "inp_input").unwrap_or(0.0);
        let proc = num(v, "inp_proc").unwrap_or(0.0);
        let pres = num(v, "inp_present").unwrap_or(0.0);
        report.metrics.push(Metric::new(
            "INP input delay",
            idl,
            "ms",
            MetricStatus::Info,
        ));
        report.metrics.push(Metric::new(
            "INP processing",
            proc,
            "ms",
            MetricStatus::Info,
        ));
        report.metrics.push(Metric::new(
            "INP presentation",
            pres,
            "ms",
            MetricStatus::Info,
        ));
        if inp >= 100.0 && !diagnosed.contains(&"inp") {
            let typ = v
                .get("inp_type")
                .and_then(|x| x.as_str())
                .unwrap_or("interaction");
            let target = v.get("inp_target").and_then(|x| x.as_str()).unwrap_or("?");
            let (phase, advice) = if idl >= proc && idl >= pres {
                (
                    "input delay",
                    "the main thread was busy when the input arrived — break up long tasks",
                )
            } else if proc >= pres {
                (
                    "event processing",
                    "the event handler itself is slow — optimize / defer its work",
                )
            } else {
                (
                    "presentation delay",
                    "rendering after the handler is slow — reduce layout/paint work",
                )
            };
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.inp_breakdown",
                    format!("INP {inp:.0} ms on {typ} — {phase} dominates"),
                    Severity::Low,
                    Confidence::High,
                    Surface::Runtime,
                    format!(
                        "Worst interaction: {typ} on {target}. input delay {idl:.0} ms · processing {proc:.0} ms · presentation {pres:.0} ms.\n  {advice}."
                    ),
                )
                .with_location(Location::target(url.to_string())),
            );
        }
    }
    if let Some(v) = worst_vitals("lcp") {
        let el = v.get("lcp_el").and_then(|x| x.as_str());
        let lurl = v
            .get("lcp_url")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty());
        if (el.is_some() || lurl.is_some()) && !diagnosed.contains(&"lcp") {
            let lcp = num(v, "lcp").unwrap_or(0.0);
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.lcp_element",
                    format!("LCP element: {} ({lcp:.0} ms)", el.unwrap_or("?")),
                    Severity::Info,
                    Confidence::High,
                    Surface::Runtime,
                    format!(
                        "Largest Contentful Paint was {}{}.",
                        el.unwrap_or("(unknown element)"),
                        lurl.map(|u| format!(" — {}", short_url(u)))
                            .unwrap_or_default()
                    ),
                )
                .with_location(Location::target(url.to_string())),
            );
        }
    }
    if let Some(v) = worst_vitals("cls") {
        let cls = num(v, "cls").unwrap_or(0.0);
        if cls > 0.01 && !diagnosed.contains(&"cls") {
            if let Some(el) = v.get("cls_el").and_then(|x| x.as_str()) {
                findings.push(
                    Finding::new(
                        Domain::Performance,
                        "web.layout_shift_culprit",
                        format!("Layout shift culprit: {el} (CLS {cls:.3})"),
                        Severity::Info,
                        Confidence::Medium,
                        Surface::Runtime,
                        format!("The largest layout shift moved {el}. Give it explicit dimensions / reserve space."),
                    )
                    .with_location(Location::target(url.to_string())),
                );
            }
        }
    }

    // ---- Main-thread category split (CDP Performance.getMetrics) -------------------
    // Durations are cumulative since the renderer started, so report (latest − first
    // reading): for a fresh browser the baseline is ~0, but for `--connect` it strips the
    // time the tab was busy before we attached.
    for (label, key) in [
        ("main: scripting", "ScriptDuration"),
        ("main: layout", "LayoutDuration"),
        ("main: style", "RecalcStyleDuration"),
        ("main: task total", "TaskDuration"),
    ] {
        let base = mainthread_base.get(key).copied().unwrap_or(0.0);
        let v = (mainthread.get(key).copied().unwrap_or(0.0) - base).max(0.0) * 1000.0;
        if v > 0.0 {
            report
                .metrics
                .push(Metric::new(label, v, "ms", MetricStatus::Info));
        }
    }
    if let Some(h) = mainthread.get("JSHeapUsedSize") {
        if *h > 0.0 {
            report.metrics.push(Metric::new(
                "JS heap",
                h / 1_048_576.0,
                "MB",
                MetricStatus::Info,
            ));
        }
    }
    if let Some(n) = mainthread.get("Nodes") {
        if *n > 0.0 {
            report
                .metrics
                .push(Metric::new("DOM nodes", *n, "count", MetricStatus::Info));
        }
    }

    // ---- Render-blocking resources + oversized images (from the DOM snapshots) ------
    let mut blocking: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut images: Vec<(String, f64, f64, f64, f64)> = Vec::new();
    let mut seen_img: std::collections::HashSet<String> = std::collections::HashSet::new();
    for nav in navs {
        if let Some(arr) = nav.vitals.get("blocking").and_then(|x| x.as_array()) {
            for u in arr {
                if let Some(s) = u.as_str() {
                    blocking.insert(s.to_string());
                }
            }
        }
        if let Some(arr) = nav.vitals.get("images").and_then(|x| x.as_array()) {
            for im in arr {
                let src = im["src"].as_str().unwrap_or("");
                if src.is_empty() || !seen_img.insert(src.to_string()) {
                    continue;
                }
                images.push((
                    src.to_string(),
                    im["nw"].as_f64().unwrap_or(0.0),
                    im["nh"].as_f64().unwrap_or(0.0),
                    im["dw"].as_f64().unwrap_or(0.0),
                    im["dh"].as_f64().unwrap_or(0.0),
                ));
            }
        }
    }
    if blocking.len() >= 2 {
        let bytes: f64 = all
            .iter()
            .filter(|r| blocking.contains(&r.url))
            .map(|r| r.bytes)
            .sum();
        let mut list: Vec<&String> = blocking.iter().collect();
        list.sort();
        let mut seen_block = std::collections::HashSet::new();
        let top = list
            .iter()
            .map(|u| short_url(u))
            .filter(|s| seen_block.insert(s.clone()))
            .take(4)
            .collect::<Vec<_>>()
            .join(", ");
        findings.push(
            Finding::new(
                Domain::Performance,
                "web.render_blocking",
                format!("{} render-blocking resources delay first paint", blocking.len()),
                if blocking.len() >= 5 { Severity::Medium } else { Severity::Low },
                Confidence::High,
                Surface::Runtime,
                format!(
                    "{} stylesheet / synchronous-script resources in <head> ({}) block the first paint — each must download + parse before anything renders.\n  e.g.: {top}",
                    blocking.len(),
                    human_bytes(bytes)
                ),
            )
            .with_location(Location::target(url.to_string()))
            .with_remediation("Inline critical CSS, defer the rest (media=print swap), and add async/defer or type=module to head scripts."),
        );
    }
    {
        let mut wasted = 0.0_f64;
        let mut worst: Vec<(String, f64)> = Vec::new();
        for (src, nw, nh, dw, dh) in &images {
            if *dw < 1.0 || *dh < 1.0 || *nw < 1.0 {
                continue;
            }
            let nat = nw * nh;
            let disp = dw * dh * 4.0; // allow up to ~2× DPR before calling it oversized
            if nat > disp * 1.1 {
                let b = all
                    .iter()
                    .find(|r| &r.url == src)
                    .map(|r| r.bytes)
                    .unwrap_or(0.0);
                let save = b * (1.0 - (disp / nat).min(1.0));
                if save > 20.0 * 1024.0 {
                    wasted += save;
                    worst.push((src.clone(), save));
                }
            }
        }
        if wasted > 50.0 * 1024.0 {
            worst.sort_by(|a, b| b.1.total_cmp(&a.1));
            let mut seen_img = std::collections::HashSet::new();
            let top = worst
                .iter()
                .map(|(u, s)| (short_url(u), *s))
                .filter(|(u, _)| seen_img.insert(u.clone()))
                .take(3)
                .map(|(u, s)| format!("{u} (~{})", human_bytes(s)))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.oversized_images",
                    format!("{} oversized images — est. save ~{}", worst.len(), human_bytes(wasted)),
                    Severity::Low,
                    Confidence::Medium,
                    Surface::Runtime,
                    format!(
                        "{} image(s) ship far more pixels than they're displayed at. Resize to the rendered size (× DPR) and use a modern format.\n  worst: {top}",
                        worst.len()
                    ),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation("Generate responsive sizes (srcset/sizes), serve AVIF/WebP, and resize to the displayed dimensions."),
            );
        }
    }

    // ---- Critical request chain depth (sequential dependencies) --------------------
    {
        use std::collections::HashMap as Map;
        let init: Map<&str, &str> = all
            .iter()
            .map(|r| (r.url.as_str(), r.initiator.as_str()))
            .collect();
        fn depth<'a>(
            u: &'a str,
            init: &Map<&'a str, &'a str>,
            memo: &mut Map<&'a str, usize>,
            guard: usize,
        ) -> usize {
            if guard > 60 {
                return 1;
            }
            if let Some(&d) = memo.get(u) {
                return d;
            }
            let d = match init.get(u) {
                Some(&p) if p != u && init.contains_key(p) => 1 + depth(p, init, memo, guard + 1),
                _ => 1,
            };
            memo.insert(u, d);
            d
        }
        let mut memo = Map::new();
        // Track which leaf has the deepest chain, so we can show the actual files in it.
        let mut max_depth = 0usize;
        let mut deepest: Option<&str> = None;
        for r in &all {
            let d = depth(r.url.as_str(), &init, &mut memo, 0);
            if d > max_depth {
                max_depth = d;
                deepest = Some(r.url.as_str());
            }
        }
        if max_depth >= 5 {
            // Walk the initiator links from the deepest leaf up to the root, then reverse to
            // read root → leaf — the exact sequential file chain that serializes.
            let mut chain: Vec<&str> = Vec::new();
            if let Some(mut u) = deepest {
                let mut guard = 0;
                loop {
                    chain.push(u);
                    match init.get(u) {
                        Some(&p) if p != u && init.contains_key(p) && guard < 60 => {
                            u = p;
                            guard += 1;
                        }
                        _ => break,
                    }
                }
            }
            chain.reverse();
            let mut chain_disp: Vec<String> = chain.iter().map(|u| short_url(u)).collect();
            chain_disp.dedup(); // collapse consecutive links that abbreviate to the same name
            let chain_str = chain_disp.join(" → ");
            let chain_line = if chain_str.is_empty() {
                String::new()
            } else {
                format!("\n  chain: {chain_str}")
            };
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.critical_chain",
                    format!("Critical request chain is {max_depth} levels deep"),
                    Severity::Low,
                    Confidence::Medium,
                    Surface::Runtime,
                    format!(
                        "The deepest dependency chain is {max_depth} requests long — each level waits for the one before it, so on a high-latency network they serialize into {max_depth} sequential round-trips.{chain_line}"
                    ),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation("Preload the chain's head (rel=preload / modulepreload), inline tiny critical resources, and avoid deep import() chains for above-the-fold content."),
            );
        }
    }

    // ---- Unused JavaScript (precise coverage) --------------------------------------
    {
        let total: f64 = cap.js_coverage.iter().map(|(_, t, _)| t).sum();
        let unused: f64 = cap.js_coverage.iter().map(|(_, _, u)| u).sum();
        if unused > 100.0 * 1024.0 && total > 0.0 {
            report.metrics.push(Metric::new(
                "unused JS",
                unused / 1024.0,
                "KB",
                MetricStatus::Warn,
            ));
            let mut worst: Vec<&(String, f64, f64)> = cap
                .js_coverage
                .iter()
                .filter(|(_, _, u)| *u > 0.0)
                .collect();
            worst.sort_by(|a, b| b.2.total_cmp(&a.2));
            let top = worst
                .iter()
                .take(3)
                .map(|(u, t, un)| {
                    format!(
                        "{} ({}% of {})",
                        short_url(u),
                        (un / t.max(1.0) * 100.0) as i64,
                        human_bytes(*t)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let pct = (unused / total * 100.0) as i64;
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.unused_javascript",
                    format!(
                        "{} of JS never executed ({pct}% of {}) — est. save ~{}",
                        human_bytes(unused),
                        human_bytes(total),
                        human_bytes(unused)
                    ),
                    if !dev_mode && unused > 512.0 * 1024.0 {
                        Severity::Medium
                    } else {
                        Severity::Low
                    },
                    Confidence::High,
                    Surface::Runtime,
                    format!(
                        "Of {} of JavaScript downloaded, {} ({pct}%) was never run during the session.\n  worst: {top}",
                        human_bytes(total),
                        human_bytes(unused)
                    ),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation("Code-split by route so only needed JS loads; tree-shake dead exports; drop unused deps. (Coverage is per-session — exercise more routes for fuller numbers.)"),
            );
        }
    }

    // ---- Main-thread time by script (sampling CPU profile) -------------------------
    if let Some((top_url, top_ms)) = cap.cpu_by_script.first() {
        if *top_ms > 200.0 {
            let top = cap
                .cpu_by_script
                .iter()
                .take(3)
                .map(|(u, ms)| format!("{} {ms:.0}ms", short_url(u)))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(
                Finding::new(
                    Domain::Performance,
                    "web.heavy_script_execution",
                    format!("{} dominates main-thread JS ({top_ms:.0} ms)", short_url(top_url)),
                    Severity::Low,
                    Confidence::Medium,
                    Surface::Runtime,
                    format!(
                        "Top scripts by main-thread execution time: {top}. Long JS execution blocks interaction."
                    ),
                )
                .with_location(Location::target(url.to_string()))
                .with_remediation("Split the hot script, move heavy work to a web worker, memoize, and lazy-init non-critical code."),
            );
        }
    }

    // ---- Measurement context (honest framing — dev/prod + throttle state) ----------
    {
        let build = if cap.prod {
            "a PRODUCTION build (your production preview, via --prod)"
        } else if dev_mode {
            "a DEV build (unbundled/uncompressed by design)"
        } else {
            "a build with no dev-server markers (treated as production-like)"
        };
        let net = if cap.throttled {
            "throttled to slow-4G + 4× CPU (field-representative)"
        } else {
            "UNTHROTTLED on localhost — best-case numbers, NOT field-representative"
        };
        // Steer the user to the most trustworthy next step: a dev run should be re-measured
        // against a prod preview (--prod); a prod run that's unthrottled should add --throttle;
        // a throttled prod run is the gold standard — no caveat.
        let advice = if dev_mode {
            " For a real-user verdict, start your production preview server and re-measure it with --prod (add --lab --throttle for field conditions)."
        } else if !cap.throttled {
            " Re-run with --lab --throttle for field-representative numbers (slow-4G + 4× CPU)."
        } else {
            ""
        };
        findings.push(
            Finding::new(
                Domain::Performance,
                "web.measurement_context",
                format!("Measured {build}, {net}"),
                Severity::Info,
                Confidence::Certain,
                Surface::Runtime,
                format!(
                    "How to read these numbers: this was {build}; network/CPU was {net}.{advice}"
                ),
            )
            .with_location(Location::target(url.to_string())),
        );
    }

    // ---- Time series for the report's line charts (cumulative over the session) ----
    {
        let mut by_start: Vec<&Req> = all.clone();
        by_start.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
        let req_pts: Vec<[f64; 2]> = by_start
            .iter()
            .enumerate()
            .map(|(i, r)| [r.start_ms, (i + 1) as f64])
            .collect();
        if req_pts.len() >= 2 {
            report.series.push(Series {
                name: "req_timeline".into(),
                unit: "reqs".into(),
                points: req_pts,
            });
        }
        let mut by_end: Vec<&Req> = all.iter().copied().filter(|r| r.bytes > 0.0).collect();
        by_end.sort_by(|a, b| a.end_ms.total_cmp(&b.end_ms));
        let mut cum = 0.0;
        let byte_pts: Vec<[f64; 2]> = by_end
            .iter()
            .map(|r| {
                cum += r.bytes;
                [r.end_ms, cum / 1024.0]
            })
            .collect();
        if byte_pts.len() >= 2 {
            report.series.push(Series {
                name: "byte_timeline".into(),
                unit: "KB".into(),
                points: byte_pts,
            });
        }
    }

    (report, findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn req(kind: &str, start: f64, end: f64) -> Req {
        Req {
            url: format!("https://x/{kind}-{start}"),
            method: "GET".into(),
            status: 200,
            kind: kind.into(),
            start_ms: start,
            end_ms: end,
            failed: false,
            bytes: 1000.0,
            encoding: String::new(),
            cache_control: String::new(),
            has_validator: false,
            from_cache: false,
            ttfb_ms: 0.0,
            initiator: String::new(),
        }
    }

    fn nav_with(reqs: Vec<Req>) -> Nav {
        let mut m = HashMap::new();
        for (i, r) in reqs.into_iter().enumerate() {
            m.insert(format!("r{i}"), r);
        }
        Nav {
            url: "https://x/page".into(),
            started_ms: 0.0,
            loaded_ms: 0.0,
            reqs: m,
            vitals: Value::Null,
        }
    }

    #[test]
    fn nav_load_excludes_think_time_request() {
        // A lone request firing 3s after nav start = the user clicked away (the next
        // page's request leaking into this segment). It must NOT count as load time.
        let nav = nav_with(vec![req("doc", 3000.0, 3100.0)]);
        let (span, window) = nav_load(&nav, 800.0);
        assert!(
            span < 100.0,
            "think-time must not inflate the span (got {span})"
        );
        assert!(window.is_empty());
    }

    #[test]
    fn nav_load_includes_contiguous_burst() {
        let nav = nav_with(vec![
            req("doc", 0.0, 200.0),
            req("script", 100.0, 600.0),
            req("script", 500.0, 1200.0),
        ]);
        let (span, window) = nav_load(&nav, 800.0);
        assert_eq!(window.len(), 3);
        assert!((span - 1200.0).abs() < 1.0, "got {span}");
    }

    #[test]
    fn nav_load_stops_at_idle_gap() {
        // Two-request burst, then a >800ms gap, then a straggler → straggler excluded.
        let nav = nav_with(vec![
            req("doc", 0.0, 300.0),
            req("script", 200.0, 500.0),
            req("api", 2000.0, 2100.0),
        ]);
        let (span, window) = nav_load(&nav, 800.0);
        assert_eq!(window.len(), 2, "only the initial burst is the load");
        assert!((span - 500.0).abs() < 1.0, "got {span}");
    }

    #[test]
    fn slow_cause_classifies() {
        let waterfall: Vec<Req> = (0..120)
            .map(|i| req("script", i as f64, i as f64 + 5.0))
            .collect();
        assert_eq!(
            slow_cause(&waterfall.iter().collect::<Vec<_>>()).0,
            "module waterfall"
        );

        let server = [req("api", 0.0, 1200.0)];
        assert_eq!(
            slow_cause(&server.iter().collect::<Vec<_>>()).0,
            "slow server response"
        );

        let images = [req("img", 0.0, 600.0), req("img", 0.0, 600.0)];
        assert_eq!(
            slow_cause(&images.iter().collect::<Vec<_>>()).0,
            "heavy images"
        );
    }

    #[test]
    fn failing_vital_carries_cause_and_remediation() {
        use voltiq_core::Severity;
        // A nav whose worst INP is presentation-dominated on a specific element, plus a real
        // layout shift and a fast LCP — the two FAILURES must become self-explanatory
        // headline findings (cause + fix), and their standalone breakdown findings must be
        // suppressed; the PASSING vital keeps its informational element finding.
        let mut nav = nav_with(vec![req("doc", 0.0, 100.0)]);
        nav.vitals = json!({
            "inp": 632.0, "inp_input": 0.0, "inp_proc": 0.0, "inp_present": 632.0,
            "inp_type": "keydown", "inp_target": "textarea.vai-input",
            "cls": 0.30, "cls_el": "div.hero",
            "lcp": 800.0, "lcp_el": "p.title"
        });
        let cap = Capture {
            navs: vec![nav],
            ..Default::default()
        };
        let (_perf, findings) = build_report("http://localhost:8786/", &cap);
        let find = |rule: &str| findings.iter().find(|f| f.rule_id == rule);

        // INP failed (632 > 500ms poor): High, names the interaction + element + phase, has a fix.
        let inp = find("web.inp").expect("web.inp should fire");
        assert_eq!(inp.severity, Severity::High);
        assert!(
            inp.description.contains("keydown") && inp.description.contains("textarea.vai-input"),
            "INP desc must name the interaction: {}",
            inp.description
        );
        assert!(
            inp.description.contains("presentation"),
            "INP desc must name the dominant phase: {}",
            inp.description
        );
        assert!(inp.remediation.is_some(), "failing INP must carry a fix");
        assert!(
            find("web.inp_breakdown").is_none(),
            "breakdown must not duplicate a diagnosed failure"
        );

        // CLS failed (0.30 > 0.25 poor): names the culprit element, has a fix, no duplicate.
        let cls = find("web.cls").expect("web.cls should fire");
        assert!(
            cls.description.contains("div.hero"),
            "CLS desc must name the culprit: {}",
            cls.description
        );
        assert!(cls.remediation.is_some());
        assert!(find("web.layout_shift_culprit").is_none());

        // LCP passed (800 < 2500ms): no headline finding, but its element is still reported.
        assert!(find("web.lcp").is_none());
        assert!(
            find("web.lcp_element").is_some(),
            "a passing LCP still reports its element informationally"
        );
    }

    fn jsreq(url: &str, bytes: f64, init: &str) -> Req {
        Req {
            url: url.into(),
            method: "GET".into(),
            status: 200,
            kind: "script".into(),
            start_ms: 0.0,
            end_ms: 10.0,
            failed: false,
            bytes,
            encoding: String::new(),
            cache_control: String::new(),
            has_validator: false,
            from_cache: false,
            ttfb_ms: 0.0,
            initiator: init.into(),
        }
    }

    #[test]
    fn waterfall_and_chain_name_culprit_files() {
        // A 7-deep initiator chain (doc → a → b → c → d → e → f) ...
        let mut reqs = vec![
            jsreq("https://x/doc", 10.0, ""),
            jsreq("https://x/a.js", 10.0, "https://x/doc"),
            jsreq("https://x/b.js", 10.0, "https://x/a.js"),
            jsreq("https://x/c.js", 10.0, "https://x/b.js"),
            jsreq("https://x/d.js", 10.0, "https://x/c.js"),
            jsreq("https://x/e.js", 10.0, "https://x/d.js"),
            jsreq("https://x/f.js", 10.0, "https://x/e.js"),
        ];
        // ... plus enough modules to trip the waterfall, three of them heavy.
        for i in 0..110 {
            let bytes = if i < 3 { 500_000.0 } else { 1000.0 };
            reqs.push(jsreq(&format!("https://x/m{i}.js"), bytes, ""));
        }
        let (_perf, findings) = build_report(
            "https://x/",
            &Capture {
                navs: vec![nav_with(reqs)],
                ..Default::default()
            },
        );
        let find = |rule: &str| findings.iter().find(|f| f.rule_id == rule);

        let wf = find("web.request_waterfall").expect("waterfall should fire");
        assert!(
            wf.description.contains("heaviest modules:"),
            "waterfall must name the heaviest files: {}",
            wf.description
        );

        let chain = find("web.critical_chain").expect("critical chain should fire");
        assert!(
            chain.description.contains("chain:") && chain.description.contains('→'),
            "chain must show the actual file path: {}",
            chain.description
        );
    }

    #[test]
    fn prod_flag_flips_framing_and_severity() {
        use voltiq_core::Severity;
        // An un-bundled-looking waterfall, but asserted as a PRODUCTION build (--prod):
        // it must be framed as production and NOT downgraded as expected dev behavior.
        let reqs: Vec<Req> = (0..110)
            .map(|i| jsreq(&format!("https://x/m{i}.js"), 1000.0, ""))
            .collect();
        let cap = Capture {
            navs: vec![nav_with(reqs)],
            prod: true,
            ..Default::default()
        };
        let (_perf, findings) = build_report("https://x/", &cap);
        let find = |rule: &str| findings.iter().find(|f| f.rule_id == rule);

        let ctx = find("web.measurement_context").expect("context finding");
        assert!(
            ctx.title.contains("PRODUCTION"),
            "prod run must be framed as production: {}",
            ctx.title
        );
        let wf = find("web.request_waterfall").expect("waterfall");
        assert_eq!(
            wf.severity,
            Severity::Medium,
            "a waterfall in a PROD build is a real issue, not a downgraded dev artifact"
        );
        assert!(
            !wf.description.contains("looks like a dev server"),
            "no dev-server hint under --prod: {}",
            wf.description
        );
    }

    #[test]
    fn culprit_lists_do_not_repeat_a_file() {
        // The SAME heavy module fetched twice (e.g. on two navigations) must appear ONCE in
        // the "heaviest modules" list — not as a fake duplicate (the bug the user spotted).
        let mut reqs = vec![
            jsreq("https://x/deps/big.js", 3_000_000.0, ""),
            jsreq("https://x/deps/big.js", 3_000_000.0, ""), // same URL, fetched again
            jsreq("https://x/deps/second.js", 2_000_000.0, ""),
        ];
        for i in 0..110 {
            reqs.push(jsreq(&format!("https://x/m{i}.js"), 1000.0, ""));
        }
        let (_perf, findings) = build_report(
            "https://x/",
            &Capture {
                navs: vec![nav_with(reqs)],
                ..Default::default()
            },
        );
        let wf = findings
            .iter()
            .find(|f| f.rule_id == "web.request_waterfall")
            .expect("waterfall");
        let line = wf
            .description
            .lines()
            .find(|l| l.contains("heaviest modules"))
            .expect("heaviest line");
        assert_eq!(
            line.matches("big.js").count(),
            1,
            "a re-fetched module must not be listed twice: {line}"
        );
    }
}
