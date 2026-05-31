//! Detect which JS runtime a launch command uses.

/// Map a launch command to a runtime name (`node`/`bun`/`deno`/`npm`/`pnpm`/`yarn`),
/// resolving the package-manager `run` wrappers to themselves.
pub fn detect_runtime(command: &[String]) -> Option<String> {
    let first = command.first()?;
    let bin = first
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(first)
        .trim_end_matches(".exe")
        .trim_end_matches(".cmd");
    let rt = match bin {
        "node" | "nodejs" => "node",
        "bun" | "bunx" => "bun",
        "deno" => "deno",
        "npm" | "npx" => "npm",
        "pnpm" | "pnpx" | "pnpm.cjs" => "pnpm",
        "yarn" => "yarn",
        other => other,
    };
    Some(rt.to_string())
}

/// Cold-start baseline (ms) a runtime is expected to beat; used by the slow-startup rule.
pub fn cold_start_baseline_ms(runtime: &str) -> f64 {
    match runtime {
        "bun" => 400.0,
        "deno" => 800.0,
        _ => 1500.0, // node and the pm wrappers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_runtimes() {
        let d = |s: &str| detect_runtime(&s.split(' ').map(String::from).collect::<Vec<_>>());
        assert_eq!(d("bun run start").as_deref(), Some("bun"));
        assert_eq!(d("/usr/bin/node server.js").as_deref(), Some("node"));
        assert_eq!(d("pnpm dev").as_deref(), Some("pnpm"));
        assert_eq!(d("deno run main.ts").as_deref(), Some("deno"));
    }
}
