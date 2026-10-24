#!/usr/bin/env bash
# Scenario B-E2E-01: automatic learning, then cross-agent recall.
#   1. Claude Code (real, with chloe's capture hooks) fixes a small bug in ./repo.
#   2. chloe imports the captured events and a local model proposes candidate memories.
#   3. A person (this script, standing in) accepts a candidate.
#   4. A fresh Codex session finds it through MCP. Nobody calls memory_save.
# Everything runs in a temp dir with a temp database and a temp Claude settings file; your real
# chloe data and agent settings are not touched. Needs Ollama with qwen3:8b and uv (pytest is installed by `uv run --with pytest` into a throwaway environment).
# Usage: CHLOE=/path/to/chloe ./run.sh   (default: `chloe` on PATH)
set -euo pipefail

CHLOE=${CHLOE:-chloe}
CHLOE=$(command -v "$CHLOE") || { echo "chloe binary not found; set CHLOE=" >&2; exit 1; }
CHLOE=$(cd "$(dirname "$CHLOE")" && pwd)/$(basename "$CHLOE")
tmp=$(mktemp -d)
[ -n "${KEEP:-}" ] || trap 'rm -rf "$tmp"' EXIT
cp -R "$(dirname "$0")/repo" "$tmp/repo"
git -C "$tmp/repo" init -q
db=$tmp/e2e.db
mkdir "$tmp/bin" && ln -s "$CHLOE" "$tmp/bin/chloe" # the hooks run the bare command `chloe`
export PATH="$tmp/bin:$PATH" CHLOE_DB=$db
cd "$tmp/repo"
c() { chloe --dir "$tmp/repo" "$@"; }
c init

echo "== 0. install hooks into a temp settings file =="
chloe hook install claude --settings "$tmp/claude-settings.json"

echo "== 1. Claude Code fixes the bug (no chloe MCP, no memory_save) =="
claude -p "The tests fail (run: uv run --with pytest pytest -q). Find the cause, fix pager.py, run the tests again, and explain the root cause in one sentence." \
  --settings "$tmp/claude-settings.json" --allowedTools "Read,Edit,Bash(uv run *)" </dev/null | tail -5
uv run --with pytest pytest -q 2>&1 | tail -1

echo "== 2. captured events =="
c events import
c events sessions
c memory list | sed 's/^/memory: /'
echo "(memories before review: $(c --json memory list | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'))"

echo "== 3. extract candidates with the local model =="
c consolidate
c candidates list
# A lenient reviewer: accept every candidate. Nothing is cherry-picked, so what Codex can find is
# what the pipeline learned.
ids=$(c --json candidates list | python3 -c 'import json,sys; print(" ".join(x["id"] for x in json.load(sys.stdin)))')
[ -n "$ids" ] || { echo "FAIL: no candidate was proposed" >&2; exit 1; }

echo "== 4. review: accept all candidates =="
mids=""
for cid in $ids; do
  c candidates show "$cid" | head -4
  mids="$mids $(c --json candidates accept "$cid" | python3 -c 'import json,sys; print(json.load(sys.stdin)["memory_id"])')"
done
echo "memories:$mids"
c embeddings backfill

echo "== 5. a new Codex session finds it through MCP =="
out=$(codex exec --skip-git-repo-check \
  -c "mcp_servers.chloe.command=\"$CHLOE\"" \
  -c "mcp_servers.chloe.args=[\"--db-path\",\"$db\",\"mcp\",\"serve\"]" \
  -c 'mcp_servers.chloe.default_tools_approval_mode="approve"' \
  "Use the chloe memory_search tool to look for what we know about page counts or pagination bugs in this project. Reply with the id, title and source field of every hit." </dev/null 2>&1 | grep -v '^hook')
echo "$out" | tail -8
for mid in $mids; do
  if grep -q "$mid" <<<"$out"; then echo "PASS: Codex returned memory $mid"; found=1; fi
done
[ -n "${found:-}" ] || { echo "FAIL: none of the accepted memories was returned" >&2; exit 1; }
