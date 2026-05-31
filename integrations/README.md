# voltiq integrations

voltiq ships as a single static binary. Put it on your `PATH`, then wire it into
your AI agent (MCP, stdio) or CI.

## Claude Code

```bash
claude mcp add voltiq -- voltiq mcp
```

or commit `.mcp.json` (see `claude-mcp.json`) to share with your team.

## Cursor

Copy `cursor-mcp.json` to `.cursor/mcp.json`.

## OpenAI Codex

Add the `[mcp_servers.voltiq]` block from `codex-config.toml` to your Codex config.

## Pre-commit

Copy `pre-commit.sh` to `.git/hooks/pre-commit` (and `chmod +x`), or wire it through
your pre-commit framework. It runs a fast secret scan and blocks the commit on
high/critical findings.

## CI (GitHub Actions)

`github-action-scan.yml` runs `voltiq audit` and uploads SARIF to GitHub
code-scanning. The repo's own build/test matrix lives in `.github/workflows/`.
