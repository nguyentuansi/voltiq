//! Redaction — the tool must never leak the secrets it finds.
//!
//! Every secret value is passed through [`redact_secret`] before being stored in a
//! [`crate::finding::Finding`]. [`Redactor`] additionally scrubs known secret values
//! out of arbitrary text (log lines, code snippets) before they're shown.

/// Redact a secret for safe display: keep a short prefix/suffix for recognizability,
/// mask the middle. Values of 8 chars or fewer are fully masked.
pub fn redact_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let len = chars.len();
    if len <= 8 {
        return "*".repeat(len.max(1));
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[len - 2..].iter().collect();
    format!("{prefix}…{suffix} ({len} chars)")
}

/// Scrubs a set of known secret values out of arbitrary text.
#[derive(Debug, Default, Clone)]
pub struct Redactor {
    secrets: Vec<String>,
}

impl Redactor {
    pub fn new() -> Self {
        Redactor::default()
    }

    /// Register a raw secret value to scrub on sight.
    pub fn add(&mut self, secret: impl Into<String>) {
        let s = secret.into();
        if !s.is_empty() {
            self.secrets.push(s);
        }
    }

    /// Replace every registered secret in `text` with its redacted form.
    pub fn scrub(&self, text: &str) -> String {
        let mut out = text.to_string();
        for s in &self.secrets {
            if !s.is_empty() && out.contains(s.as_str()) {
                out = out.replace(s.as_str(), &redact_secret(s));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_secret_fully_masked() {
        assert_eq!(redact_secret("abc"), "***");
        assert_eq!(redact_secret("12345678"), "********");
    }

    #[test]
    fn scrub_replaces_known_secret() {
        let mut r = Redactor::new();
        r.add("AKIA1234567890ABCDEF");
        let scrubbed = r.scrub("key=AKIA1234567890ABCDEF end");
        assert!(!scrubbed.contains("1234567890"));
        assert!(scrubbed.contains("key=AKIA"));
    }
}
