#!/usr/bin/env bash
# Scenario B-08: the same fact in two sessions must not become two Active memories.
# Session 1 and session 2 are hand-written event streams (no agent is run), fed through the real
# capture spool, the local model (extraction + duplicate judgement) and the real review commands.
# Four cases, each stated in session 1 and again, worded differently, in session 2:
#   A same fact            -> should merge into the first memory (no second Active memory)
#   B similar topic, other detail -> must stay a separate memory
#   C same subject, contradicting detail (a day) -> never merged away as a duplicate
#   D same subject, a changed value (a version)  -> never merged away as a duplicate
# VARIANT=1|2|3 picks the wording of the C correction.
# Temp dir and temp database; your real chloe data is not touched. Needs Ollama with qwen3:8b.
# Usage: CHLOE=/path/to/chloe ./run.sh   (default: `chloe` on PATH)
set -euo pipefail

CHLOE=${CHLOE:-chloe}
CHLOE=$(command -v "$CHLOE") || { echo "chloe binary not found; set CHLOE=" >&2; exit 1; }
tmp=$(mktemp -d)
[ -n "${KEEP:-}" ] || trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/repo" && git -C "$tmp/repo" init -q
export CHLOE_DB=$tmp/dedup.db
c() { "$CHLOE" --dir "$tmp/repo" "$@"; }
c init >/dev/null

session() { # session <id> <events.json>: write the events into the capture spool
  python3 - "$tmp" "$1" "$2" <<'PY'
import json, os, sys
tmp, sid, events = sys.argv[1:4]
d = os.path.join(tmp, "dedup.db.spool", "pending")
os.makedirs(d, exist_ok=True)
for i, (kind, tool, text) in enumerate(json.loads(events)):
    ev = {"schema_version": 1, "native_agent": "claude_code", "native_session_id": sid,
          "native_event_id": f"{sid}-{i}", "kind": kind, "cwd": os.path.join(tmp, "repo"),
          "occurred_at": f"2026-01-0{1 if sid == 's1' else 2}T00:00:{i:02d}Z",
          "tool_name": tool, "text": text}
    open(os.path.join(d, f"{sid}-{i:02d}.json"), "w").write(json.dumps(ev))
PY
}

# Wording of the staging correction in session 2 (VARIANT=1..3) so the result is not tied to one phrasing.
case "${VARIANT:-1}" in
  1) CORRECTION="Correction on staging: the database is read-only on Saturdays from 02:00 to 04:00 UTC (backup job), not Fridays." ;;
  2) CORRECTION="Heads up, staging is actually read-only on Saturdays, 02:00-04:00 UTC, because of the backup job. Please update what we know about Fridays." ;;
  3) CORRECTION="The staging database goes read-only for the backup every Saturday between 02:00 and 04:00 UTC, so imports fail then." ;;
esac
S1='[
["user_prompt",null,"Publishing the release fails with: unsigned artifact. What is going on?"],
["tool_result_summary","Bash","make publish: error unsigned artifact"],
["tool_result_summary","Bash","make sign && make publish: published v1.2.0"],
["assistant_message",null,"Root cause: the publish target does not sign the artifact itself. The release only works if you run `make sign` first and `make publish` after it."],
["user_prompt",null,"Also, the migration 014 failed on sqlite. Why?"],
["tool_result_summary","Edit","migrations/014.sql\n- ALTER TABLE jobs ADD COLUMN owner TEXT NOT NULL;\n+ ALTER TABLE jobs ADD COLUMN owner TEXT NOT NULL DEFAULT '"'"''"'"';"],
["assistant_message",null,"SQLite refuses ALTER TABLE ... ADD COLUMN with NOT NULL unless the column has a non-null DEFAULT. Adding DEFAULT empty string fixed the migration."],
["user_prompt",null,"For your information: the staging database is read-only every Friday between 02:00 and 04:00 UTC because of the backup job, so imports fail in that window."],
["user_prompt",null,"FYI the CI image runs Python 3.9, so do not use match statements in the scripts."],
["session_end",null,null]
]'
S2='[
["user_prompt",null,"our deploy script died with \"unsigned artifact\", help"],
["tool_result_summary","Bash","make publish: error unsigned artifact"],
["tool_result_summary","Bash","make sign; make publish: ok"],
["assistant_message",null,"Signing is a separate step. Run `make sign` before `make publish`; `make publish` alone never signs, which is why you saw unsigned artifact."],
["user_prompt",null,"Another one: dropping a column in sqlite with ALTER TABLE ... DROP COLUMN gives a syntax error on our CI image."],
["tool_result_summary","Bash","sqlite3 --version: 3.31.1"],
["assistant_message",null,"DROP COLUMN only exists since SQLite 3.35.0; the CI image has 3.31.1, so we have to rebuild the table instead."],
["user_prompt",null,"'"$CORRECTION"'"],
["user_prompt",null,"Update: the CI image was upgraded and now runs Python 3.12, not 3.9, so match statements are fine."],
["session_end",null,null]
]'

accept_all() {
  for id in $(c --json candidates list | python3 -c 'import json,sys; print(" ".join(x["id"] for x in json.load(sys.stdin)))'); do
    c --json candidates accept "$id" >/dev/null
  done
}
active() { c --json memory list | python3 -c '
import json,sys
for m in json.load(sys.stdin): print("  ", m["id"][:8], m["kind"], "|", m["title"])'; }

echo "== session 1: extract, then accept everything (the first copy of each fact) =="
session s1 "$S1"; c consolidate --session s1
accept_all
active
n1=$(c --json memory list | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')

echo "== session 2: same facts again, other words; one contradiction =="
session s2 "$S2"; c consolidate --session s2
echo "-- candidates after the automatic step (state, proposal) --"
c --json candidates list --state all | python3 -c '
import json,sys
for x in json.load(sys.stdin):
    if x["evidence"]["native_session_id"] == "s2":
        p = x.get("proposal") or {}
        print("  ", x["state"], "|", p.get("action"), p.get("method"), "|", x["kind"], "|", x["title"])'
echo "-- active memories now (nothing was accepted from session 2 yet) --"
active
n2=$(c --json memory list | python3 -c 'import json,sys; print(len(json.load(sys.stdin)))')

echo "== a person follows the model's proposal for what is still pending (merge / create); conflicts are left =="
c --json candidates list | python3 -c '
import json,sys
for x in json.load(sys.stdin):
    p = x.get("proposal") or {}
    if x["state"] == "pending":
        print(x["id"], p.get("action") or "create", p.get("target") or "-")' |
while read -r id action target; do
  case "$action" in
    merge) c --json candidates merge "$id" --into "$target" >/dev/null ;;
    create) c --json candidates accept "$id" >/dev/null ;;
  esac
done
active

echo "== checks =="
sign=$(c --json memory list | python3 -c '
import json,sys
print(sum(1 for m in json.load(sys.stdin) if "make sign" in (m["title"]+m["content"]).lower() or "signing" in (m["title"]+m["content"]).lower()))')
echo "Active memories about signing before publish: $sign (wanted 1)"
[ "$sign" -le 1 ] && echo "PASS A: no two equivalent Active memories" || echo "FAIL A: duplicate Active memories"
c --json memory list | python3 -c '
import json,sys
t=" ".join((m["title"]+m["content"]).lower() for m in json.load(sys.stdin))
print("PASS B: both sqlite points kept" if "default" in t and ("3.35" in t or "drop column" in t) else "FAIL B: a sqlite point is missing")'
python3 - <<PY
import json, subprocess
def run(*a):
    return json.loads(subprocess.run(["$CHLOE", "--dir", "$tmp/repo", "--json", *a], capture_output=True, text=True).stdout)
mem = run("memory", "list")
cands = run("candidates", "list", "--state", "all")
for name, subject, new in [("C staging day", "staging", "saturday"), ("D python version", "python", "3.12")]:
    text = " ".join((m["title"] + " " + m["content"]).lower() for m in mem if subject in (m["title"] + m["content"]).lower())
    new_in = lambda x: new in (x["title"] + " " + x["content"]).lower()
    merged = [x for x in cands if x["state"] == "merged" and new_in(x)]
    kept = new in text or any(x["state"] in ("conflict", "pending", "accepted") and new_in(x) for x in cands)
    print(("PASS " if kept and not merged else "FAIL ") + name + (": the new value is kept or open for a person" if kept and not merged else ": the new value was merged away as a duplicate"))
PY
echo "-- provenance of each memory --"
for id in $(c --json memory list | python3 -c 'import json,sys; print(" ".join(m["id"] for m in json.load(sys.stdin)))'); do
  echo "memory ${id:0:8}:"; c memory sources "$id" | sed 's/^/    /'
done
echo "(memories after session 1: $n1; after the automatic step of session 2: $n2)"
