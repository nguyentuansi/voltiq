use serde::{Deserialize, Serialize};

/// Severity of a finding.
///
/// Ordered ascending so that `Critical` is the greatest: deriving `Ord` lets us
/// gate a report on "any finding `>=` some threshold".
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }

    /// Hex color matching the `@landing-v/ui` `SEVERITY_COLORS` palette, so the
    /// dashboard and any terminal rendering agree.
    pub fn color(self) -> &'static str {
        match self {
            Severity::Critical => "#E53935",
            Severity::High => "#EF5350",
            Severity::Medium => "#FFA726",
            Severity::Low => "#FFD54F",
            Severity::Info => "#42A5F5",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
