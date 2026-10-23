#!/usr/bin/env bash
# Black-box run of the Milestone A acceptance items that need no real agent (see
# docs/acceptance-criteria.md). A-08 and A-15 are run with real agents separately
# (examples/cross-agent/run.sh). Uses temp dirs only; exits non-zero if any item FAILs.
# Usage: scripts/acceptance-a.sh [--skip-cargo]   (needs a built chloe: CHLOE=... or target/debug/chloe)
set -u
CHLOE=${CHLOE:-$PWD/target/debug/chloe}
tmp=$(mktemp -d)
trap 'chmod -R u+rwx "$tmp" 2>/dev/null; rm -rf "$tmp"' EXIT
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
unset CHLOE_DB

DB="" P=""
c() { "$CHLOE" --db-path "$DB" --dir "$P" --json "$@"; }
J() { python3 -c 'import sys,json;d=json.load(sys.stdin);print(eval(sys.argv[1]))' "$1"; }
mkrepo() { mkdir -p "$1" && git init -q "$1"; }
mode() { stat -f '%Lp' "$1" 2>/dev/null || stat -c '%a' "$1"; }
mcp() { # DB P; stdin: JSON-RPC lines -> stdout replies
  "$CHLOE" --db-path "$DB" --dir "$P" mcp serve; }

fails=0
check() { # id desc function
  local id=$1 desc=$2; shift 2
  local log; log=$("$@" 2>&1); local rc=$?
  if [ $rc -eq 0 ]; then printf 'PASS  %-5s %s\n' "$id" "$desc"
  elif [ $rc -eq 77 ]; then printf 'BLOCKED %-5s %s (%s)\n' "$id" "$desc" "$log"
  else printf 'FAIL  %-5s %s\n%s\n' "$id" "$desc" "$log"; fails=$((fails + 1)); fi
}

a01() { cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace; }
a02() { DB=$tmp/a02.db P=$tmp/a02; mkrepo "$P"
  c init >/dev/null && c memory add --title t --content body >/dev/null && c init >/dev/null || return 1
  [ "$(c memory list | J 'len(d)')" = 1 ]; }
a03() { DB=$tmp/a03.db P=$tmp/a03; mkrepo "$P"
  local id; id=$(c memory add --title t --content crud | J 'd["id"]') || return 1
  [ "$(c memory show "$id" | J 'd["revision"]')" = 1 ] || return 1
  [ "$(c memory update "$id" --revision 1 --title t2 | J 'd["revision"]')" = 2 ] || return 1
  [ "$(c memory archive "$id" --revision 2 | J 'd["revision"]')" = 3 ] || return 1
  [ "$(c memory search crud | J 'len(d)')" = 0 ] && [ "$(c memory search crud --all | J 'len(d)')" = 1 ]; }
a04() { DB=$tmp/a04.db P=$tmp/a04; mkrepo "$P"
  c memory add --title cli --content clipersist >/dev/null || return 1
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_save","arguments":{"kind":"fact","title":"mcp","content":"mcppersist"}}}' | mcp >/dev/null || return 1
  [ "$(c memory search mcppersist | J 'len(d)')" = 1 ] || return 1   # written by MCP process, read by a new CLI process
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"clipersist"}}}' | mcp | grep -q clipersist; }
a05() { DB=$tmp/a05.db P=$tmp/a05; mkrepo "$P"
  local id; id=$(c memory add --title t --content alphaword | J 'd["id"]') || return 1
  [ "$(c memory search alphaword | J 'len(d)')" = 1 ] || return 1
  c memory update "$id" --revision 1 --content betaword >/dev/null || return 1
  [ "$(c memory search alphaword | J 'len(d)')" = 0 ] && [ "$(c memory search betaword | J 'len(d)')" = 1 ] || return 1
  c memory archive "$id" --revision 2 >/dev/null || return 1
  [ "$(c memory search betaword | J 'len(d)')" = 0 ]; }
a06() { DB=$tmp/a06.db; local a=$tmp/a06a b=$tmp/a06b; mkrepo "$a"; mkrepo "$b"
  P=$a; local id; id=$(c memory add --title t --content onlyinA | J 'd["id"]') || return 1
  P=$b; [ "$(c memory search onlyinA | J 'len(d)')" = 0 ] || return 1
  [ "$(c memory show "$id" | J 'd["error"]["code"]')" = not_found ] || return 1
  [ "$(c memory update "$id" --revision 1 --title x | J 'd["error"]["code"]')" = not_found ]; }
a07() { DB=$tmp/a07.db; local r=$tmp/a07r other=$tmp/a07o; mkrepo "$r"; mkrepo "$other"
  git -C "$r" commit -q --allow-empty -m x && git -C "$r" worktree add -q "$tmp/a07wt" || return 1
  P=$r; local main; main=$(c project show | J 'd["project_id"]')
  P=$tmp/a07wt; local wt; wt=$(c project show | J 'd["project_id"]')
  P=$other; local oth; oth=$(c project show | J 'd["project_id"]')
  [ "$main" = "$wt" ] && [ "$main" != "$oth" ]; }
a09() { DB=$tmp/a09.db P=$tmp/a09; mkrepo "$P"
  local out; out=$(printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_save","arguments":{"kind":"bogus","title":"t","content":"c"}}}' \
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nope","arguments":{}}}' \
    'not json' \
    '{"jsonrpc":"2.0","id":5,"method":"ping"}' | mcp) || return 1
  echo "$out" | python3 -c '
import sys,json
r=[json.loads(l) for l in sys.stdin]
assert len(r)==6, len(r)
assert len(r[1]["result"]["tools"])==5 and all("inputSchema" in t for t in r[1]["result"]["tools"])
assert r[2]["result"]["isError"] and json.loads(r[2]["result"]["content"][0]["text"])["code"]=="invalid_argument"
assert r[3]["error"]["code"]==-32602 and r[4]["error"]["code"]==-32700
assert r[5]["result"]=={}   # still alive after bad input
'; }
a10() { DB=$tmp/a10.db P=$tmp/a10; mkrepo "$P"
  local id; id=$(c memory add --title t --content cas | J 'd["id"]') || return 1
  for i in 1 2 3 4 5 6 7 8; do (c memory update "$id" --revision 1 --title "w$i" >"$tmp/a10.$i" 2>&1; echo $? >"$tmp/a10.$i.rc") & done; wait
  local ok; ok=$(cat "$tmp"/a10.*.rc | grep -c '^0$'); local conf; conf=$(grep -l '"conflict"' "$tmp"/a10.? | wc -l)
  [ "$ok" -eq 1 ] && [ "$conf" -eq 7 ] && [ "$(c memory show "$id" | J 'd["revision"]')" = 2 ]; }
a11() { cargo test -q -p memory-store busy_database && cargo test -q -p memory-store failed_update_rolls_back && cargo test -q -p memory-cli --test concurrency; }
a12() { DB=$tmp/a12/data/a12.db P=$tmp/a12r; mkrepo "$P"
  c memory add --title t --content SECRETBODY123 >"$tmp/a12.out" 2>"$tmp/a12.err" || return 1
  [ "$(mode "$DB")" = 600 ] && [ "$(mode "$(dirname "$DB")")" = 700 ] || { echo "bad permissions"; return 1; }
  CHLOE_LOG=trace c memory add --title t2 --content SECRETBODY456 2>"$tmp/a12.trace" >/dev/null
  ! grep -q SECRET "$tmp/a12.err" "$tmp/a12.trace" || { echo "memory body in logs"; return 1; }
  command -v lsof >/dev/null || { echo "lsof missing"; return 77; }
  (sleep 3) | mcp >/dev/null 2>&1 & local pid=$!; sleep 1
  local sock; sock=$(lsof -nP -a -p "$pid" -i 2>/dev/null | tail -n +2); kill "$pid" 2>/dev/null; wait 2>/dev/null
  [ -z "$sock" ] || { echo "sockets: $sock"; return 1; }; }
a13() { command -v sqlite3 >/dev/null || { echo "sqlite3 missing"; return 77; }
  DB=$tmp/a13.db P=$tmp/a13; mkrepo "$P"
  c memory add --title t --content ftsword >/dev/null || return 1
  sqlite3 "$DB" "INSERT INTO memory_fts(memory_fts) VALUES('delete-all')" || return 1   # index anomaly
  [ "$(c doctor | J 'd["healthy"]')" = False ] || { echo "anomaly not detected"; return 1; }
  [ "$(c doctor --reindex | J 'd["healthy"]')" = True ] && [ "$(c memory search ftsword | J 'len(d)')" = 1 ]; }
a14() { DB=$tmp/a14.db P=$tmp/a14; mkrepo "$P"
  local id; id=$(c memory add --title t --content jsonword | J 'd["id"]') || return 1
  c memory search jsonword | J 'd[0]["memory"]["id"]' >/dev/null && c memory list | J 'len(d)' >/dev/null && c memory show "$id" | J 'd["id"]' >/dev/null || return 1
  [ "$(c memory update "$id" --revision 9 --title x | J 'd["error"]["code"]')" = conflict ] &&
  [ "$(c memory show 00000000-0000-0000-0000-000000000000 | J 'd["error"]["code"]')" = not_found ]; }
a16() { command -v sandbox-exec >/dev/null || { echo "sandbox-exec missing"; return 77; }
  DB=$tmp/a16.db P=$tmp/a16; mkrepo "$P"
  local sb=(sandbox-exec -p '(version 1)(allow default)(deny network*)')
  "${sb[@]}" "$CHLOE" --db-path "$DB" --dir "$P" --json memory add --title t --content offlineword >/dev/null || return 1
  [ "$("${sb[@]}" "$CHLOE" --db-path "$DB" --dir "$P" --json memory search offlineword | J 'len(d)')" = 1 ] || return 1
  "${sb[@]}" "$CHLOE" --db-path "$DB" --dir "$P" --json doctor >/dev/null; }
a17() { command -v sqlite3 >/dev/null || { echo "sqlite3 missing"; return 77; }
  DB=$tmp/a17.db P=$tmp/a17; mkrepo "$P"
  local id; id=$(c memory add --title t --content audit | J 'd["id"]') || return 1
  c memory update "$id" --revision 1 --title t2 >/dev/null && c memory archive "$id" --revision 2 >/dev/null || return 1
  [ "$(c memory history "$id" | J '[(r["revision"],r["cause"],r["actor"]) for r in d]')" = "[(1, 'create', 'cli'), (2, 'update', 'cli'), (3, 'archive', 'cli')]" ] || return 1
  ! sqlite3 "$DB" "UPDATE memory_revisions SET cause='x'" 2>/dev/null && ! sqlite3 "$DB" "DELETE FROM memory_revisions" 2>/dev/null; }
a18() { DB=$tmp/a18.db; local a=$tmp/a18a b=$tmp/a18b; mkrepo "$a"; mkrepo "$b"
  P=$a; c memory add --title g --content scopeword --global >/dev/null && c memory add --title p --content scopeword >/dev/null || return 1
  P=$b; [ "$(c memory search scopeword | J 'len(d)')" = 0 ] || return 1
  [ "$(c memory search scopeword --global | J '[h["memory"]["title"] for h in d]')" = "['g']" ] || return 1
  [ "$(c memory add --title x --content c --kind nonsense 2>&1 >/dev/null | head -c 1)" != "" ] || return 1
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_save","arguments":{"kind":"fact","title":"t","content":"c","scope":"galaxy"}}}' | mcp | grep -q invalid_argument; }
a19() { DB=$tmp/a19.db P=$tmp/a19; mkrepo "$P"
  [ "$(c doctor | J 'd["healthy"]')" = True ] || return 1
  local ro=$tmp/a19ro; mkdir -p "$ro"; : >"$ro/t.db"; chmod 444 "$ro/t.db"; chmod 555 "$ro"
  local out; out=$("$CHLOE" --db-path "$ro/t.db" --json doctor); [ $? -ne 0 ] && echo "$out" | grep -q '"database_open"' || { echo "read-only db not reported"; return 1; }
  : >"$tmp/blocker"; out=$("$CHLOE" --db-path "$tmp/blocker/x.db" --json doctor); [ $? -ne 0 ] && echo "$out" | grep -q '"database_open"'; }
a20() { DB=$tmp/a20.db P=$tmp/a20; mkrepo "$P"
  printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' | mcp | python3 -c '
import sys,json
t=json.loads(sys.stdin.readline())["result"]["instructions"]
assert "memory_search" in t and "memory_save" in t and "expected_revision" in t'; }

if [ "${1:-}" = --skip-cargo ]; then echo "SKIPPED A-01 (--skip-cargo)"; else check A-01 "fmt, clippy, tests" a01; fi
check A-02 "init twice, data kept" a02
check A-03 "CRUD, revisions increase, archived hidden" a03
check A-04 "persistence across processes (CLI <-> MCP)" a04
check A-05 "FTS follows add/update/archive" a05
check A-06 "project isolation" a06
check A-07 "worktree resolves to main project" a07
check A-09 "MCP conformance and bad input" a09
check A-10 "8 parallel same-revision updates: one winner" a10
check A-11 "SQLITE_BUSY, rollback, parallel writers, SIGKILL" a11
check A-12 "private files, no sockets, no body in logs" a12
check A-13 "FTS anomaly detected and repaired" a13
check A-14 "--json output and stable error codes" a14
check A-16 "works with network denied" a16
check A-17 "audit trail ordered and append-only" a17
check A-18 "explicit global scope" a18
check A-19 "doctor reports faults, passes when healthy" a19
check A-20 "MCP instructions present (agent behaviour not tested here)" a20
echo "A-08, A-15: not part of this script (real agents / clean-environment quickstart)"
[ $fails -eq 0 ] && echo "ALL RUN ITEMS PASSED" || echo "$fails ITEM(S) FAILED"
exit $((fails > 0))
