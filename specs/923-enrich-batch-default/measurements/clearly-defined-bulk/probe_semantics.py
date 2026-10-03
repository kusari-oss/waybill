#!/usr/bin/env python3
"""Probe ClearlyDefined GET /definitions/{coord} vs POST /definitions."""
import json, sys, time, urllib.request, urllib.parse, urllib.error

BASE = "https://api.clearlydefined.io"
UA = {"User-Agent": "waybill-probe/933", "Content-Type": "application/json"}

def enc(seg):  # mirrors clearly_defined_coord.rs::url_encode
    out = []
    for ch in seg:
        if ch.isascii() and (ch.isalnum() or ch in "-._~@"):
            out.append(ch)
        else:
            out.extend("%%%02X" % b for b in ch.encode())
    return "".join(out)

def get(coord):
    path = "/".join(enc(s) for s in coord.split("/", 4))
    t = time.time()
    try:
        with urllib.request.urlopen(urllib.request.Request(f"{BASE}/definitions/{path}", headers=UA), timeout=30) as r:
            body = json.load(r); status = r.status
    except urllib.error.HTTPError as e:
        body, status = None, e.code
    except (TimeoutError, OSError) as e:
        body, status = None, f"ERR:{type(e).__name__}"
    return status, body, time.time() - t

def post(coords):
    t = time.time()
    req = urllib.request.Request(f"{BASE}/definitions", data=json.dumps(coords).encode(), headers=UA, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            return r.status, json.load(r), time.time() - t
    except urllib.error.HTTPError as e:
        return e.code, e.read()[:300].decode(errors="replace"), time.time() - t
    except (TimeoutError, OSError) as e:
        return f"ERR:{type(e).__name__}", None, time.time() - t

def declared(body):
    return ((body or {}).get("licensed") or {}).get("declared")

COORDS = [
    "npm/npmjs/-/express/4.18.2",
    "npm/npmjs/@types/node/18.0.0",
    "npm/npmjs/types/node/18.0.0",
    "crate/cratesio/-/serde/1.0.200",
    "pypi/pypi/-/django/4.2.0",
    "pypi/pypi/-/Django/4.2.0",
    "gem/rubygems/-/rails/7.0.0",
    "maven/mavencentral/org.apache.commons/commons-lang3/3.12.0",
    "go/golang/golang.org%2fx/text/v0.14.0",
    "go/golang/golang.org/x/text/v0.14.0",
    "npm/npmjs/-/this-package-does-not-exist-933/1.0.0",
]

if __name__ == "__main__":
    print("== GET per coordinate")
    for c in COORDS:
        s, b, dt = get(c)
        print(f"{s} {dt*1000:6.0f}ms declared={declared(b)!r:28} keys={sorted((b or {}).keys())[:6] if isinstance(b, dict) else None} {c}")
    print("== POST bulk")
    s, b, dt = post(COORDS)
    print(f"status={s} {dt*1000:.0f}ms")
    if isinstance(b, dict):
        print("response keys:", json.dumps(sorted(b.keys()), indent=0))
        for c in COORDS:
            v = b.get(c)
            print(f"  {'present' if c in b else 'ABSENT ':7} declared={declared(v)!r:28} {c}")
    else:
        print(b)
