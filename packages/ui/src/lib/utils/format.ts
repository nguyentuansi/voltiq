/**
 * Tiny formatters that several console pages and widgets duplicate.
 */

/** "May 28, 01:17" or "—" for nullish. */
export function fmtTime(iso?: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString(undefined, {
    month:  "short",
    day:    "2-digit",
    hour:   "2-digit",
    minute: "2-digit",
  });
}

/** "1.2s" or "3m 14s" or "—" for zero. */
export function fmtDur(ms: number): string {
  if (ms === 0) return "—";
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60_000).toFixed(0)}m ${Math.round((ms % 60_000) / 1000)}s`;
}
