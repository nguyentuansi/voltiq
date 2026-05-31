//! `voltiq-server` — serves the embedded SvelteKit dashboard (rust-embed) plus a
//! JSON API the dashboard queries, and renders a self-contained themed HTML report.
//!
//! - `voltiq serve` → live dashboard (`GET /` SPA, `GET /api/report` data).
//! - `voltiq audit --html report.html` → [`render_html`], a portable single file.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, StatusCode, Uri};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use rust_embed::RustEmbed;
use voltiq_core::{Metric, MetricStatus, Report};

/// The dashboard's static build, embedded into the binary at compile time.
/// (The dashboard must be built first — see the repo Makefile `build-dashboard`.)
#[derive(RustEmbed)]
#[folder = "../../apps/dashboard/build"]
struct Assets;

#[derive(Clone)]
struct AppState {
    report: Arc<Report>,
}

/// Serve the dashboard + report at `addr`, blocking until the process is stopped.
pub fn serve_blocking(report: Report, addr: &str) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run(report, addr))
}

async fn run(report: Report, addr: &str) -> Result<(), String> {
    let state = AppState {
        report: Arc::new(report),
    };
    let app = Router::new()
        .route("/api/report", get(report_handler))
        .fallback(static_handler)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    println!("voltiq dashboard → http://{bound}   (Ctrl-C to stop)");
    axum::serve(listener, app).await.map_err(|e| e.to_string())
}

async fn report_handler(State(state): State<AppState>) -> Json<Report> {
    Json((*state.report).clone())
}

/// Serve an embedded asset; fall back to `index.html` so client-side routes resolve.
async fn static_handler(uri: Uri) -> Response {
    let raw = uri.path().trim_start_matches('/');
    let path = if raw.is_empty() { "index.html" } else { raw };

    if let Some(file) = Assets::get(path) {
        let ct = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        return ([(header::CONTENT_TYPE, ct)], file.data.into_owned()).into_response();
    }
    match Assets::get("index.html") {
        Some(file) => Html(String::from_utf8_lossy(&file.data).into_owned()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "dashboard assets not embedded — build apps/dashboard first",
        )
            .into_response(),
    }
}

// ── Report HTML: theme, scrollspy, and inline-SVG charts (all self-contained) ──────

const REPORT_CSS: &str = r#"
:root{--bg:rgb(28,27,25);--surface:rgb(39,37,32);--text:rgb(252,232,195);--muted:rgb(145,129,117);--accent:rgb(127,217,98);--border:rgb(46,43,38)}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--text);font:14px/1.5 "JetBrains Mono",ui-monospace,Menlo,Consolas,monospace}
a{color:inherit}
header{padding:14px 24px;border-bottom:1px solid var(--border);display:flex;align-items:baseline;gap:12px;flex-wrap:wrap;position:sticky;top:0;background:var(--bg);z-index:5}
header b{letter-spacing:.07em} .accent{color:var(--accent)} .dim{color:var(--muted)}
.badge{font-weight:bold;padding:2px 10px;border:1px solid var(--c);color:var(--c);border-radius:3px;font-size:.78rem}
.ctxbar{max-width:1240px;margin:0 auto;padding:8px 24px;color:#e8943a;font-size:.76rem;border-bottom:1px solid var(--border)}
.layout{display:flex;max-width:1240px;margin:0 auto;align-items:flex-start}
#spy{position:sticky;top:52px;align-self:flex-start;width:210px;flex:none;padding:22px 10px;max-height:calc(100vh - 52px);overflow:auto}
#spy a{display:block;padding:6px 12px;color:var(--muted);text-decoration:none;border-left:2px solid transparent;font-size:.78rem;letter-spacing:.03em}
#spy a:hover{color:var(--text)}
#spy a.active{color:var(--text);border-left-color:var(--accent);background:var(--surface)}
#spy .cnt{float:right;color:var(--muted);font-size:.72rem}
main{flex:1;min-width:0;padding:8px 28px 80px}
section{padding:22px 0;border-top:1px solid var(--border)}
section:first-child{border-top:none}
h2{font-size:.82rem;letter-spacing:.08em;text-transform:uppercase;color:var(--muted);font-weight:normal;margin:0 0 16px}
.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px}
.xaxis{display:flex;justify-content:space-between;font-size:.7rem;color:var(--muted);margin-top:5px}
.card{border:1px solid var(--border);padding:16px;text-align:center;border-radius:4px;background:var(--surface)}
.card .big{font-size:1.55rem;font-weight:bold;line-height:1.2}
.card .lbl{color:var(--muted);text-transform:uppercase;font-size:.66rem;margin-top:7px;letter-spacing:.04em}
.card .bud{color:var(--muted);font-size:.6rem;margin-top:6px;line-height:1.35}
.panel{border:1px solid var(--border);border-radius:4px;background:var(--surface);padding:16px 18px;margin-bottom:14px}
.rate{margin:14px 0}
.rl{display:flex;justify-content:space-between;font-size:.82rem;margin-bottom:3px}
.rbar{width:100%;height:auto;display:block}
.line{width:100%;height:120px;display:block;border-radius:2px}
.rs{font-size:.74rem;color:var(--muted);margin-top:2px;line-height:1.45}
.cols{display:flex;gap:14px;flex-wrap:wrap;align-items:stretch}
.cols>.panel{flex:1 1 280px;min-width:260px;margin-bottom:0}
.hint{color:var(--muted);font-size:.78rem;margin:-8px 0 16px;max-width:780px;line-height:1.55}
.ct{font-size:.74rem;color:var(--muted);margin-bottom:9px;text-transform:uppercase;letter-spacing:.04em}
.bars{display:flex;flex-direction:column;gap:7px}
.bar{display:flex;align-items:center;gap:10px;font-size:.8rem}
.bar .bl{width:110px;flex:none;text-align:right;color:var(--muted)}
.bar .bt{flex:1;background:var(--bg);height:14px;border-radius:2px;overflow:hidden}
.bar .bf{display:block;height:100%}
.bar .bv{width:54px;flex:none;text-align:right}
.stack{display:flex;height:22px;border-radius:3px;overflow:hidden;margin-bottom:12px}
.stack span{display:block;height:100%}
.donutwrap{display:flex;align-items:center;gap:24px;flex-wrap:wrap}
.donut{width:130px;height:130px;flex:none}
.legend{display:flex;flex-direction:column;gap:6px;font-size:.82rem}
.lg{display:flex;align-items:center;gap:8px}
.sw{width:11px;height:11px;border-radius:2px;flex:none}
.linewrap .rl{margin-bottom:6px}
table{width:100%;border-collapse:collapse}
th,td{text-align:left;padding:8px 10px;border-bottom:1px solid var(--border);vertical-align:top;font-size:.84rem}
th{color:var(--muted);font-weight:normal;text-transform:uppercase;font-size:.68rem}
.sev{color:var(--c);font-weight:bold;text-transform:uppercase;font-size:.7rem}
.desc{color:var(--muted);font-size:.76rem;margin-top:4px;line-height:1.45}
.rootcause{margin-top:6px;padding:5px 9px;border-left:2px solid var(--accent,#7fd962);background:rgba(127,217,98,.06);font-size:.78rem;line-height:1.5}
.rootcause b{display:block;color:var(--accent,#7fd962);font-weight:bold;text-transform:uppercase;font-size:.6rem;letter-spacing:.05em;margin-bottom:2px}
.fix{color:var(--muted);font-size:.72rem;margin-top:5px;line-height:1.45;opacity:.85}
@media(max-width:760px){#spy{display:none}main{padding:8px 16px 60px}}
"#;

const SPY_JS: &str = r#"<script>
(function(){
  var links=[].slice.call(document.querySelectorAll('#spy a'));
  var map={}; links.forEach(function(a){map[a.getAttribute('href').slice(1)]=a;});
  var io=new IntersectionObserver(function(es){
    es.forEach(function(e){ if(e.isIntersecting){ links.forEach(function(l){l.classList.remove('active');});
      var a=map[e.target.id]; if(a)a.classList.add('active'); }});
  },{rootMargin:'-12% 0px -78% 0px',threshold:0});
  document.querySelectorAll('main section').forEach(function(s){io.observe(s);});
})();
</script>"#;

fn status_color(s: MetricStatus) -> &'static str {
    match s {
        MetricStatus::Pass => "#7fd962",
        MetricStatus::Warn => "#e8943a",
        MetricStatus::Fail => "#ef2f27",
        MetricStatus::Info => "#8a8175",
    }
}

/// Color for a request-kind bar (`script reqs` → script).
fn kind_color(label: &str) -> &'static str {
    match label.trim_end_matches(" reqs") {
        "script" => "#68a8e4",
        "api" => "#7fd962",
        "css" => "#c397ff",
        "doc" => "#e8943a",
        "img" => "#fbb829",
        "font" => "#5fd7d7",
        _ => "#8a8175",
    }
}

fn fmt_metric(m: &Metric) -> String {
    if m.unit == "count" {
        format!("{:.0}", m.value)
    } else if m.unit.is_empty() {
        format!("{:.2}", m.value)
    } else {
        format!("{:.0} {}", m.value, m.unit)
    }
}

fn fmt_num(v: f64) -> String {
    if v < 10.0 {
        format!("{v:.2}")
    } else {
        format!("{v:.0}")
    }
}

fn human_kb(kb: f64) -> String {
    if kb >= 1024.0 {
        format!("{:.1} MB", kb / 1024.0)
    } else {
        format!("{kb:.0} KB")
    }
}

fn card(label: &str, val: &str, accent: &str) -> String {
    format!(
        "<div class=\"card\"><div class=\"big\" style=\"color:{accent}\">{val}</div><div class=\"lbl\">{}</div></div>",
        html_escape(label)
    )
}

/// A budgeted metric tile: value colored good/amber/poor + a threshold caption — the same
/// good-vs-poor treatment the Core-Web-Vitals bars get, for quantity tiles that otherwise
/// show a bare number with no sense of scale.
fn card_rated(label: &str, val: &str, accent: &str, caption: &str) -> String {
    format!(
        "<div class=\"card\"><div class=\"big\" style=\"color:{accent}\">{val}</div><div class=\"lbl\">{}</div><div class=\"bud\">{caption}</div></div>",
        html_escape(label)
    )
}

/// Good/poor budgets for the non-vital quantity tiles (transfer size, request count,
/// long-task time) so they read good-vs-poor like the vitals do. Returns the rating + a
/// caption. Transfer uses Lighthouse's byte budget (≤1.6 MB good, >4 MB poor); long-task
/// time uses the Total-Blocking-Time bands (200/600 ms); request count has no official
/// threshold, so it's a clearly-labelled production rule-of-thumb. These are FIELD /
/// production targets — an un-bundled dev build is expected to exceed them (the
/// measurement-context note explains why a red here on localhost isn't a field verdict).
fn budget(name: &str, v: f64) -> Option<(MetricStatus, String)> {
    let (good, poor, kind, note) = match name {
        "transfer" => (1600.0, 4096.0, "kb", "Lighthouse byte budget"),
        "requests" => (50.0, 100.0, "count", "prod rule-of-thumb"),
        "long-task time" => (200.0, 600.0, "ms", "≈ Total Blocking Time"),
        _ => return None,
    };
    let status = if v <= good {
        MetricStatus::Pass
    } else if v <= poor {
        MetricStatus::Warn
    } else {
        MetricStatus::Fail
    };
    let verdict = match status {
        MetricStatus::Pass => "good",
        MetricStatus::Warn => "needs work",
        _ => "poor",
    };
    let caption = match kind {
        "kb" => format!(
            "{verdict} — good ≤ {} · poor &gt; {} ({note})",
            human_kb(good),
            human_kb(poor)
        ),
        "ms" => format!("{verdict} — good ≤ {good:.0} ms · poor &gt; {poor:.0} ms ({note})"),
        _ => format!("{verdict} — good ≤ {good:.0} · poor &gt; {poor:.0} ({note})"),
    };
    Some((status, caption))
}

/// Official Core-Web-Vitals (and Lighthouse) good/poor breakpoints, for the rating bars.
fn cwv_thresholds(name: &str) -> Option<(f64, f64)> {
    Some(match name {
        "FCP" => (1800.0, 3000.0),
        "LCP" => (2500.0, 4000.0),
        "INP" => (200.0, 500.0),
        "CLS" => (0.1, 0.25),
        "TBT" => (200.0, 600.0),
        "SpeedIndex" => (3400.0, 5800.0),
        "TTI" => (3800.0, 7300.0),
        _ => return None,
    })
}

/// A Core-Web-Vital as a horizontal rating bar: green / amber / red zones, a value marker,
/// on-chart axis numbers (0 · good · poor · max + the current value), and a plain caption.
fn rating_bar(m: &Metric) -> Option<String> {
    let (good, poor) = cwv_thresholds(&m.name)?;
    let max = (poor * 1.5).max(m.value * 1.1).max(1e-6);
    let w = 460.0;
    let sx = |v: f64| (v / max * w).clamp(0.0, w);
    let (gx, px, mx) = (sx(good), sx(poor), sx(m.value));
    let val = if m.unit.is_empty() {
        format!("{:.3}", m.value)
    } else {
        format!("{:.0} {}", m.value, m.unit)
    };
    let unit = if m.unit.is_empty() { "" } else { "ms" };
    let vlx = mx.clamp(20.0, w - 20.0); // keep the value label inside the viewBox
    Some(format!(
        "<div class=\"rate\"><div class=\"rl\"><span>{name}</span><b style=\"color:{c}\">{val}</b></div>\
<svg viewBox=\"0 0 {w} 42\" class=\"rbar\">\
<rect x=\"0\" y=\"13\" width=\"{gx:.1}\" height=\"14\" fill=\"#7fd96233\"/>\
<rect x=\"{gx:.1}\" y=\"13\" width=\"{aw:.1}\" height=\"14\" fill=\"#e8943a33\"/>\
<rect x=\"{px:.1}\" y=\"13\" width=\"{rw:.1}\" height=\"14\" fill=\"#ef2f2733\"/>\
<line x1=\"{gx:.1}\" y1=\"11\" x2=\"{gx:.1}\" y2=\"29\" stroke=\"var(--border)\"/>\
<line x1=\"{px:.1}\" y1=\"11\" x2=\"{px:.1}\" y2=\"29\" stroke=\"var(--border)\"/>\
<rect x=\"{ml:.1}\" y=\"11\" width=\"2.4\" height=\"18\" fill=\"{c}\"/>\
<text x=\"{vlx:.1}\" y=\"8\" fill=\"{c}\" font-size=\"9\" text-anchor=\"middle\">{val}</text>\
<text x=\"2\" y=\"40\" fill=\"#918175\" font-size=\"8\">0</text>\
<text x=\"{gx:.1}\" y=\"40\" fill=\"#7fd962\" font-size=\"8\" text-anchor=\"middle\">{gv}</text>\
<text x=\"{px:.1}\" y=\"40\" fill=\"#e8943a\" font-size=\"8\" text-anchor=\"middle\">{pv}</text>\
<text x=\"{w}\" y=\"40\" fill=\"#918175\" font-size=\"8\" text-anchor=\"end\">{mxl}{unit}</text></svg>\
<div class=\"rs\">good ≤ {gv}{unit} · poor &gt; {pv}{unit} — {help}</div></div>",
        name = html_escape(&m.name),
        c = status_color(m.status),
        aw = px - gx,
        rw = w - px,
        ml = (mx - 1.2).max(0.0),
        gv = fmt_num(good),
        pv = fmt_num(poor),
        mxl = fmt_num(max),
        help = metric_help(&m.name),
    ))
}

/// One-line, plain-language explanation of a metric (for non-experts).
fn metric_help(name: &str) -> &'static str {
    match name {
        "FCP" => "First Contentful Paint: how soon the first text/image appears.",
        "LCP" => "Largest Contentful Paint: when the main content is visible — the page \"looks loaded\".",
        "INP" => "Interaction to Next Paint: how fast the page responds to a click/tap.",
        "CLS" => "Cumulative Layout Shift: how much the page jumps around while loading (0 = rock-steady).",
        "TBT" => "Total Blocking Time: how long the page was frozen while scripts ran.",
        "SpeedIndex" => "Speed Index: how quickly the visible page fills in.",
        "TTI" => "Time to Interactive: when the page is reliably ready for input.",
        _ => "",
    }
}

/// One-line, plain-language description of a whole section (tech + non-tech).
fn section_help(id: &str) -> &'static str {
    match id {
        "vitals" => "Google's Core Web Vitals — the user-experience scores Search ranks on. Each bar shows where this page lands: green = good, amber = needs work, red = poor.",
        "requests" => "Every file the page downloaded (scripts, styles, images, API calls). Fewer, smaller requests load faster — a huge script count usually means an un-bundled dev build. The curve shows requests accumulating as the page loads (a steep step = a burst fired at once). The tiles below are rated against production budgets — a dev build will exceed them.",
        "network" => "How much data was transferred and where it came from. 1st-party = your own server; 3rd-party = external (fonts, analytics, embeds). The curve shows bytes arriving over time. Transfer + long-task tiles are rated good/needs-work/poor against Lighthouse byte and Total-Blocking-Time budgets.",
        "mainthread" => "The browser runs your JavaScript, lays out the page and paints pixels on ONE \"main thread\". While it's busy the page can't respond to clicks/scroll — less time here = a smoother page. Scripting = running JS, Layout = positioning elements, Style = recalculating CSS.",
        "runtime" => "Server-side load test: how fast the app starts, how many requests/second it sustains, and its memory over time.",
        "findings" => "Concrete issues voltiq flagged (worst first), each with a plain-language fix you or an AI agent can act on.",
        _ => "",
    }
}

/// Horizontal labelled bars (request-type counts).
fn hbars(items: &[(String, f64, &'static str)]) -> String {
    let max = items
        .iter()
        .map(|(_, v, _)| *v)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let mut s = String::from("<div class=\"bars\">");
    for (label, v, color) in items {
        let pct = (v / max * 100.0).clamp(2.0, 100.0);
        s.push_str(&format!(
            "<div class=\"bar\"><span class=\"bl\">{}</span><span class=\"bt\"><span class=\"bf\" style=\"width:{pct:.1}%;background:{color}\"></span></span><span class=\"bv\">{v:.0}</span></div>",
            html_escape(label)
        ));
    }
    s.push_str("</div>");
    s
}

/// Two-or-more-segment donut (1st- vs 3rd-party transfer), with a KB legend.
fn donut(segs: &[(String, f64, &'static str)]) -> String {
    let total: f64 = segs.iter().map(|(_, v, _)| *v).sum();
    if total <= 0.0 {
        return String::new();
    }
    let circ = 2.0 * std::f64::consts::PI * 50.0;
    let mut off = 0.0;
    let mut arcs = String::new();
    let mut legend = String::from("<div class=\"legend\">");
    for (label, v, color) in segs {
        let len = v / total * circ;
        arcs.push_str(&format!(
            "<circle cx=\"60\" cy=\"60\" r=\"50\" fill=\"none\" stroke=\"{color}\" stroke-width=\"16\" stroke-dasharray=\"{len:.2} {rest:.2}\" stroke-dashoffset=\"{o:.2}\" transform=\"rotate(-90 60 60)\"/>",
            rest = circ - len,
            o = -off,
        ));
        off += len;
        legend.push_str(&format!(
            "<div class=\"lg\"><span class=\"sw\" style=\"background:{color}\"></span>{} <span class=\"dim\">{}</span></div>",
            html_escape(label),
            human_kb(*v)
        ));
    }
    legend.push_str("</div>");
    format!(
        "<div class=\"donutwrap\"><svg viewBox=\"0 0 120 120\" class=\"donut\"><circle cx=\"60\" cy=\"60\" r=\"50\" fill=\"none\" stroke=\"var(--border)\" stroke-width=\"16\"/>{arcs}</svg>{legend}</div>"
    )
}

/// One stacked horizontal bar (main-thread time split) + legend.
fn stacked(segs: &[(String, f64, &'static str)]) -> String {
    let total: f64 = segs.iter().map(|(_, v, _)| *v).sum();
    if total <= 0.0 {
        return String::new();
    }
    let mut bar = String::from("<div class=\"stack\">");
    let mut legend = String::from("<div class=\"legend\">");
    for (label, v, color) in segs {
        bar.push_str(&format!(
            "<span style=\"width:{:.1}%;background:{color}\" title=\"{} {v:.0} ms\"></span>",
            v / total * 100.0,
            html_escape(label)
        ));
        legend.push_str(&format!(
            "<div class=\"lg\"><span class=\"sw\" style=\"background:{color}\"></span>{} <span class=\"dim\">{v:.0} ms</span></div>",
            html_escape(label)
        ));
    }
    bar.push_str("</div>");
    legend.push_str("</div>");
    format!("{bar}{legend}")
}

/// A simple area/line chart for a time series (cumulative requests/bytes, RSS over time…),
/// with a one-line caption and a real time axis (0s → duration). Times are ms since the
/// session started.
fn line_chart(points: &[[f64; 2]], title: &str, unit: &str, caption: &str) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let (w, h, pad) = (460.0, 120.0, 6.0);
    let x0 = points.first().map(|p| p[0]).unwrap_or(0.0);
    let x1 = points.last().map(|p| p[0]).unwrap_or(1.0);
    let ymin = points.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    let ymax = points.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
    let xr = (x1 - x0).max(1.0);
    let yr = (ymax - ymin).max(1e-6);
    let sx = |x: f64| pad + (x - x0) / xr * (w - 2.0 * pad);
    let sy = |y: f64| (h - pad) - (y - ymin) / yr * (h - 2.0 * pad);
    let pts: String = points
        .iter()
        .map(|p| format!("{:.1},{:.1}", sx(p[0]), sy(p[1])))
        .collect::<Vec<_>>()
        .join(" ");
    let area = format!(
        "{:.1},{:.1} {pts} {:.1},{:.1}",
        pad,
        h - pad,
        w - pad,
        h - pad
    );
    let dur = (x1 - x0) / 1000.0; // seconds
    let mid = dur / 2.0;
    format!(
        "<div class=\"linewrap\"><div class=\"rl\"><span>{title}</span><b class=\"dim\">{ymin:.0} → {ymax:.0} {unit}</b></div>\
<div class=\"ct\" style=\"margin-bottom:7px\">{caption}</div>\
<svg viewBox=\"0 0 {w} {h}\" class=\"line\" preserveAspectRatio=\"none\"><polygon points=\"{area}\" fill=\"#7fd96222\"/>\
<polyline points=\"{pts}\" fill=\"none\" stroke=\"var(--accent)\" stroke-width=\"1.5\"/></svg>\
<div class=\"xaxis\"><span>0s</span><span>{mid:.1}s</span><span>{dur:.1}s</span></div>\
<div class=\"ct\" style=\"text-align:center;margin-top:2px\">time →</div></div>",
        title = html_escape(title),
        caption = html_escape(caption),
    )
}

/// Look up a named time series in the report's performance section.
fn series<'a>(report: &'a Report, name: &str) -> Option<&'a [[f64; 2]]> {
    report
        .performance
        .as_ref()?
        .series
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.points.as_slice())
}

fn findings_rows(report: &Report) -> String {
    let mut fs: Vec<&voltiq_core::Finding> = report.findings.iter().collect();
    fs.sort_by(|a, b| b.severity.cmp(&a.severity));
    let mut rows = String::new();
    for f in fs {
        let loc = f
            .location
            .as_ref()
            .map(|l| match (&l.file, l.line, &l.target) {
                (Some(file), Some(line), _) => format!("{file}:{line}"),
                (Some(file), None, _) => file.clone(),
                (None, _, Some(t)) => t.clone(),
                _ => String::new(),
            })
            .unwrap_or_default();
        // Split the description into summary prose (unindented) and the captured ROOT CAUSE
        // (the indented culprit lines — the specific files/elements with measured cost). The
        // root cause is highlighted; the generic remediation is demoted to a muted line. This
        // puts the actionable signal — WHICH files caused it — front and center, instead of
        // letting generic advice be the thing that stands out.
        let mut summary = String::new();
        let mut causes: Vec<&str> = Vec::new();
        let mut notes: Vec<&str> = Vec::new();
        for line in f.description.lines() {
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            if line.starts_with(char::is_whitespace) {
                if t.starts_with("NOTE:") {
                    notes.push(t);
                } else {
                    causes.push(t);
                }
            } else {
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(t);
            }
        }
        let desc = if summary.is_empty() {
            String::new()
        } else {
            format!("<div class=\"desc\">{}</div>", html_escape(&summary))
        };
        let root = if causes.is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"rootcause\"><b>root cause</b>{}</div>",
                causes
                    .iter()
                    .map(|&c| html_escape(c))
                    .collect::<Vec<_>>()
                    .join("<br>")
            )
        };
        let note = if notes.is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"desc\" style=\"opacity:.7\">{}</div>",
                notes
                    .iter()
                    .map(|&n| html_escape(n))
                    .collect::<Vec<_>>()
                    .join("<br>")
            )
        };
        // Generic, deterministic remediation — kept available, but demoted below the root cause.
        let fix = match &f.remediation {
            Some(r) if !r.trim().is_empty() => format!(
                "<div class=\"fix\">→ suggestion: {}</div>",
                html_escape(r).replace('\n', "<br>")
            ),
            _ => String::new(),
        };
        rows.push_str(&format!(
            "<tr><td><span class=\"sev\" style=\"--c:{color}\">{sev}</span></td>\
             <td class=\"dim\">{rule}</td><td>{loc}</td><td>{title}{desc}{root}{note}{fix}</td><td class=\"dim\">{surface:?}</td></tr>",
            color = f.severity.color(),
            sev = html_escape(f.severity.as_str()),
            rule = html_escape(&f.rule_id),
            loc = html_escape(&loc),
            title = html_escape(&f.title),
            surface = f.surface,
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=5 class=\"dim\">no findings — clean</td></tr>");
    }
    rows
}

/// Render a self-contained, themed HTML report (no external assets) with a scrollspy
/// sidebar, measurement points grouped by category, and inline-SVG charts.
pub fn render_html(report: &Report) -> String {
    let metrics: &[Metric] = report
        .performance
        .as_ref()
        .map(|p| p.metrics.as_slice())
        .unwrap_or(&[]);
    let find = |n: &str| metrics.iter().find(|m| m.name == n);
    let card_m = |m: &Metric| card(&m.name, &fmt_metric(m), status_color(m.status));
    // Like card_m, but if the metric has a good/poor budget (transfer / requests / long-task
    // time) the value is colored by that budget and a threshold caption is shown — so these
    // numbers read good-vs-poor like the vitals, instead of as context-free figures.
    let card_budget = |m: &Metric| match budget(&m.name, m.value) {
        Some((status, cap)) => card_rated(&m.name, &fmt_metric(m), status_color(status), &cap),
        None => card_m(m),
    };

    // (id, title, body) — only non-empty sections are shown / linked.
    let mut sections: Vec<(&str, String, usize)> = Vec::new();
    let mut bodies: Vec<String> = Vec::new();
    let mut push = |id: &'static str, title: &str, count: usize, body: String| {
        if !body.trim().is_empty() {
            sections.push((id, title.to_string(), count));
            bodies.push(body);
        }
    };

    // ── Core Web Vitals ──
    let mut vit = String::new();
    let rate: String = ["FCP", "LCP", "INP", "CLS", "TBT", "SpeedIndex", "TTI"]
        .iter()
        .filter_map(|n| find(n).and_then(rating_bar))
        .collect();
    if !rate.is_empty() {
        vit.push_str(&format!("<div class=\"panel\">{rate}</div>"));
    }
    let inp_break: String = ["INP input delay", "INP processing", "INP presentation"]
        .iter()
        .filter_map(|n| find(n))
        .map(card_m)
        .collect();
    if let Some(lh) = find("Lighthouse") {
        vit.push_str(&format!("<div class=\"grid\">{}</div>", card_m(lh)));
    }
    if !inp_break.is_empty() {
        vit.push_str(&format!(
            "<h2 style=\"margin-top:18px\">INP breakdown</h2><div class=\"grid\">{inp_break}</div>"
        ));
    }
    push(
        "vitals",
        "Core Web Vitals",
        rate.matches("class=\"rate\"").count(),
        vit,
    );

    // ── Requests ──
    let req_kinds: Vec<(String, f64, &'static str)> = metrics
        .iter()
        .filter(|m| m.name.ends_with(" reqs"))
        .map(|m| {
            (
                m.name.trim_end_matches(" reqs").to_string(),
                m.value,
                kind_color(&m.name),
            )
        })
        .collect();
    let mut reqs = String::new();
    if !req_kinds.is_empty() {
        let mut sorted = req_kinds.clone();
        sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
        let bars_panel = format!(
            "<div class=\"panel\"><div class=\"ct\">by type</div>{}</div>",
            hbars(&sorted)
        );
        // Second column: cumulative requests over the session.
        let line_panel = series(report, "req_timeline")
            .map(|p| {
                format!(
                    "<div class=\"panel\">{}</div>",
                    line_chart(
                        p,
                        "Requests over time",
                        "reqs",
                        "cumulative requests as the page loads"
                    )
                )
            })
            .unwrap_or_default();
        if line_panel.is_empty() {
            reqs.push_str(&bars_panel);
        } else {
            reqs.push_str(&format!(
                "<div class=\"cols\">{bars_panel}{line_panel}</div>"
            ));
        }
    }
    let req_cards: String = ["navigations", "requests"]
        .iter()
        .filter_map(|n| find(n))
        .map(card_budget)
        .collect();
    if !req_cards.is_empty() {
        reqs.push_str(&format!("<div class=\"grid\">{req_cards}</div>"));
    }
    push("requests", "Requests", req_kinds.len(), reqs);

    // ── Network weight ──
    let mut net = String::new();
    let first = find("1st-party").map(|m| m.value).unwrap_or(0.0);
    let third = find("3rd-party").map(|m| m.value).unwrap_or(0.0);
    let donut_panel = if first + third > 0.0 {
        format!(
            "<div class=\"panel\"><div class=\"ct\">1st vs 3rd-party</div>{}</div>",
            donut(&[
                ("1st-party".into(), first, "#7fd962"),
                ("3rd-party".into(), third, "#e8943a"),
            ])
        )
    } else {
        String::new()
    };
    // Second column: cumulative bytes over the session (the loading curve).
    let byte_line = series(report, "byte_timeline")
        .map(|p| {
            format!(
                "<div class=\"panel\">{}</div>",
                line_chart(
                    p,
                    "Bytes over time",
                    "KB",
                    "cumulative data downloaded — the loading curve"
                )
            )
        })
        .unwrap_or_default();
    if !donut_panel.is_empty() && !byte_line.is_empty() {
        net.push_str(&format!(
            "<div class=\"cols\">{donut_panel}{byte_line}</div>"
        ));
    } else {
        net.push_str(&donut_panel);
        net.push_str(&byte_line);
    }
    let net_cards: String = ["transfer", "3rd-party reqs", "long-task time"]
        .iter()
        .filter_map(|n| find(n))
        .map(card_budget)
        .collect();
    if !net_cards.is_empty() {
        net.push_str(&format!("<div class=\"grid\">{net_cards}</div>"));
    }
    push("network", "Network weight", 0, net);

    // ── Main thread ──
    let mut mt = String::new();
    let seg = |n: &str, c: &'static str| {
        find(n).map(|m| (n.trim_start_matches("main: ").to_string(), m.value, c))
    };
    let segs: Vec<(String, f64, &'static str)> = [
        seg("main: scripting", "#68a8e4"),
        seg("main: layout", "#c397ff"),
        seg("main: style", "#fbb829"),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !segs.is_empty() {
        mt.push_str(&format!("<div class=\"panel\">{}</div>", stacked(&segs)));
    }
    let mt_cards: String = ["main: task total", "JS heap", "DOM nodes"]
        .iter()
        .filter_map(|n| find(n))
        .map(card_m)
        .collect();
    if !mt_cards.is_empty() {
        mt.push_str(&format!("<div class=\"grid\">{mt_cards}</div>"));
    }
    push("mainthread", "Main thread", segs.len(), mt);

    // ── Runtime (load / watch) ──
    let mut rt = String::new();
    if let Some(p) = &report.performance {
        let mut cards = String::new();
        if let Some(v) = p.startup_ms {
            cards.push_str(&card("startup ms", &format!("{v:.0}"), "#68a8e4"));
        }
        if let Some(v) = p.throughput_rps {
            cards.push_str(&card("req / s", &format!("{v:.0}"), "#7fd962"));
        }
        if let Some(l) = &p.latency {
            cards.push_str(&card("p50 ms", &format!("{:.0}", l.p50), "#8a8175"));
            cards.push_str(&card("p99 ms", &format!("{:.0}", l.p99), "#e8943a"));
        }
        if let Some(e) = p.error_rate {
            let c = if e > 0.01 { "#ef2f27" } else { "#7fd962" };
            cards.push_str(&card("error rate", &format!("{:.1}%", e * 100.0), c));
        }
        for n in ["peak_rss", "peak_cpu"] {
            if let Some(m) = find(n) {
                cards.push_str(&card_m(m));
            }
        }
        if !cards.is_empty() {
            rt.push_str(&format!("<div class=\"grid\">{cards}</div>"));
        }
        if let Some(rss) = p.series.iter().find(|s| s.name == "rss") {
            let chart = line_chart(
                &rss.points,
                "RSS over time",
                &rss.unit,
                "resident memory during the run",
            );
            if !chart.is_empty() {
                rt.push_str(&format!(
                    "<div class=\"panel\" style=\"margin-top:14px\">{chart}</div>"
                ));
            }
        }
    }
    push("runtime", "Runtime", 0, rt);

    // ── Findings (always) ──
    let findings_body = format!(
        "<table><thead><tr><th>sev</th><th>rule</th><th>location</th><th>title</th><th>surface</th></tr></thead><tbody>{}</tbody></table>",
        findings_rows(report)
    );
    push(
        "findings",
        "Findings",
        report.summary.total_findings,
        findings_body,
    );

    // ── Assemble nav + sections ──
    let mut nav = String::new();
    let mut body = String::new();
    for ((id, title, count), b) in sections.iter().zip(bodies.iter()) {
        let cnt = if *count > 0 {
            format!("<span class=\"cnt\">{count}</span>")
        } else {
            String::new()
        };
        nav.push_str(&format!(
            "<a href=\"#{id}\">{}{cnt}</a>",
            html_escape(title)
        ));
        let hint = section_help(id);
        let hint_html = if hint.is_empty() {
            String::new()
        } else {
            format!("<p class=\"hint\">{}</p>", html_escape(hint))
        };
        body.push_str(&format!(
            "<section id=\"{id}\"><h2>{}</h2>{hint_html}{b}</section>",
            html_escape(title)
        ));
    }

    let gate_color = if report.summary.passed {
        "#7fd962"
    } else {
        "#ef2f27"
    };
    let target = html_escape(
        report
            .target
            .path
            .as_deref()
            .or(report.target.command.as_deref())
            .unwrap_or("—"),
    );
    // Honest framing: surface the measurement context (dev/prod + throttle) right next to
    // the gate, so a green "PASS" on a dev/unthrottled run isn't mistaken for a field verdict.
    let context = report
        .findings
        .iter()
        .find(|f| f.rule_id == "web.measurement_context")
        .map(|f| format!("<div class=\"ctxbar\">{}</div>", html_escape(&f.title)))
        .unwrap_or_default();

    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>voltiq report</title>
<style>{css}</style></head><body>
<header><span class="accent">&gt;</span> <b>VOLTIQ</b> <span class="dim">{target}</span>
<span class="dim">· {total} findings ·</span>
<span class="badge" style="--c:{gate_color}">{gate}</span></header>
{context}
<div class="layout"><nav id="spy">{nav}</nav><main>{body}</main></div>
{spy}
</body></html>"#,
        css = REPORT_CSS,
        target = target,
        total = report.summary.total_findings,
        gate = if report.summary.passed {
            "PASS"
        } else {
            "FAIL"
        },
        gate_color = gate_color,
        context = context,
        nav = nav,
        body = body,
        spy = SPY_JS,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Serve a single self-contained HTML page (e.g. [`render_html`]) on `addr`, opening the
/// user's browser at it, and block until `stop` flips true (Ctrl-C). Used by
/// `voltiq web --serve` to pop up a localhost dashboard for the run just captured.
pub fn serve_html_until(
    html: String,
    addr: &str,
    stop: Arc<std::sync::atomic::AtomicBool>,
    open: bool,
) -> Result<(), String> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    rt.block_on(run_html(html, addr, stop, open))
}

async fn run_html(
    html: String,
    addr: &str,
    stop: Arc<std::sync::atomic::AtomicBool>,
    open: bool,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    let html = Arc::new(html);
    let app = Router::new().route(
        "/",
        get({
            let h = html.clone();
            move || {
                let h = h.clone();
                async move { Html((*h).clone()) }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;
    let bound = listener.local_addr().map_err(|e| e.to_string())?;
    let url = format!("http://{bound}");
    println!("voltiq report dashboard → {url}   (Ctrl-C to stop)");
    if open {
        open_browser(&url);
    }
    let shutdown = async move {
        while !stop.load(Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    };
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| e.to_string())
}

/// Best-effort: open a URL in the user's default browser (no-op on failure).
pub fn open_browser(url: &str) {
    use std::process::{Command, Stdio};
    let mut cmd;
    #[cfg(target_os = "linux")]
    {
        cmd = Command::new("xdg-open");
        cmd.arg(url);
    }
    #[cfg(target_os = "macos")]
    {
        cmd = Command::new("open");
        cmd.arg(url);
    }
    #[cfg(target_os = "windows")]
    {
        cmd = Command::new("cmd");
        cmd.args(["/C", "start", "", url]);
    }
    let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use voltiq_core::{
        Confidence, Domain, Finding, PerfReport, Series, Severity, Surface, TargetInfo,
    };

    #[test]
    fn render_html_has_grouped_sections_charts_and_explanations() {
        let mut r = Report::new(TargetInfo {
            command: Some("web http://localhost:8786/".into()),
            ..Default::default()
        });
        let perf = PerfReport {
            metrics: vec![
                Metric::new("LCP", 696.0, "ms", MetricStatus::Pass).with_threshold(2500.0),
                Metric::new("CLS", 0.02, "", MetricStatus::Pass).with_threshold(0.1),
                Metric::new("INP input delay", 0.0, "ms", MetricStatus::Info),
                Metric::new("script reqs", 371.0, "count", MetricStatus::Info),
                Metric::new("api reqs", 19.0, "count", MetricStatus::Info),
                Metric::new("navigations", 12.0, "count", MetricStatus::Info),
                Metric::new("requests", 226.0, "count", MetricStatus::Info),
                Metric::new("transfer", 6916.0, "KB", MetricStatus::Info),
                Metric::new("long-task time", 206.0, "ms", MetricStatus::Info),
                Metric::new("1st-party", 32653.0, "KB", MetricStatus::Info),
                Metric::new("3rd-party", 1471.0, "KB", MetricStatus::Info),
                Metric::new("main: scripting", 222.0, "ms", MetricStatus::Info),
                Metric::new("main: layout", 135.0, "ms", MetricStatus::Info),
            ],
            series: vec![
                Series {
                    name: "req_timeline".into(),
                    unit: "reqs".into(),
                    points: vec![[0.0, 1.0], [100.0, 50.0], [400.0, 371.0]],
                },
                Series {
                    name: "byte_timeline".into(),
                    unit: "KB".into(),
                    points: vec![[0.0, 10.0], [200.0, 9000.0], [500.0, 34124.0]],
                },
            ],
            ..Default::default()
        };
        r.performance = Some(perf);
        r.add_finding(Finding::new(
            Domain::Performance,
            "web.measurement_context",
            "Measured a DEV build, UNTHROTTLED on localhost — not field-representative",
            Severity::Info,
            Confidence::Certain,
            Surface::Runtime,
            "context",
        ));
        r.add_finding(
            Finding::new(
                Domain::Performance,
                "web.inp",
                "INP 632 ms (worst across navigations)",
                Severity::High,
                Confidence::High,
                Surface::Runtime,
                // First line = summary; indented line = the captured root cause.
                "INP exceeded the 'good' threshold (good \u{2264} 200 ms).\n  Worst interaction: keydown on textarea.vai-input \u{2014} presentation dominates.",
            )
            .with_remediation("Reduce the layout/paint the interaction triggers."),
        );
        r.recompute_summary(Severity::High);
        let html = render_html(&r);

        assert!(html.starts_with("<!doctype html>"));
        assert!(html.trim_end().ends_with("</html>"));
        for needle in [
            "id=\"spy\"",
            "id=\"vitals\"",
            "id=\"requests\"",
            "id=\"network\"",
            "id=\"mainthread\"",
            "id=\"findings\"",
            "class=\"rate\"",           // CWV rating bar
            "class=\"donut\"",          // 1st/3rd-party donut
            "class=\"stack\"",          // main-thread stacked bar
            "class=\"bars\"",           // request-type bars
            "class=\"cols\"",           // 2-column (bar+line, donut+line)
            "Requests over time",       // request line chart
            "Bytes over time",          // network line chart
            "class=\"xaxis\"",          // line-chart time axis (0s → duration)
            "cumulative requests",      // line-chart caption
            "class=\"hint\"",           // per-section plain explanation
            "Largest Contentful Paint", // per-metric explanation
            "class=\"ctxbar\"",         // honest dev/throttle context banner
            "text-anchor=\"end\"",      // on-chart axis number (max)
            "IntersectionObserver",     // scrollspy
            "class=\"bud\"",            // budget caption on quantity tiles
            "Lighthouse byte budget",   // transfer tile good/poor framing
            "Total Blocking Time",      // long-task tile good/poor framing
            "class=\"fix\"",            // finding remediation is shown inline (demoted)
            "→ suggestion:",            // remediation prefix (relabeled from "fix")
            "keydown on textarea",      // root-cause evidence in a finding description
            "class=\"rootcause\"",      // captured root cause is highlighted
            ">root cause<",             // the root-cause label
        ] {
            assert!(html.contains(needle), "render_html missing `{needle}`");
        }
        // The budgeted tiles read good-vs-poor: 6916 KB transfer and 226 requests both blow
        // past their budgets (poor); 206 ms long-task is in the needs-work band. The poor
        // ones must render in the red status color, not the neutral grey.
        assert!(html.contains("poor — good ≤ 1.6 MB"), "transfer not rated poor");
        assert!(html.contains("needs work — good ≤ 200 ms"), "long-task not rated needs-work");
        // Contextual tiles (navigations) stay neutral — no budget caption smuggled in.
        assert!(
            !html.contains("navigations</div><div class=\"bud\""),
            "navigations should not be budget-rated"
        );
    }
}
