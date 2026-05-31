//! Client/browser-bundle exposure — the high-value, under-served surface.
//!
//! Built front-end output is downloaded by every visitor, so anything secret in it
//! is public. We scan the build dirs for: secret-rule matches, Supabase `service_role`
//! JWTs (the #1 vibe-code leak), public-env-prefixed secrets, and shipped source maps.

use std::path::Path;

use base64::Engine;
use voltiq_core::{Confidence, Domain, Finding, Location, Severity, Surface};
use walkdir::WalkDir;

/// Build-output directories whose contents ship to the browser.
const BUNDLE_DIRS: &[&str] = &["dist", "build", "out", ".next", ".svelte-kit", "public"];

/// File extensions worth scanning inside a bundle.
const SCAN_EXTS: &[&str] = &["js", "mjs", "cjs", "css", "html", "json", "map", "txt"];

const MAX_FILE_SIZE: u64 = 8 * 1024 * 1024;

/// Scan known build-output directories under `root`.
pub fn scan_client_bundles(root: &Path, findings: &mut Vec<Finding>) {
    for dir in BUNDLE_DIRS {
        let bundle = root.join(dir);
        if bundle.is_dir() {
            scan_bundle_dir(&bundle, findings);
        }
    }
}

fn scan_bundle_dir(dir: &Path, findings: &mut Vec<Finding>) {
    for entry in WalkDir::new(dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // Shipped source maps expose original source (and sometimes secrets).
        if ext == "map" {
            findings.push(
                Finding::new(
                    Domain::Security,
                    "client.sourcemap_exposed",
                    "Source map shipped to production",
                    Severity::Medium,
                    Confidence::High,
                    Surface::ClientBundle,
                    "A .js.map source map is present in the build output, exposing original source to anyone.",
                )
                .with_location(Location::file(path.display().to_string(), 1))
                .with_remediation("Disable source-map emission for production, or upload maps to your error tracker and delete them from the deploy."),
            );
        }

        if !SCAN_EXTS.contains(&ext.as_str()) {
            continue;
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_SIZE {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let file = path.display().to_string();

        // Generic secret rules, but tagged as a (more severe) client-bundle leak.
        crate::scan_text(&file, &content, Surface::ClientBundle, findings);

        // Supabase service_role / privileged JWTs shipped to the client.
        scan_jwts(&file, &content, findings);

        // Public-env-prefixed secret-shaped values inlined into the bundle.
        scan_public_env(&file, &content, findings);
    }
}

/// Find JWTs and, when they decode to a privileged role (e.g. Supabase
/// `service_role`), raise a critical finding — that key bypasses row-level security.
fn scan_jwts(file: &str, content: &str, findings: &mut Vec<Finding>) {
    static JWT: &str = r"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}";
    let re = regex::Regex::new(JWT).expect("jwt regex");
    for m in re.find_iter(content) {
        let token = m.as_str();
        if let Some(role) = jwt_role(token) {
            if role == "service_role" || role == "admin" {
                findings.push(
                    Finding::new(
                        Domain::Security,
                        "client.supabase_service_role",
                        format!("Privileged `{role}` JWT shipped to the browser"),
                        Severity::Critical,
                        Confidence::High,
                        Surface::ClientBundle,
                        "A JWT whose payload role is privileged (service_role bypasses row-level security) is present in the client bundle. Anyone can read it and gain full database access.",
                    )
                    .with_location(Location::file(file, line_of(content, m.start())))
                    .with_evidence_redacted(token)
                    .with_remediation("Remove the service_role key from client code; use the anon/publishable key in the browser and the service key only on the server."),
                );
            }
        }
    }
}

/// Decode a JWT payload and return its `role` claim, if present.
fn jwt_role(token: &str) -> Option<String> {
    let payload_b64 = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("role").and_then(|r| r.as_str()).map(str::to_string)
}

/// Public-env prefixes that frameworks inline into the client bundle.
const PUBLIC_PREFIXES: &[&str] = &[
    "NEXT_PUBLIC_",
    "VITE_",
    "EXPO_PUBLIC_",
    "REACT_APP_",
    "GATSBY_",
    "NUXT_PUBLIC_",
];

/// Flag public-env-prefixed names that contain a secret-ish token (e.g.
/// `NEXT_PUBLIC_SUPABASE_SERVICE_ROLE_KEY`, `..._SECRET`, `..._PRIVATE`).
fn scan_public_env(file: &str, content: &str, findings: &mut Vec<Finding>) {
    for prefix in PUBLIC_PREFIXES {
        let mut from = 0;
        while let Some(rel) = content[from..].find(prefix) {
            let start = from + rel;
            // Read the identifier following the prefix.
            let rest = &content[start..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            from = start + name.len().max(prefix.len());
            let upper = name.to_ascii_uppercase();
            if upper.contains("SERVICE_ROLE")
                || upper.contains("SECRET")
                || upper.contains("PRIVATE")
                || upper.ends_with("_KEY")
                    && upper.contains("SUPABASE")
                    && upper.contains("SERVICE")
            {
                findings.push(
                    Finding::new(
                        Domain::Security,
                        "client.public_env_secret",
                        format!("Secret-named public env var in client bundle: {name}"),
                        Severity::High,
                        Confidence::Medium,
                        Surface::ClientBundle,
                        "A framework public-env variable (inlined into the browser bundle) is named like a secret. Public-prefixed vars are world-readable.",
                    )
                    .with_location(Location::file(file, line_of(content, start)))
                    .with_evidence_safe(name)
                    .with_remediation("Don't expose secrets via a public-prefixed env var; drop the public prefix and read the value server-side only."),
                );
            }
        }
    }
}

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

    // A real Supabase-style JWT payload: {"role":"service_role","iss":"supabase"}
    // header.payload.signature (URL-safe base64, no padding). Signature is dummy.
    fn service_role_jwt() -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"role":"service_role","iss":"supabase","iat":1700000000}"#);
        format!("{header}.{payload}.ZHVtbXlfc2lnbmF0dXJlX3ZhbHVl")
    }

    #[test]
    fn detects_service_role_and_sourcemap_in_bundle() {
        let dir = tempdir().unwrap();
        let dist = dir.path().join("dist");
        fs::create_dir_all(&dist).unwrap();
        fs::write(
            dist.join("app.js"),
            format!("const sb='{}';\n", service_role_jwt()),
        )
        .unwrap();
        fs::write(dist.join("app.js.map"), "{\"version\":3}").unwrap();

        let mut findings = Vec::new();
        scan_client_bundles(dir.path(), &mut findings);

        assert!(
            findings
                .iter()
                .any(|f| f.rule_id == "client.supabase_service_role"),
            "should flag service_role JWT: {findings:#?}"
        );
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "client.sourcemap_exposed"));
    }

    #[test]
    fn flags_public_env_secret_name() {
        let dir = tempdir().unwrap();
        let dist = dir.path().join("dist");
        fs::create_dir_all(&dist).unwrap();
        fs::write(
            dist.join("chunk.js"),
            "var x = process.env.NEXT_PUBLIC_SUPABASE_SERVICE_ROLE_KEY;",
        )
        .unwrap();

        let mut findings = Vec::new();
        scan_client_bundles(dir.path(), &mut findings);
        assert!(findings
            .iter()
            .any(|f| f.rule_id == "client.public_env_secret"));
    }
}
