<script lang="ts">
  const GH = "https://github.com/nguyentuansi/voltiq";

  // The three-step loop.
  const loop = [
    {
      n: "01",
      t: "Measure",
      c: "#68a8e4",
      d: "Drive your app with one command — a real browser, a load test, or a passive watch. voltiq captures Core Web Vitals, the full request waterfall, transfer/compression/cache, the main-thread split, memory, and secret exposure.",
    },
    {
      n: "02",
      t: "Flag",
      c: "#e8943a",
      d: "Deterministic rules turn raw signals into ranked findings with plain-language fixes — uncompressed assets, render loops, slow APIs, tail-latency, memory growth, leaked credentials. Issues a fast localhost run would otherwise hide.",
    },
    {
      n: "03",
      t: "AI fixes",
      c: "#7fd962",
      d: "Hand the findings to the AI agent already in your terminal (Claude Code / Codex) over MCP — or a BYO-key LLM. No API key needed for the local path: it reads the evidence and proposes concrete edits.",
    },
  ];

  // What it measures.
  const measures = [
    ["Core Web Vitals", "LCP / INP / CLS / FCP with per-phase breakdowns and the element/route at fault — not just a number.", "#7fd962"],
    ["Request waterfall", "Every request by type (script / api / css / img / font), categorized by cause when slow.", "#68a8e4"],
    ["Transfer & cache", "Bytes over time, uncompressed text/JS, and assets with no usable cache lifetime.", "#e8943a"],
    ["1st vs 3rd-party", "Where your page weight comes from — your server vs external fonts, analytics, embeds.", "#c397ff"],
    ["Main-thread time", "Scripting / layout / style split — why the page can't respond to clicks.", "#5fd7d7"],
    ["Memory & leaks", "Process-tree RSS over a run; flags sustained growth that hasn't plateaued — a likely leak.", "#fbb829"],
    ["Secret hygiene", "Leaked credentials, committed .env, git history — redacted by default.", "#ef2f27"],
    ["Client-bundle exposure", "Secrets shipped to the browser, Supabase service_role confusion, stray source-maps.", "#ef2f27"],
  ];
</script>

<svelte:head><title>voltiq — measure, flag, let AI fix</title></svelte:head>

<!-- ── Nav ── -->
<header
  class="sticky top-0 z-20 flex items-center justify-between px-6 py-4"
  style="background:var(--aim-bg);border-bottom:1px solid var(--aim-border)"
>
  <a href="#top" class="font-bold tracking-wider" style="letter-spacing:.08em">
    <span style="color:var(--aim-accent)">&gt;</span> VOLTIQ
  </a>
  <nav class="flex items-center gap-5 text-sm" style="color:var(--aim-text-muted)">
    <a href="#how" class="hidden sm:inline hover:opacity-80">how it works</a>
    <a href="#case" class="hidden sm:inline hover:opacity-80">use case</a>
    <a href="#measures" class="hidden sm:inline hover:opacity-80">measures</a>
    <a
      href={GH}
      class="rounded px-3 py-1.5"
      style="border:1px solid var(--aim-border);color:var(--aim-text)">GitHub →</a
    >
  </nav>
</header>

<main id="top" class="mx-auto" style="max-width:1080px;padding:0 24px">
  <!-- ── Hero ── -->
  <section class="rise" style="padding:84px 0 56px;text-align:center">
    <div
      class="inline-block rounded-full px-3 py-1 text-xs"
      style="border:1px solid var(--aim-border);color:var(--aim-accent);margin-bottom:24px"
    >
      AI-first · Rust · zero heavy install
    </div>
    <h1 style="font-size:clamp(2.1rem,5vw,3.4rem);line-height:1.12;font-weight:bold;margin:0 0 18px">
      Catch what your AI-generated app<br />
      <span style="color:var(--aim-accent)">regresses on</span> — before users do.
    </h1>
    <p
      class="mx-auto"
      style="max-width:640px;color:var(--aim-text-soft);font-size:1.05rem;line-height:1.7"
    >
      voltiq is a single cross-platform binary that measures the runtime health
      <span style="color:var(--aim-text)">and</span> secret hygiene of Node.js / Bun apps —
      then hands the findings to an AI agent to fix.
    </p>
    <div class="flex flex-wrap items-center justify-center gap-3" style="margin-top:30px">
      <a
        href="#how"
        class="rounded px-5 py-2.5 font-bold"
        style="background:var(--aim-accent);color:var(--aim-bg)">How it works</a
      >
      <a href={GH} class="rounded px-5 py-2.5" style="border:1px solid var(--aim-border)"
        >View source</a
      >
    </div>
    <pre
      class="mx-auto"
      style="max-width:560px;text-align:left;margin-top:40px;background:var(--aim-surface);border:1px solid var(--aim-border);border-radius:8px;padding:18px 20px;overflow:auto;font-size:.86rem;line-height:1.7"><span style="color:var(--aim-text-muted)"># measure a running app, open the dashboard</span>
<span style="color:var(--aim-accent)">$</span> voltiq web http://localhost:3000/ --interactive --serve
<span style="color:var(--aim-text-muted)"># let your AI agent read the findings &amp; suggest edits</span>
<span style="color:var(--aim-accent)">$</span> voltiq explain</pre>
  </section>

  <!-- ── The loop ── -->
  <section id="how" style="padding:48px 0">
    <h2 style="font-size:.82rem;letter-spacing:.1em;text-transform:uppercase;color:var(--aim-text-muted);text-align:center;margin-bottom:32px">
      Measure → Flag → AI fixes
    </h2>
    <div class="grid gap-4" style="grid-template-columns:repeat(auto-fit,minmax(260px,1fr))">
      {#each loop as s}
        <div
          class="rise"
          style="background:var(--aim-surface);border:1px solid var(--aim-border);border-radius:10px;padding:24px"
        >
          <div style="font-size:.8rem;color:{s.c};font-weight:bold;letter-spacing:.1em">{s.n}</div>
          <div style="font-size:1.25rem;font-weight:bold;margin:8px 0 10px">{s.t}</div>
          <p style="color:var(--aim-text-soft);font-size:.9rem;line-height:1.65">{s.d}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- ── Real-world case study ── -->
  <section id="case" style="padding:48px 0">
    <h2 style="font-size:.82rem;letter-spacing:.1em;text-transform:uppercase;color:var(--aim-text-muted);margin-bottom:8px">
      Real-world use case
    </h2>
    <h3 style="font-size:1.6rem;font-weight:bold;margin:0 0 8px">The request waterfall</h3>
    <p style="color:var(--aim-text-soft);max-width:720px;line-height:1.7;margin-bottom:24px">
      A dev build firing hundreds of un-bundled module requests per page loads fine on
      localhost — so the problem stays invisible until it ships and crawls on a real network.
      voltiq surfaces it anyway, and the AI agent acts on the fix.
    </p>
    <div class="grid gap-4" style="grid-template-columns:1fr 1fr">
      <div
        style="background:var(--aim-surface);border:1px solid var(--aim-error);border-radius:10px;padding:22px"
      >
        <div style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;color:var(--aim-error);font-weight:bold">
          flagged
        </div>
        <div style="font-size:2rem;font-weight:bold;margin:10px 0 4px">105+ <span style="font-size:1rem;color:var(--aim-text-muted)">requests / page</span></div>
        <div style="font-family:inherit;font-size:.82rem;color:var(--aim-text-soft);margin-top:10px;line-height:1.6">
          <span style="color:var(--aim-error);font-weight:bold">web.request_waterfall</span> —
          96 of them JS modules (700+ across an admin session). The classic signature of an
          un-bundled build.
        </div>
      </div>
      <div
        style="background:var(--aim-surface);border:1px solid var(--aim-accent);border-radius:10px;padding:22px"
      >
        <div style="font-size:.72rem;text-transform:uppercase;letter-spacing:.06em;color:var(--aim-accent);font-weight:bold">
          AI suggested fix
        </div>
        <div style="font-size:2rem;font-weight:bold;margin:10px 0 4px">→ a handful <span style="font-size:1rem;color:var(--aim-text-muted)">of chunks</span></div>
        <div style="font-size:.82rem;color:var(--aim-text-soft);margin-top:10px;line-height:1.6">
          "Ship a production build: bundling + code-splitting collapses hundreds of dev module
          requests into a handful of chunks (hundreds → tens)."
        </div>
      </div>
    </div>
    <p style="color:var(--aim-text-muted);font-size:.78rem;margin-top:14px;font-style:italic">
      The brief voltiq produces is consumed directly by Claude Code / Codex over MCP — no
      API key, no cost — so the agent locates the build config and proposes the change.
    </p>
  </section>

  <!-- ── What it measures ── -->
  <section id="measures" style="padding:48px 0">
    <h2 style="font-size:.82rem;letter-spacing:.1em;text-transform:uppercase;color:var(--aim-text-muted);margin-bottom:26px">
      What it measures
    </h2>
    <div class="grid gap-3" style="grid-template-columns:repeat(auto-fill,minmax(248px,1fr))">
      {#each measures as [t, d, c]}
        <div
          style="background:var(--aim-surface-2);border:1px solid var(--aim-border);border-radius:8px;padding:18px"
        >
          <div style="display:flex;align-items:center;gap:8px;margin-bottom:8px">
            <span style="width:8px;height:8px;border-radius:2px;background:{c};display:inline-block"></span>
            <span style="font-weight:bold;font-size:.95rem">{t}</span>
          </div>
          <p style="color:var(--aim-text-muted);font-size:.82rem;line-height:1.55">{d}</p>
        </div>
      {/each}
    </div>
  </section>

  <!-- ── Integrations ── -->
  <section style="padding:48px 0 64px">
    <div
      style="background:var(--aim-surface);border:1px solid var(--aim-border);border-radius:12px;padding:32px;text-align:center"
    >
      <h2 style="font-size:1.4rem;font-weight:bold;margin:0 0 10px">Built for the AI loop</h2>
      <p class="mx-auto" style="max-width:640px;color:var(--aim-text-soft);line-height:1.7">
        Plugged into Claude Code / Codex / Cursor over <b style="color:var(--aim-text)">MCP</b>, voltiq
        returns structured evidence the host agent reasons over — no key, no cost. Standalone, point
        <code style="color:var(--aim-accent)">--ai</code> at any OpenAI-compatible endpoint
        (LiteLLM, OpenRouter, Ollama). Always falls back to deterministic rules.
      </p>
      <div class="flex flex-wrap items-center justify-center gap-3" style="margin-top:24px">
        <a href={GH} class="rounded px-5 py-2.5 font-bold" style="background:var(--aim-accent);color:var(--aim-bg)">Get started on GitHub</a>
      </div>
    </div>
  </section>
</main>

<footer
  class="mx-auto flex flex-wrap items-center justify-between gap-3 px-6 py-7 text-xs"
  style="max-width:1080px;color:var(--aim-text-muted);border-top:1px solid var(--aim-border)"
>
  <span><span style="color:var(--aim-accent)">&gt;</span> voltiq · Apache-2.0 · pre-1.0</span>
  <span>AI-first performance + security scanner for Node.js / Bun</span>
</footer>
