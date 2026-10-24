#!/usr/bin/env bash
# B-08 with two real Claude Code sessions. Usage: ./run.sh <run-number> <variant 1|2|3>
# Everything lives in runs/run<N>/: temp db, temp Claude settings, fixture repo, logs.
set -euo pipefail

N=${1:?run number}; V=${2:-1}
HERE=$(cd "$(dirname "$0")" && pwd)
CHLOE_BIN=${CHLOE:-$(command -v chloe)}
R=$HERE/runs/run$N
rm -rf "$R"; mkdir -p "$R/logs" "$R/bin"
ln -s "$CHLOE_BIN" "$R/bin/chloe"
export PATH="$R/bin:$PATH" CHLOE_DB="$R/b08.db"
cp -R "$HERE/fixture${FX:-}" "$R/fixture"
git -C "$R/fixture" init -q && git -C "$R/fixture" add -A && git -C "$R/fixture" -c user.name=t -c user.email=t@t commit -qm fixture
c() { chloe --dir "$R/fixture" "$@"; }
c init >/dev/null
chloe hook install claude --settings "$R/claude-settings.json" >/dev/null
{ date; claude --version; chloe --version; echo "variant $V"; } > "$R/logs/versions.txt"

run_session() { # run_session <n>
  ( cd "$R/fixture" && claude -p "$(cat "$HERE/prompts/s$1-v${V}.txt")" \
      --settings "$R/claude-settings.json" \
      --allowedTools "Read,Edit,Write,Bash(make *),Bash(python3 *)" </dev/null ) > "$R/logs/claude-s$1.txt" 2>&1 || true
}
dump() { # dump <tag>: candidates (all states) and memories as JSON
  c --json candidates list --state all > "$R/logs/$1-candidates.json"
  c --json memory list > "$R/logs/$1-memories.json"
}

echo "== session 1"; run_session 1
c events import >/dev/null; c events sessions > "$R/logs/s1-sessions.txt"
c consolidate > "$R/logs/s1-consolidate.txt" 2>&1 || true
dump s1-before-review
for id in $(c --json candidates list | python3 -c 'import json,sys; print(" ".join(x["id"] for x in json.load(sys.stdin)))'); do
  c --json candidates accept "$id" >/dev/null || true
done
dump s1-after-accept

git -C "$R/fixture" checkout -q . && git -C "$R/fixture" clean -qfd; [ -d "$HERE/overlay${FX:-}-s2" ] && cp -R "$HERE/overlay${FX:-}-s2/." "$R/fixture/"
echo "== session 2"; run_session 2
c events import >/dev/null; c events sessions > "$R/logs/s2-sessions.txt"
c consolidate > "$R/logs/s2-consolidate.txt" 2>&1 || true
dump s2-after-consolidate
c memory list > "$R/logs/s2-memories-before-review.txt"
echo "run $N done; review state is in $R/logs"
