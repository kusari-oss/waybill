#!/usr/bin/env python3
"""Would deps.dev confirm the nix closure's PyPI candidates? (#1365, milestone 1366)

Milestone 1366 proposes giving a nix-closure component a `pkg:pypi` identifier
only when the existing enrichment resolution path confirms the coordinate.
Spec SC-001 deliberately asserts no target number: 75 candidates exist, and
what fraction survives confirmation was unmeasured when the spec was written.

This probe measures it, BEFORE the implementation that depends on it — the
standard CLAUDE.md sets ("Measure external behaviour before designing around
it"). It asks deps.dev exactly what waybill's `DepsDevClient::get_version`
asks, at the same URL shape (`deps_dev_client.rs:190`), so the answer is about
the production path rather than about this script.

What it is NOT: a model of waybill's confirmation. It issues the same request
and reads the same three outcomes (200 / 404 / other), which is the most a
probe can honestly claim.

Usage:
    nix derivation show --recursive <flakeref>#default > closure.json
    python3 -I depsdev-confirmation-probe.py closure.json [--json out.json]

Two carrier gotchas inherited from `pypi-identity-probe.py`, repeated because
each produced a confident wrong number first:

* `pname` lives in the flat `env` map on most derivations and in the top-level
  `structuredAttrs` carrier on others — 66 of 1,275 on the moat closure.
* The candidate rule needs the **versioned** interpreter prefix. A bare
  `python-` prefix matches nixpkgs setup hooks (build machinery, not
  packages), and bootstrap variants carry the prefix inside `pname` itself.
"""
import argparse
import collections
import concurrent.futures
import json
import re
import sys
import urllib.error
import urllib.parse
import urllib.request

BASE = "https://api.deps.dev/v3"
VERSIONED_PY = re.compile(r"^python3\.\d+-")
TIMEOUT = 15
WORKERS = 8


def attr(drv, key):
    """A derivation attribute from whichever carrier holds it."""
    env = drv.get("env", {}) or {}
    if key in env:
        return env[key]
    structured = drv.get("structuredAttrs")
    if isinstance(structured, dict) and key in structured:
        return structured[key]
    return None


def pep503(name):
    """PyPI's normalised project name."""
    return re.sub(r"[-_.]+", "-", str(name)).lower()


def candidates(closure):
    """(normalised_name, version) pairs the milestone-1366 rule produces."""
    out = set()
    for drv in closure.values():
        name = drv.get("name") or ""
        pname = attr(drv, "pname")
        if not pname or not VERSIONED_PY.match(name):
            continue
        if str(pname).startswith("python3"):
            continue
        version = attr(drv, "version")
        if not version:
            continue
        out.add((pep503(pname), str(version)))
    return sorted(out)


def probe(pair):
    """One GetVersion call. Returns (name, version, outcome, detail).

    Outcomes mirror waybill's `LookupResult` (`depsdev_source.rs:50`):
      found    <- HTTP 200          -> Confirmed
      absent   <- HTTP 404          -> Declined   (an answer, not a failure)
      failed   <- anything else     -> Unchecked
    """
    name, version = pair
    url = "{}/systems/pypi/packages/{}/versions/{}".format(
        BASE, urllib.parse.quote(name, safe=""), urllib.parse.quote(version, safe="")
    )
    req = urllib.request.Request(url, headers={"User-Agent": "waybill-spec-probe/1366"})
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            body = json.load(r)
            return (name, version, "found", body.get("versionKey", {}).get("name", ""))
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return (name, version, "absent", "404")
        return (name, version, "failed", "HTTP {}".format(e.code))
    except Exception as e:  # noqa: BLE001 - a probe reports, it does not raise
        return (name, version, "failed", type(e).__name__)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("closure")
    ap.add_argument("--json", dest="out")
    args = ap.parse_args()

    raw = json.load(open(args.closure))
    closure = raw["derivations"] if "derivations" in raw else raw
    pairs = candidates(closure)
    print("closure derivations: {}".format(len(closure)))
    print("candidate coordinates: {}".format(len(pairs)))

    with concurrent.futures.ThreadPoolExecutor(max_workers=WORKERS) as pool:
        results = list(pool.map(probe, pairs))

    counts = collections.Counter(r[2] for r in results)
    print(
        "\nfound(confirm) {found}  absent(decline) {absent}  failed(unchecked) {failed}".format(
            found=counts["found"], absent=counts["absent"], failed=counts["failed"]
        )
    )

    for outcome in ("absent", "failed"):
        rows = [r for r in results if r[2] == outcome]
        if rows:
            print("\n{} ({}):".format(outcome, len(rows)))
            for n, v, _, d in sorted(rows):
                print("    {:34s} {:16s} {}".format(n, v, d))

    # deps.dev echoes the name it holds. Where that differs from the name we
    # asked under, PEP 503 normalisation did real work and the two spellings
    # are the same project.
    renamed = [r for r in results if r[2] == "found" and r[3] and pep503(r[3]) != r[0]]
    print("\nfound under a name whose normalised form differs: {}".format(len(renamed)))
    for n, v, _, d in renamed:
        print("    asked {:28s} -> holds {}".format(n, d))

    if args.out:
        with open(args.out, "w") as f:
            json.dump(
                [
                    {"name": n, "version": v, "outcome": o, "detail": d}
                    for n, v, o, d in results
                ],
                f,
                indent=2,
                sort_keys=True,
            )
        print("\nwrote {}".format(args.out))


if __name__ == "__main__":
    main()
