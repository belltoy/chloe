import json,sys
d=sys.argv[1]
for tag in ["s1-after-accept","s2-after-consolidate"]:
    print("==",tag,"candidates")
    for x in json.load(open(f"{d}/logs/{tag}-candidates.json")):
        p=x.get("proposal") or {}
        print(f' {x["state"]:9}|{p.get("action")} {p.get("method")}|{x["kind"]}|{x["title"]}  [{(p.get("reason") or "")[:110]}]')
    print("==",tag,"memories")
    for m in json.load(open(f"{d}/logs/{tag.replace('candidates','')}-memories.json".replace("after-consolidate","after-consolidate"))):
        print(" ",m["id"][:8],m["status"] if "status" in m else "",m["kind"],"|",m["title"])
