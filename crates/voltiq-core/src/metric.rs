use serde::{Deserialize, Serialize};

/// Pass/warn/fail status of a single performance metric, mirroring the dashboard's
/// status tints.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MetricStatus {
    Pass,
    Warn,
    Fail,
    Info,
}

/// A single scalar performance metric with an optional threshold it was judged against.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Metric {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub status: MetricStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
}

impl Metric {
    pub fn new(
        name: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        status: MetricStatus,
    ) -> Self {
        Metric {
            name: name.into(),
            value,
            unit: unit.into(),
            status,
            threshold: None,
        }
    }

    pub fn with_threshold(mut self, t: f64) -> Self {
        self.threshold = Some(t);
        self
    }
}

/// Latency distribution (milliseconds).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LatencyStats {
    pub min: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

/// A time series for trend charts (e.g. RSS over time). Each point is `[t_ms, value]`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Series {
    pub name: String,
    pub unit: String,
    pub points: Vec<[f64; 2]>,
}

/// The performance section of a [`crate::report::Report`].
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PerfReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    /// Time from spawn to first successful response (launch mode only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_rps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<LatencyStats>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metrics: Vec<Metric>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub series: Vec<Series>,
}
