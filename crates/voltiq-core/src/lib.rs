//! `voltiq-core` — the shared data model for voltiq.
//!
//! This crate defines the **report schema** that every other crate produces or
//! consumes: the Rust engines emit a [`Report`], the server serializes it to JSON
//! for the dashboard, the MCP adapter returns it as structured tool output, and the
//! renderers turn it into human text / JSON / SARIF.
//!
//! It also owns **redaction** ([`redact`]) — secrets must be redacted *before* they
//! are ever stored in a [`Finding`], so the tool can never leak what it finds.

pub mod finding;
pub mod metric;
pub mod process;
pub mod redact;
pub mod render;
pub mod report;
pub mod severity;

pub use finding::{Confidence, Domain, Finding, Location, Surface};
pub use metric::{LatencyStats, Metric, MetricStatus, PerfReport, Series};
pub use report::{Report, Summary, TargetInfo, ToolInfo, SCHEMA_VERSION};
pub use severity::Severity;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Confidence, Domain, Surface};

    #[test]
    fn schema_roundtrip() {
        let mut r = Report::new(TargetInfo {
            path: Some("fixtures/leaky".into()),
            ..Default::default()
        });
        let f = Finding::new(
            Domain::Security,
            "secret.aws_access_key",
            "AWS access key committed in source",
            Severity::Critical,
            Confidence::High,
            Surface::Source,
            "An AWS access key id was found hardcoded in source.",
        )
        .with_evidence_redacted("AKIA1234567890ABCDEF");
        r.add_finding(f);
        r.recompute_summary(Severity::High);

        let json = render::to_json(&r, false);
        let back: Report = serde_json::from_str(&json).expect("roundtrip");
        assert_eq!(back.summary.total_findings, 1);
        assert!(
            !back.summary.passed,
            "a critical finding must fail the report"
        );
        assert_eq!(back.findings[0].rule_id, "secret.aws_access_key");
    }

    #[test]
    fn redaction_hides_secret() {
        let red = redact::redact_secret("AKIA1234567890ABCDEF");
        assert!(!red.contains("1234567890"), "middle must be masked: {red}");
        assert!(
            red.starts_with("AKIA"),
            "prefix kept for recognizability: {red}"
        );
    }

    #[test]
    fn sarif_is_valid_json() {
        let r = Report::new(TargetInfo::default());
        let s = render::to_sarif(&r);
        let v: serde_json::Value = serde_json::from_str(&s).expect("sarif json");
        assert_eq!(v["version"], "2.1.0");
    }
}
