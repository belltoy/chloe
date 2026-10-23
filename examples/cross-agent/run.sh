#!/usr/bin/env bash
# Cross-agent demo: Claude Code saves a memory, Codex retrieves it from the same database.
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
marker="zebra-$RANDOM$RANDOM"

cat >"$tmp/mcp.json" <<JSON
{"mcpServers":{"chloe":{"command":"$CHLOE","args":["--db-path","$db","mcp","serve"]}}}
JSON

cd "$tmp/repo"

echo "== Agent A (Claude Code) saves =="
claude -p "Call the chloe memory_save tool with kind 'procedure', title 'Deploy checklist', content 'Before deploy, run the smoke suite. Ticket code: $marker'. Reply with only the memory id." \
  --mcp-config "$tmp/mcp.json" --strict-mcp-config \
  --allowedTools mcp__chloe__memory_save </dev/null

echo "== Agent B (Codex) retrieves =="
out=$(codex exec --skip-git-repo-check \
  -c "mcp_servers.chloe.command=\"$CHLOE\"" \
  -c "mcp_servers.chloe.args=[\"--db-path\",\"$db\",\"mcp\",\"serve\"]" \
  -c 'mcp_servers.chloe.default_tools_approval_mode="approve"' \
  "Call the chloe memory_search tool with query 'deploy' and reply with the ticket code stored in the hit's content." </dev/null 2>&1)
echo "$out" | grep -v '^hook' | tail -5

if grep -q "$marker" <<<"$out"; then
  echo "PASS: Codex retrieved '$marker' saved by Claude Code"
else
  echo "FAIL: marker '$marker' not found in Codex output" >&2
  "$CHLOE" --db-path "$db" --dir "$tmp/repo" memory list >&2
  exit 1
fi
