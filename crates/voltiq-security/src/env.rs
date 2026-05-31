//! `.env` exposure checks. The *values* inside `.env` files are caught by the source
//! scan (the walk includes dotfiles); this adds the structural risk: a real `.env`
//! that isn't gitignored can be committed and leaked.

use std::path::Path;

use ignore::gitignore::GitignoreBuilder;
use voltiq_core::{Confidence, Domain, Finding, Location, Severity, Surface};

fn is_env_file(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

fn is_template(name: &str) -> bool {
    name.ends_with(".example") || name.ends_with(".sample") || name.ends_with(".template")
}

/// Flag real `.env*` files that are not matched by the project's `.gitignore`.
pub fn scan_env(root: &Path, findings: &mut Vec<Finding>) {
    let mut builder = GitignoreBuilder::new(root);
    let _ = builder.add(root.join(".gitignore"));
    let gitignore = builder.build().ok();

    for file in crate::walk::walk_source_files(root) {
        let Some(name) = file.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_env_file(name) || is_template(name) {
            continue;
        }
        let ignored = gitignore
            .as_ref()
            .map(|gi| gi.matched(&file, false).is_ignore())
            .unwrap_or(false);
        if ignored {
            continue;
        }
        findings.push(
            Finding::new(
                Domain::Security,
                "env.file_not_gitignored",
                format!("{name} is not gitignored"),
                Severity::High,
                Confidence::High,
                Surface::Env,
                "An environment file that typically holds secrets is not matched by .gitignore, so it can be committed and leaked.",
            )
            .with_location(Location::file(file.display().to_string(), 1))
            .with_remediation(format!(
                "Add `{name}` (or `.env*`) to .gitignore and confirm it isn't already committed (check git history)."
            )),
        );
    }
}
