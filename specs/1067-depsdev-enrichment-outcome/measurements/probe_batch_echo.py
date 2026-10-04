#!/usr/bin/env python3
"""#1058 / analysis U1 probe: does the batch endpoint answer every requested
key, and echo it exactly as sent? Names chosen where deps.dev might normalise:
pypi case and _/-, nuget case, npm scope, maven group:artifact, go module case.
A request with no response item, or an echo differing from what was sent,
would become a false absence in waybill today (research R2)."""
import json, http.client
KEYS = [
    ("PYPI", "Flask", "3.0.0"), ("PYPI", "flask", "3.0.0"),
    ("PYPI", "typing_extensions", "4.9.0"), ("PYPI", "typing-extensions", "4.9.0"),
    ("NUGET", "Newtonsoft.Json", "13.0.3"), ("NUGET", "newtonsoft.json", "13.0.3"),
    ("NPM", "@types/node", "20.10.0"), ("NPM", "Express", "4.18.2"),
    ("MAVEN", "com.google.guava:guava", "33.0.0-jre"),
    ("GO", "github.com/BurntSushi/toml", "v1.3.2"), ("GO", "github.com/burntsushi/toml", "v1.3.2"),
    ("CARGO", "serde", "1.0.197"), ("CARGO", "Serde", "1.0.197"),
    ("PYPI", "flask", "3.0.0"),   # duplicate of key 1 in the same request
]
c = http.client.HTTPSConnection("api.deps.dev", timeout=30)
c.request("POST", "/v3alpha/versionbatch",
          body=json.dumps({"requests": [{"versionKey": {"system": s, "name": n, "version": v}} for s, n, v in KEYS]}),
          headers={"Content-Type": "application/json"})
r = c.getresponse(); d = json.loads(r.read())
resp = d.get("responses", [])
print(f"HTTP {r.status}: {len(KEYS)} requested, {len(resp)} response items, nextPageToken={d.get('nextPageToken')!r}")
echoes = [(e["request"]["versionKey"]["system"], e["request"]["versionKey"]["name"], e["request"]["versionKey"]["version"], "version" in e) for e in resp]
for i, k in enumerate(KEYS):
    match = [e for e in echoes if (e[0], e[1], e[2]) == k]
    print(f"  {k[0]:5} {k[1]}@{k[2]:12} exact-echo={'yes' if match else 'NO'} found={match[0][3] if match else '-'}")
extra = [e for e in echoes if (e[0], e[1], e[2]) not in KEYS]
print("echoes matching no request:", extra or "none")

# Part 2: each spelling in a request of its own. Separates "deps.dev
# normalises the echo" from "deps.dev merges two spellings sent together".
print("\n-- each key alone --")
for s, n, v in KEYS[:-1]:
    c.request("POST", "/v3alpha/versionbatch",
              body=json.dumps({"requests": [{"versionKey": {"system": s, "name": n, "version": v}}]}),
              headers={"Content-Type": "application/json"})
    d = json.loads(c.getresponse().read())
    items = d.get("responses", [])
    echo = items[0]["request"]["versionKey"]["name"] if items else None
    print(f"  {s:5} sent={n!r:28} echoed={echo!r:28} same={echo == n} found={bool(items and 'version' in items[0])}")
