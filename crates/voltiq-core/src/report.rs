use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::finding::Finding;
use crate::metric::PerfReport;
use crate::severity::Severity;

/// Bump when the JSON shape changes incompatibly. The dashboard checks this.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

impl Default for ToolInfo {
    fn default() -> Self {
        ToolInfo {
            name: "voltiq".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }
    }
}

/// What was scanned/measured.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TargetInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
}

/// Roll-up counts and the pass/fail gate.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Summary {
    pub total_findings: usize,
    pub by_severity: BTreeMap<Severity, usize>,
    pub passed: bool,
}

/// The top-level artifact every engine produces and the dashboard/MCP consume.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Report {
    pub schema_version: u32,
    pub tool: ToolInfo,
    pub generated_at_unix_ms: u64,
    pub target: TargetInfo,
    pub summary: Summary,
    #[serde(default)]
    pub findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance: Option<PerfReport>,
    /// Analysis output: deterministic insights and (optionally) LLM suggestions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Report {
    pub fn new(target: TargetInfo) -> Self {
        Report {
            schema_version: SCHEMA_VERSION,
            tool: ToolInfo::default(),
            generated_at_unix_ms: crate::process::now_unix_ms(),
            target,
            summary: Summary::default(),
            findings: Vec::new(),
            performance: None,
            notes: Vec::new(),
        }
    }

    pub fn add_finding(&mut self, f: Finding) {
        self.findings.push(f);
    }

    pub fn extend_findings(&mut self, fs: impl IntoIterator<Item = Finding>) {
        self.findings.extend(fs);
    }

    /// Recompute counts and the pass/fail gate. The report fails if any finding has
    /// `severity >= fail_at`.
    pub fn recompute_summary(&mut self, fail_at: Severity) {
        let mut by_severity: BTreeMap<Severity, usize> = BTreeMap::new();
        for f in &self.findings {
            *by_severity.entry(f.severity).or_insert(0) += 1;
        }
        let passed = !self.findings.iter().any(|f| f.severity >= fail_at);
        self.summary = Summary {
            total_findings: self.findings.len(),
            by_severity,
            passed,
        };
    }
}
