#!/usr/bin/env python3
"""Evaluation only (B-16): ask a cloud model to judge the 39 reviewed frozen-set candidates
and compare with the human final rulings in review-frozen.md. Reads DEEPSEEK_API_KEY,
DEEPSEEK_BASE_URL, DEEPSEEK_MODEL from the environment; sends only the hand-made sessions.
Usage: python3 verify_frozen.py [out.json]"""
import json, os, re, sys, urllib.request

here = os.path.dirname(os.path.abspath(__file__))
sessions = {s["id"]: s for s in json.load(open(f"{here}/sessions-frozen.json"))["sessions"]}
KIND = {"user_prompt": "user", "assistant_message": "assistant", "tool_use": "tool", "tool_result": "tool"}

RULES = """You check whether a memory candidate is faithful to the session events it came from.
Split the candidate (title and body) into claims: what was done, who proposed or decided it, why, the scope, numbers/commands/negations. A claim is supported only if the events show it.
Rules:
- Who did it matters: what the assistant did or reported is the assistant's, not the user's or "the team's".
- An explicit assistant report may support a candidate that restates it as a report of that one run. Only "independently verified", a wider result or a stronger conclusion is unsupported.
- A concrete fix is fine; writing a one-off fix, command or workaround as a general rule, guarantee or requirement ("must", "always", "all", "standard", "known issue") widens the scope unless the events say so.
- A reason, motive, cause or benefit must be stated by an event; plausible inference does not count.
- Numbers, commands and negations must match the events (e.g. range(3) is 3 attempts, not 3 retries).
- Judge faithfulness only, not whether the candidate is worth saving.
Answer JSON only: {"verdict": "faithful" | "partial" | "unfaithful", "unsupported": [{"claim": "...", "type": "invented_motive|wrong_attribution|widened_scope|generalisation|inferred_cause|wrong_detail"}]}.
"faithful" = no unsupported claim; "partial" = some; "unfaithful" = the main claim is unsupported or contradicted."""

def events_text(s):
    out = []
    for e in s["events"]:
        who = KIND.get(e["kind"], e["kind"])
        tool = f" {e['tool']}" if e.get("tool") else ""
        out.append(f"[{e['id']}] {who}{tool}: {e.get('text') or ''}")
    return "\n".join(out)

rows = []
for l in open(f"{here}/review-frozen.md"):
    if re.match(r"\| \d+ \|", l):
        c = [x.strip() for x in l.rstrip("\n").split("|")[1:-1]]
        rows.append(dict(n=c[0], sid=c[1], kind=c[3], title=c[4], body=c[5], final=c[11], dev=c[12]))
assert len(rows) == 39

def call(prompt):
    req = urllib.request.Request(
        os.environ["DEEPSEEK_BASE_URL"].rstrip("/") + "/chat/completions",
        data=json.dumps({
            "model": os.environ["DEEPSEEK_MODEL"], "temperature": 0,
            "response_format": {"type": "json_object"},
            "messages": [{"role": "system", "content": RULES}, {"role": "user", "content": prompt}],
        }).encode(),
        headers={"Content-Type": "application/json",
                 "Authorization": "Bearer " + os.environ["DEEPSEEK_API_KEY"]})
    r = json.load(urllib.request.urlopen(req, timeout=180))
    return json.loads(r["choices"][0]["message"]["content"])

res = []
for r in rows:
    p = f"Session events:\n{events_text(sessions[r['sid']])}\n\nCandidate ({r['kind']}):\nTitle: {r['title']}\nBody: {r['body']}"
    try:
        v = call(p)
    except Exception as e:  # report and continue; never print the key
        v = {"verdict": "error", "unsupported": [], "error": type(e).__name__}
    res.append({**r, "model": v})
    print(r["n"], r["final"], "->", v["verdict"], flush=True)
json.dump(res, open(sys.argv[1] if len(sys.argv) > 1 else f"{here}/verify-frozen.json", "w"), ensure_ascii=False, indent=1)

MAP = {"忠实": "faithful", "部分忠实": "partial", "不忠实": "unfaithful"}
fail = lambda x: x != "faithful"
agree = sum(MAP[r["final"]] == r["model"]["verdict"] for r in res)
tp = sum(fail(MAP[r["final"]]) and fail(r["model"]["verdict"]) for r in res)
fn = sum(fail(MAP[r["final"]]) and not fail(r["model"]["verdict"]) for r in res)
fp = sum(not fail(MAP[r["final"]]) and fail(r["model"]["verdict"]) for r in res)
print(f"exact agreement {agree}/39; human-failing caught {tp}, missed {fn}; false alarms {fp}")
