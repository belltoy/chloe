#!/usr/bin/env bash
# Cross-agent demo, both directions, one shared database:
#   1. Claude Code saves a memory, Codex retrieves it.
#   2. Codex saves a memory, Claude Code retrieves it.
# Uses a temp copy of ./repo and a temp database; your real chloe data is never touched.
# Usage: CHLOE=/path/to/chloe ./run.sh   (default: `chloe` on PATH)
set -euo pipefail

CHLOE=${CHLOE:-chloe}
CHLOE=$(command -v "$CHLOE") || { echo "chloe binary not found; set CHLOE=" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp -R "$(dirname "$0")/repo" "$tmp/repo"
git -C "$tmp/repo" init -q # a Git root makes the project scope stable
db=$tmp/shared.db

cat >"$tmp/mcp.json" <<JSON
{"mcpServers":{"chloe":{"command":"$CHLOE","args":["--db-path","$db","mcp","serve"]}}}
JSON

cd "$tmp/repo"

claude_run() { # prompt -> stdout
  claude -p "$1" --mcp-config "$tmp/mcp.json" --strict-mcp-config \
    --allowedTools mcp__chloe__memory_save,mcp__chloe__memory_search </dev/null
}

codex_run() { # prompt -> stdout (Codex also echoes hook noise; drop it)
  codex exec --skip-git-repo-check \
    -c "mcp_servers.chloe.command=\"$CHLOE\"" \
    -c "mcp_servers.chloe.args=[\"--db-path\",\"$db\",\"mcp\",\"serve\"]" \
    -c 'mcp_servers.chloe.default_tools_approval_mode="approve"' \
    "$1" </dev/null 2>&1 | grep -v '^hook'
}

expect() { # direction output marker
  if grep -q "$3" <<<"$2"; then
    echo "PASS ($1): retrieved '$3'"
  else
    echo "FAIL ($1): marker '$3' not found in output" >&2
    "$CHLOE" --db-path "$db" --dir "$tmp/repo" memory list >&2
    exit 1
  fi
}

m1="zebra-$RANDOM$RANDOM"
m2="otter-$RANDOM$RANDOM"

echo "== 1. Claude Code saves, Codex retrieves =="
claude_run "Call the chloe memory_save tool with kind 'procedure', title 'Deploy checklist', content 'Before deploy, run the smoke suite. Ticket code: $m1'. Reply with only the memory id."
out=$(codex_run "Call the chloe memory_search tool with query 'deploy' and reply with the ticket code stored in the hit's content.")
echo "$out" | tail -3
expect "Claude -> Codex" "$out" "$m1"

echo "== 2. Codex saves, Claude Code retrieves =="
codex_run "Call the chloe memory_save tool with kind 'gotcha', title 'Rollback checklist', content 'Before rollback, drain the queue. Ticket code: $m2'. Reply with only the memory id." | tail -2
out=$(claude_run "Call the chloe memory_search tool with query 'rollback' and reply with the ticket code stored in the hit's content.")
echo "$out" | tail -3
expect "Codex -> Claude" "$out" "$m2"

echo "PASS: both directions share one database"
