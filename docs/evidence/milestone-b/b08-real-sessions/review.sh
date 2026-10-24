#!/usr/bin/env bash
# review.sh <run>: follow the proposal for pending candidates (merge/create); leave conflict and supersede open
R=$(cd "$(dirname "$0")" && pwd)/runs/run$1
export CHLOE_DB=$R/b08.db PATH=$R/bin:$PATH
c() { chloe --dir "$R/fixture" "$@"; }
c --json candidates list | python3 -c '
import json,sys
for x in json.load(sys.stdin):
    p = x.get("proposal") or {}
    if x["state"] == "pending":
        print(x["id"], p.get("action") or "create", p.get("target") or "-", "|", x["title"])' |
while IFS=' ' read -r id action target rest; do
  case "$action" in
    merge)  c --json candidates merge "$id" --into "$target" >/dev/null && echo "merged   $rest" ;;
    create) c --json candidates accept "$id" >/dev/null && echo "accepted $rest" ;;
    *)      echo "left open ($action) $rest" ;;
  esac
done | tee "$R/logs/review-actions.txt"
c --json memory list > "$R/logs/final-memories.json"
c --json candidates list --state all > "$R/logs/final-candidates.json"
echo "-- active memories:"; python3 -c '
import json,sys
for m in json.load(open(sys.argv[1])): print("  ", m["id"][:8], m["kind"], "|", m["title"])' "$R/logs/final-memories.json"
echo "-- still open:"; python3 -c '
import json,sys
for x in json.load(open(sys.argv[1])):
    if x["state"] in ("pending","conflict"): print("  ", x["state"], "|", x["title"])' "$R/logs/final-candidates.json"
