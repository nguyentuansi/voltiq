//! The secret-detection rule pack: high-confidence provider patterns plus one
//! generic keyword+entropy rule. `regex` (no lookarounds/backrefs) keeps these fast.

use std::sync::OnceLock;

use regex::Regex;
use voltiq_core::{Confidence, Severity};

pub struct SecretRule {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub severity: Severity,
    pub confidence: Confidence,
    pub regex: Regex,
    /// Capture group holding the secret value (0 = whole match).
    pub group: usize,
    /// If set, the captured value must exceed this Shannon entropy to be reported
    /// (and be at least 16 chars). Used by the generic rule to cut false positives.
    pub entropy_min: Option<f64>,
    pub remediation: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn rule(
    id: &'static str,
    title: &'static str,
    description: &'static str,
    severity: Severity,
    confidence: Confidence,
    pattern: &str,
    group: usize,
    entropy_min: Option<f64>,
    remediation: &'static str,
) -> SecretRule {
    SecretRule {
        id,
        title,
        description,
        severity,
        confidence,
        regex: Regex::new(pattern).expect("static secret rule regex must compile"),
        group,
        entropy_min,
        remediation,
    }
}

/// The full rule pack (compiled once).
pub fn rules() -> &'static [SecretRule] {
    static RULES: OnceLock<Vec<SecretRule>> = OnceLock::new();
    RULES.get_or_init(build).as_slice()
}

fn build() -> Vec<SecretRule> {
    let crit = Severity::Critical;
    let high = Severity::High;
    let med = Severity::Medium;
    let c_high = Confidence::High;
    let c_med = Confidence::Medium;
    let c_low = Confidence::Low;
    vec![
        rule(
            "secret.aws_access_key_id",
            "AWS access key id",
            "An AWS access key id was found. Paired with a secret key it grants account access.",
            crit, c_high,
            r"\b((?:AKIA|ASIA|AGPA|AIDA|AROA|AIPA|ANPA|ANVA)[A-Z0-9]{16})\b",
            1, None,
            "Rotate the key in IAM immediately and load credentials from the environment / a secrets manager.",
        ),
        rule(
            "secret.github_token",
            "GitHub token",
            "A GitHub personal access / OAuth / app token was found.",
            crit, c_high,
            r"\b((?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36})\b",
            1, None,
            "Revoke the token in GitHub settings and use a short-lived token from the environment.",
        ),
        rule(
            "secret.github_pat_fine_grained",
            "GitHub fine-grained PAT",
            "A GitHub fine-grained personal access token was found.",
            crit, c_high,
            r"\b(github_pat_[A-Za-z0-9_]{60,})\b",
            1, None,
            "Revoke the token in GitHub settings.",
        ),
        rule(
            "secret.openai_key",
            "OpenAI API key",
            "An OpenAI API key was found.",
            crit, c_high,
            r"\b(sk-(?:proj-)?[A-Za-z0-9_-]{20,})\b",
            1, None,
            "Revoke the key in the OpenAI dashboard and read it from the environment server-side only.",
        ),
        rule(
            "secret.anthropic_key",
            "Anthropic API key",
            "An Anthropic API key was found.",
            crit, c_high,
            r"\b(sk-ant-[A-Za-z0-9_-]{20,})\b",
            1, None,
            "Revoke the key in the Anthropic console and read it from the environment server-side only.",
        ),
        rule(
            "secret.stripe_secret_key",
            "Stripe secret key",
            "A live Stripe secret/restricted key was found.",
            crit, c_high,
            r"\b((?:sk|rk)_live_[0-9A-Za-z]{24,})\b",
            1, None,
            "Roll the key in the Stripe dashboard. Never ship secret keys to the client.",
        ),
        rule(
            "secret.google_api_key",
            "Google API key",
            "A Google API key was found.",
            high, c_high,
            r"\b(AIza[0-9A-Za-z_-]{35})\b",
            1, None,
            "Restrict or rotate the key in the Google Cloud console.",
        ),
        rule(
            "secret.slack_token",
            "Slack token",
            "A Slack API token was found.",
            high, c_high,
            r"\b(xox[baprs]-[0-9A-Za-z-]{10,})\b",
            1, None,
            "Revoke the token in the Slack admin and store it server-side only.",
        ),
        rule(
            "secret.slack_webhook",
            "Slack incoming webhook",
            "A Slack incoming webhook URL was found.",
            med, c_high,
            r"(https://hooks\.slack\.com/services/[A-Za-z0-9/_-]+)",
            1, None,
            "Regenerate the webhook; treat its URL as a secret.",
        ),
        rule(
            "secret.npm_token",
            "npm token",
            "An npm access token was found.",
            high, c_high,
            r"\b(npm_[A-Za-z0-9]{36})\b",
            1, None,
            "Revoke the token with `npm token revoke` and use a CI-scoped token.",
        ),
        rule(
            "secret.sendgrid_key",
            "SendGrid API key",
            "A SendGrid API key was found.",
            high, c_high,
            r"\b(SG\.[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43})\b",
            1, None,
            "Revoke the key in the SendGrid dashboard.",
        ),
        rule(
            "secret.private_key_block",
            "Private key block",
            "A PEM/OpenSSH private key block was found.",
            crit, c_high,
            r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP )?PRIVATE KEY-----",
            0, None,
            "Remove the key from the repo, rotate it, and store it in a secrets manager.",
        ),
        rule(
            "secret.jwt",
            "JSON Web Token",
            "A JWT was found (e.g. a Supabase anon/service_role key). Confirm it is not a privileged key shipped to clients.",
            med, c_med,
            r"\b(eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})\b",
            1, None,
            "If this is a privileged key (e.g. Supabase service_role), rotate it and keep it server-side.",
        ),
        rule(
            "secret.generic_high_entropy",
            "High-entropy secret assignment",
            "A secret-shaped, high-entropy value was assigned to a credential-like name.",
            high, c_low,
            r#"(?i)(?:api[_-]?key|secret|token|password|passwd|access[_-]?key|auth)["' ]?\s*[:=]\s*["']([^"']{16,})["']"#,
            1, Some(3.5),
            "Move the value to an environment variable / secrets manager and rotate it.",
        ),
    ]
}
