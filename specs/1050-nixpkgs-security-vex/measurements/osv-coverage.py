#!/usr/bin/env python3
"""Does OSV already know what nixpkgs declares, and can a Nix scan ask it?

Two questions, and the second is the one that matters:

1. Does OSV hold the CVEs nixpkgs flags?
2. Under an identity a Nix-built component actually carries?

**Query by package, never by CVE id.** OSV's ecosystem advisories have
their own ids (GHSA-, PYSEC-) and carry the CVE only as an *alias*, so
`/v1/vulns/CVE-...` returns the NVD-derived record — which routinely has
`affected[0].package == null` and no version ranges. An earlier version of
this probe did exactly that and concluded OSV coverage was poor. It is not;
the method was wrong. A scanner queries by package, so this does too.

Usage: osv-coverage.py
"""
import json, urllib.request

API = "https://api.osv.dev/v1/query"


def query(payload):
    req = urllib.request.Request(
        API, data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=25) as r:
        return json.load(r).get("vulns", [])


def show(label, payload):
    try:
        v = query(payload)
        ids = ", ".join(x["id"] for x in v[:3])
        print(f"  {label:<34} {len(v):>3}  {ids}")
        return len(v)
    except Exception as e:
        print(f"  {label:<34} err: {str(e)[:40]}")
        return None


print("A language-ecosystem package nixpkgs flags (alerta-server, CVE-2026-34400):")
show("PyPI/alerta-server@9.0.4", {"package": {"name": "alerta-server",
                                             "ecosystem": "PyPI"}, "version": "9.0.4"})

print("\nA C package nixpkgs flags, across OSV's distro ecosystems:")
for eco in ["Debian:12", "Alpine:v3.20", "Ubuntu:22.04"]:
    show(f"{eco}/unzip", {"package": {"name": "unzip", "ecosystem": eco}})

print("\nThe identity waybill emits for a Nix closure component:")
show("pkg:generic/unzip@6.0", {"package": {"purl": "pkg:generic/unzip@6.0"}})

print("""
Reading:

  OSV is complete for language ecosystems -- nixpkgs adds nothing there.

  For system packages OSV is complete PER DISTRO, and Nix is not one of the
  distros. There is no `Nix:<release>` ecosystem. Borrowing Debian's
  advisories for a Nix-built package would report DEBIAN's patch state,
  which is the wrong answer: milestone 1035 measured Nix's own `unzip 6.0`
  carrying 11 CVE-named patches that Debian's advisory set knows nothing
  about.

  So nixpkgs' knownVulnerabilities is not a weaker OSV. It is the
  distro-specific security metadata OSV would carry if Nix were a
  supported ecosystem in it.""")
