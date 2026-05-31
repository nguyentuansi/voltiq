//! Git-history exposure. A secret (or a `.env`) committed once is recoverable from
//! history even after deletion.
//!
//! This uses the `git` CLI, which is present wherever a working repo is. A pure-`gix`
//! implementation can replace this later (the plan's intent) without changing the
//! finding shape — kept behind this module so the swap is local.

use std::path::Path;
use std::process::Command;

use voltiq_core::{Confidence, Domain, Finding, Location, Severity, Surface};

fn is_env_file(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

fn is_template(name: &str) -> bool {
    name.ends_with(".example") || name.ends_with(".sample") || name.ends_with(".template")
}

/// Flag real `.env*` files that were ever added to git history.
pub fn scan_history(root: &Path, findings: &mut Vec<Finding>) {
    if !root.join(".git").exists() {
        return;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "log",
            "--all",
            "--no-color",
            "--diff-filter=A",
            "--name-only",
            "--pretty=format:",
        ])
        .output();
    let Ok(output) = output else { return };
    if !output.status.success() {
        return;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut seen = std::collections::BTreeSet::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let base = line.rsplit('/').next().unwrap_or(line);
        if is_env_file(base) && !is_template(base) && seen.insert(line.to_string()) {
            findings.push(
                Finding::new(
                    Domain::Security,
                    "git.env_committed",
                    format!("{base} was committed to git history"),
                    Severity::High,
                    Confidence::High,
                    Surface::GitHistory,
                    "An environment file was added to git history; its secrets remain recoverable even if the file was later deleted.",
                )
                .with_location(Location {
                    file: Some(line.to_string()),
                    target: Some("git-history".into()),
                    ..Default::default()
                })
                .with_remediation(
                    "Rotate any secrets that were in this file, purge it from history (e.g. `git filter-repo`), and add it to .gitignore.",
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn detects_committed_env_even_after_deletion() {
        let dir = tempdir().unwrap();
        let p = dir.path();
        git(p, &["init", "-q"]);
        fs::write(p.join(".env"), "SECRET=x\n").unwrap();
        git(p, &["add", ".env"]);
        git(p, &["commit", "-q", "-m", "add env"]);
        fs::remove_file(p.join(".env")).unwrap();
        git(p, &["add", "-A"]);
        git(p, &["commit", "-q", "-m", "rm env"]);

        let mut findings = Vec::new();
        scan_history(p, &mut findings);
        assert!(
            findings.iter().any(|f| f.rule_id == "git.env_committed"),
            "expected git.env_committed: {findings:#?}"
        );
    }
}
