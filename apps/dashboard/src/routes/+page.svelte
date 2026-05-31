<script lang="ts">
  // Dashboard: fetch the voltiq report and render security + performance panels
  // using the @landing-v/ui design system and --aim-* theme.
  import { Stat, severityColor } from "@landing-v/ui";
  import { useReport } from "$api/report";
  import type { Finding, Report } from "$api/types";

  const q = useReport();
  const report = $derived(q.data as Report | undefined);
  const findings = $derived(report?.findings ?? []);
  const security = $derived(findings.filter((f) => f.domain === "security").length);
  const perfCount = $derived(findings.filter((f) => f.domain === "performance").length);

  function loc(f: Finding): string {
    const l = f.location;
    if (!l) return "";
    if (l.file) return l.line ? `${l.file}:${l.line}` : l.file;
    return l.target ?? "";
  }
</script>

<div class="flex flex-col gap-3">
  {#if q.isLoading}
    <div style="color:var(--aim-text-muted)">loading report…</div>
  {:else if q.error}
    <div style="color:var(--aim-error)">failed to load report: {String(q.error)}</div>
  {:else if report}
    <!-- KPI strip -->
    <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
      <div class="aim-card-enter aim-stagger-1">
        <Stat
          value={String(report.summary.total_findings)}
          label="total findings"
          accent={report.summary.passed ? "#7fd962" : "#ef2f27"}
        />
      </div>
      <div class="aim-card-enter aim-stagger-2">
        <Stat value={String(security)} label="security" accent="#E53935" />
      </div>
      <div class="aim-card-enter aim-stagger-3">
        <Stat value={String(perfCount)} label="performance" accent="#68a8e4" />
      </div>
      <div class="aim-card-enter aim-stagger-4">
        <Stat
          value={report.summary.passed ? "PASS" : "FAIL"}
          label="gate"
          accent={report.summary.passed ? "#7fd962" : "#ef2f27"}
        />
      </div>
    </div>

    {#if report.performance}
      <div class="grid grid-cols-2 gap-3 md:grid-cols-4">
        {#if report.performance.startup_ms != null}
          <Stat value={report.performance.startup_ms.toFixed(0)} label="startup ms" accent="#68a8e4" />
        {/if}
        {#if report.performance.throughput_rps != null}
          <Stat value={report.performance.throughput_rps.toFixed(0)} label="req / s" accent="#7fd962" />
        {/if}
        {#if report.performance.latency}
          <Stat
            value={report.performance.latency.p99.toFixed(1)}
            label="p99 latency ms"
            accent="#e8943a"
          />
        {/if}
        {#if report.performance.error_rate != null}
          <Stat
            value={(report.performance.error_rate * 100).toFixed(1) + "%"}
            label="error rate"
            accent="#fbb829"
          />
        {/if}
      </div>
    {/if}

    <!-- Findings table -->
    <div style="border:1px solid var(--aim-border)">
      <div
        class="px-3 py-2 text-xs uppercase"
        style="color:var(--aim-text-muted);border-bottom:1px solid var(--aim-border)"
      >
        findings
      </div>
      <table class="w-full text-xs">
        <thead>
          <tr style="color:var(--aim-text-muted)">
            <th class="px-3 py-1.5 text-left font-normal">sev</th>
            <th class="px-3 py-1.5 text-left font-normal">rule</th>
            <th class="px-3 py-1.5 text-left font-normal">location</th>
            <th class="px-3 py-1.5 text-left font-normal">title</th>
          </tr>
        </thead>
        <tbody>
          {#each findings as f (f.id)}
            <tr style="border-top:1px solid var(--aim-border)">
              <td class="px-3 py-1.5 font-bold uppercase" style="color:{severityColor(f.severity)}">
                {f.severity}
              </td>
              <td class="px-3 py-1.5" style="color:var(--aim-text-muted)">{f.rule_id}</td>
              <td class="px-3 py-1.5">{loc(f)}</td>
              <td class="px-3 py-1.5">{f.title}</td>
            </tr>
          {:else}
            <tr>
              <td colspan="4" class="px-3 py-3" style="color:var(--aim-text-muted)">
                no findings — clean
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>
