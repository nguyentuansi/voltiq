import { createQuery } from "@tanstack/svelte-query";

import type { Report } from "./types";

declare global {
  // Set by the static `voltiq audit --html` export to inline the report.
  // eslint-disable-next-line no-var
  var __VOLTIQ_REPORT__: Report | undefined;
}

/** Fetch the report: use the inlined global if present (static export), else the
 *  local API (live `voltiq serve`), polling for live updates. */
export function useReport() {
  return createQuery<Report>(() => ({
    queryKey: ["report"],
    queryFn: async (): Promise<Report> => {
      if (typeof globalThis.__VOLTIQ_REPORT__ !== "undefined") {
        return globalThis.__VOLTIQ_REPORT__ as Report;
      }
      const res = await fetch("/api/report");
      if (!res.ok) throw new Error(`report request failed: ${res.status}`);
      return (await res.json()) as Report;
    },
    refetchInterval: 5000,
  }));
}
