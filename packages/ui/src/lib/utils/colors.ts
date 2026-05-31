/**
 * Centralised colour palettes used across the console route pages and
 * dashboard widgets. Previously these maps were copy-pasted into
 * findings, http-records, scans, agentic-scan, oast-interactions, plus
 * SeverityChart / AgentSessionsPanel / FindingsBreakdown / ScanHistoryTable.
 *
 * Keep the keys lowercase. Anything not in a map falls back to
 * `var(--aim-text-muted)` at the call site (the helpers below default to
 * that when the lookup misses).
 */

// ── Scan + agent lifecycle status ────────────────────────────────────
// Superset of every map seen in the wild — scans uses all five keys,
// agentic-scan / AgentSessionsPanel only use a subset. Extra keys are
// harmless if unused.
export const STATUS_TINT: Record<string, string> = {
  running:   "var(--aim-success)",
  completed: "var(--aim-accent)",
  paused:    "var(--aim-tertiary)",
  failed:    "var(--aim-error)",
  pending:   "var(--aim-text-muted)",
};

// ── Finding severity ─────────────────────────────────────────────────
export const SEVERITY_COLORS: Record<string, string> = {
  critical: "#E53935",
  high:     "#EF5350",
  medium:   "#FFA726",
  low:      "#FFD54F",
  suspect:  "#AB47BC",
  info:     "#42A5F5",
};

export const SEVERITY_ORDER = [
  "critical", "high", "medium", "low", "suspect", "info",
] as const;

// ── Finding confidence ──────────────────────────────────────────────
// Canonical workbench tokens (`certain`/`firm`/`tentative`) plus the
// legacy `high`/`medium`/`low` aliases that the mock seed still uses.
export const CONFIDENCE_COLORS: Record<string, string> = {
  certain:   "#98bc37",
  firm:      "#68a8e4",
  tentative: "#f0c674",
  high:      "#98bc37",
  medium:    "#FFA726",
  low:       "#918175",
};

// ── HTTP method (request table) ─────────────────────────────────────
export const METHOD_COLORS: Record<string, string> = {
  GET:    "#98bc37",
  POST:   "#68a8e4",
  PUT:    "#FFA726",
  DELETE: "#E53935",
  PATCH:  "#68a8e4",
};

// ── HTTP response status-code tint ──────────────────────────────────
export function statusColor(code: number): string {
  if (code >= 500) return "#E53935";
  if (code >= 400) return "#FFA726";
  if (code >= 300) return "#68a8e4";
  return "#98bc37";
}

// ── Lookups with a fallback ─────────────────────────────────────────
const MUTED = "var(--aim-text-muted)";
export const statusTint     = (s: string) => STATUS_TINT[s]      ?? MUTED;
export const severityColor  = (s: string) => SEVERITY_COLORS[s]  ?? MUTED;
export const confidenceColor = (s: string) => CONFIDENCE_COLORS[s] ?? MUTED;
export const methodColor    = (m: string) => METHOD_COLORS[m]    ?? "var(--aim-text)";
