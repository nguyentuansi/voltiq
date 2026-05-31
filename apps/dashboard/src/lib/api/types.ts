// Mirrors the voltiq-core report schema (the JSON served at /api/report).

export type Severity = "info" | "low" | "medium" | "high" | "critical";

export interface Location {
  file?: string;
  line?: number;
  column?: number;
  target?: string;
}

export interface Finding {
  id: string;
  domain: "security" | "performance";
  rule_id: string;
  title: string;
  severity: Severity;
  confidence: string;
  surface: string;
  location?: Location;
  evidence?: string;
  description: string;
  remediation?: string;
}

export interface LatencyStats {
  min: number;
  mean: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
}

export interface Metric {
  name: string;
  value: number;
  unit: string;
  status: string;
}

export interface Series {
  name: string;
  unit: string;
  points: [number, number][];
}

export interface PerfReport {
  runtime?: string;
  startup_ms?: number;
  throughput_rps?: number;
  error_rate?: number;
  latency?: LatencyStats;
  metrics?: Metric[];
  series?: Series[];
}

export interface Report {
  schema_version: number;
  tool: { name: string; version: string };
  generated_at_unix_ms: number;
  target: { path?: string; command?: string; runtime?: string };
  summary: { total_findings: number; by_severity: Record<string, number>; passed: boolean };
  findings: Finding[];
  performance?: PerfReport;
}
