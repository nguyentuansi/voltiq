//! `voltiq-security` — secret / credential / env-leak scanning.
//!
//! Offline by default (regex rule pack + Shannon entropy over source + env files).
//! Git-history and client-bundle surfaces and opt-in `verify` land in later tasks.

pub mod client_bundle;
pub mod entropy;
pub mod env;
pub mod git_history;
pub mod rules;
pub mod walk;

use std::path::Path;

use voltiq_core::{Domain, Finding, Location, Surface};

/// Options for a security scan.
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Opt-in: verify found secrets against provider APIs (network, read-only).
    pub verify: bool,
    /// Skip the git-history walk.
    pub no_git_history: bool,
}

/// Scan a directory tree for leaked secrets / credentials / env exposure across all
/// surfaces: source files, env files, built client bundles, and git history.
pub fn scan_path(path: &Path, opts: &ScanOptions) -> Vec<Finding> {
    let mut findings = Vec::new();
    // Source tree (skips deps + build outputs).
    for file in walk::walk_source_files(path) {
        if let Ok(content) = std::fs::read_to_string(&file) {
            scan_text(
                &file.display().to_string(),
                &content,
                Surface::Source,
                &mut findings,
            );
        }
    }
    // Structural env exposure.
    env::scan_env(path, &mut findings);
    // Secrets shipped into the browser bundle.
    client_bundle::scan_client_bundles(path, &mut findings);
    // Secrets / .env committed to git history.
    if !opts.no_git_history {
        git_history::scan_history(path, &mut findings);
    }
    findings
}

/// Apply the rule pack to one file's text, tagging each finding with `surface` and
/// pushing it (with redacted evidence).
pub fn scan_text(file: &str, content: &str, surface: Surface, findings: &mut Vec<Finding>) {
    for rule in rules::rules() {
        for caps in rule.regex.captures_iter(content) {
            let Some(whole) = caps.get(0) else { continue };
            let Some(secret) = caps.get(rule.group).or_else(|| caps.get(0)) else {
                continue;
            };
            let value = secret.as_str();
            if let Some(min) = rule.entropy_min {
                if value.len() < 16 || entropy::shannon_entropy(value) < min {
                    continue;
                }
            }
            let line = line_of(content, whole.start());
            findings.push(
                Finding::new(
                    Domain::Security,
                    rule.id,
                    rule.title,
                    rule.severity,
                    rule.confidence,
                    surface,
                    rule.description,
                )
                .with_location(Location::file(file, line))
                .with_remediation(rule.remediation)
                .with_evidence_redacted(value),
            );
        }
    }
}

/// 1-based line number for a byte offset into `content`.
fn line_of(content: &str, byte_off: usize) -> u32 {
    content
        .get(..byte_off)
        .map(|s| s.bytes().filter(|&b| b == b'\n').count() as u32 + 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn finds_aws_key_and_redacts() {
        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("config.js"),
            "const k = 'AKIAIOSFODNN7EXAMPLE';\n",
        )
        .unwrap();
        let findings = scan_path(dir.path(), &ScanOptions::default());
        let aws: Vec<_> = findings
            .iter()
            .filter(|f| f.rule_id == "secret.aws_access_key_id")
            .collect();
        assert_eq!(aws.len(), 1, "expected one AWS key, got {findings:#?}");
        let evidence = aws[0].evidence.as_deref().unwrap();
        assert!(
            !evidence.contains("IOSFODNN7"),
            "evidence must be redacted: {evidence}"
        );
        assert_eq!(aws[0].location.as_ref().unwrap().line, Some(1));
    }

    #[test]
    fn flags_env_not_gitignored() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env"), "DB_PASSWORD=hunter2\n").unwrap();
        let findings = scan_path(dir.path(), &ScanOptions::default());
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "env.file_not_gitignored"));
    }

    #[test]
    fn template_env_is_not_flagged() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".env.example"), "API_KEY=changeme\n").unwrap();
        let findings = scan_path(dir.path(), &ScanOptions::default());
        assert!(!findings
            .iter()
            .any(|f| f.rule_id == "env.file_not_gitignored"));
    }

    #[test]
    fn generic_rule_ignores_low_entropy() {
        let mut findings = Vec::new();
        // High-entropy random value -> flagged.
        scan_text(
            "a.ts",
            r#"const apiKey = "Zk9x2QpL7mWv3RtY8nUa1BcD";"#,
            Surface::Source,
            &mut findings,
        );
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "secret.generic_high_entropy"));

        // Low-entropy repetitive value -> not flagged by the generic rule.
        let mut findings2 = Vec::new();
        scan_text(
            "b.ts",
            r#"const apiKey = "aaaaaaaaaaaaaaaaaaaa";"#,
            Surface::Source,
            &mut findings2,
        );
        assert!(!findings2
            .iter()
            .any(|f| f.rule_id == "secret.generic_high_entropy"));
    }
}
