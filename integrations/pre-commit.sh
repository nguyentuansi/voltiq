#!/usr/bin/env bash
# voltiq pre-commit hook: fast secret scan; blocks the commit on high/critical findings.
# Install: cp integrations/pre-commit.sh .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
set -euo pipefail

if ! command -v voltiq >/dev/null 2>&1; then
  echo "voltiq not on PATH; skipping scan (install it to enable the pre-commit guard)." >&2
  exit 0
fi

# Scan the working tree; exit non-zero (and block the commit) on high/critical findings.
voltiq scan . --fail-on high
