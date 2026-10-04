#!/usr/bin/env python3
"""#1058 probe: how deps.dev reports each outcome, in the batch endpoint
(POST /v3alpha/versionbatch, waybill's default since #923) and the per-key
GET /v3/systems/{s}/packages/{n}/versions/{v}. One request mixing a real
version, a missing version, a missing package, a placeholder version and a
malformed (artifact-only) maven name. Prints, per key: batch item shape, and
the GET status."""
import json, http.client, urllib.parse
KEYS = [
    ("NPM",   "express", "4.18.2"),                                   # exists
    ("NPM",   "express", "999.0.0"),                                  # missing version
    ("NPM",   "waybill-probe-1058-nonexistent", "1.0.0"),             # missing package
    ("GO",    "go.opentelemetry.io/otel/bridge/opencensus", "v0.0.0-unknown"),  # placeholder
    ("MAVEN", "guava", "33.0.0-jre"),                                 # malformed: no groupId
    ("MAVEN", "com.google.guava:guava", "33.0.0-jre"),                # correct
]
c = http.client.HTTPSConnection("api.deps.dev", timeout=30)
body = json.dumps({"requests": [{"versionKey": {"system": s, "name": n, "version": v}} for s, n, v in KEYS]})
c.request("POST", "/v3alpha/versionbatch", body=body, headers={"Content-Type": "application/json"})
r = c.getresponse(); raw = r.read()
print(f"batch HTTP {r.status}, {len(raw)} bytes")
d = json.loads(raw)
for i, item in enumerate(d.get("responses", [])):
    req = item.get("request", {}).get("versionKey", {})
    ver = item.get("version")
    shape = "version=null/absent" if not ver else f"version present, licenses={ver.get('licenses')}"
    print(f"  [{i}] {KEYS[i][0]}/{KEYS[i][1]}@{KEYS[i][2]}: keys={sorted(item.keys())} -> {shape}")
print("nextPageToken:", repr(d.get("nextPageToken")))
for s, n, v in KEYS:
    path = "/v3/systems/%s/packages/%s/versions/%s" % (s.lower(), urllib.parse.quote(n, safe=""), urllib.parse.quote(v, safe=""))
    c.request("GET", path); g = c.getresponse(); gb = g.read()
    print(f"GET {s}/{n}@{v}: HTTP {g.status} {gb[:80]!r}")
