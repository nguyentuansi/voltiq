use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::severity::Severity;

/// Which engine produced a finding.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Domain {
    Security,
    Performance,
}

/// Where a finding was observed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Source files in the working tree.
    Source,
    /// Git history (a secret added then removed is still recoverable).
    GitHistory,
    /// Built browser bundle shipped to clients (`.next/static`, `dist/`, `build/`).
    ClientBundle,
    /// `.env*` / config files.
    Env,
    /// Secrets reaching logs / telemetry sinks.
    Logs,
    /// A running process / load test (perf findings, leaks).
    Runtime,
    /// Dependency manifests / lockfiles.
    Dependency,
}

/// Confidence that a finding is real (maps to `@landing-v/ui` `CONFIDENCE_COLORS`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
    Certain,
}

/// Where in the target a finding lives. A `file`+`line` for static findings, or a
/// `target` (pid / url / process descriptor) for runtime findings.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Location {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Location {
    pub fn file(path: impl Into<String>, line: u32) -> Self {
        Location {
            file: Some(path.into()),
            line: Some(line),
            ..Default::default()
        }
    }

    pub fn target(t: impl Into<String>) -> Self {
        Location {
            target: Some(t.into()),
            ..Default::default()
        }
    }
}

/// A single issue surfaced by an engine.
///
/// `evidence` MUST be redacted before construction — use
/// [`Finding::with_evidence_redacted`], never store a raw secret.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Finding {
    /// Stable id derived from `rule_id` + location, for dedup across runs.
    pub id: String,
    pub domain: Domain,
    /// Dotted rule identifier, e.g. `secret.aws_access_key`, `perf.event_loop_lag`.
    pub rule_id: String,
    pub title: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub surface: Surface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// Free-form extra context (provider, env var name, metric trend ref, …).
    /// This is where the AI layer attaches its "what to investigate" hints.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
}

impl Finding {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        domain: Domain,
        rule_id: impl Into<String>,
        title: impl Into<String>,
        severity: Severity,
        confidence: Confidence,
        surface: Surface,
        description: impl Into<String>,
    ) -> Self {
        let mut f = Finding {
            id: String::new(),
            domain,
            rule_id: rule_id.into(),
            title: title.into(),
            severity,
            confidence,
            surface,
            location: None,
            evidence: None,
            description: description.into(),
            remediation: None,
            metadata: BTreeMap::new(),
        };
        f.id = f.compute_id();
        f
    }

    pub fn with_location(mut self, loc: Location) -> Self {
        self.location = Some(loc);
        self.id = self.compute_id();
        self
    }

    /// Attach evidence, redacting the raw secret first.
    pub fn with_evidence_redacted(mut self, raw_secret: &str) -> Self {
        self.evidence = Some(crate::redact::redact_secret(raw_secret));
        self
    }

    /// Attach already-safe (non-secret) evidence text verbatim.
    pub fn with_evidence_safe(mut self, text: impl Into<String>) -> Self {
        self.evidence = Some(text.into());
        self
    }

    pub fn with_remediation(mut self, r: impl Into<String>) -> Self {
        self.remediation = Some(r.into());
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    fn compute_id(&self) -> String {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.rule_id.hash(&mut h);
        if let Some(loc) = &self.location {
            loc.file.hash(&mut h);
            loc.line.hash(&mut h);
            loc.target.hash(&mut h);
        }
        format!("{:016x}", h.finish())
    }
}
