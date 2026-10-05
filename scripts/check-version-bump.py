#!/usr/bin/env python3
"""Verify a version-bump diff touched nothing but the version (#1133).

Usage: check-version-bump.py OLD NEW [BASE_REF]

Fails if any path outside Cargo.toml, Cargo.lock, CHANGELOG.md and
waybill-cli/tests/fixtures/ changed, or if a changed fixture differs from its
BASE_REF version (default HEAD) by anything other than the tool version, in
the two places it appears (`waybill-OLD` and `"version": "OLD"`), plus the
content-addressed IDs derived from it. A dependency that happens to be at
version OLD makes this fail rather than pass, which leaves the bump for a
human, never ships a wrong golden.
"""
import re, subprocess, sys

old, new = sys.argv[1], sys.argv[2]
base = sys.argv[3] if len(sys.argv) > 3 else "HEAD"
ALLOWED = ("Cargo.toml", "Cargo.lock", "CHANGELOG.md")
FIXTURES = "waybill-cli/tests/fixtures/"

def git(*a):
    return subprocess.run(["git", *a], check=True, capture_output=True, text=True).stdout

changed = [p for p in git("diff", "--name-only", base).splitlines() if p]
untracked = [p for p in git("ls-files", "--others", "--exclude-standard").splitlines() if p]
bad = [p for p in changed + untracked if p not in ALLOWED and not p.startswith(FIXTURES)]
if bad:
    sys.exit(f"FAIL: changes outside the version bump: {bad}")

ids = re.compile(r"doc-[A-Z0-9]{20,32}|[A-Z2-7]{16}")
def norm(text, version):
    text = text.replace(f"waybill-{version}", "waybill-VERSION")
    text = text.replace(f'"version": "{version}"', '"version": "VERSION"')
    return sorted(ids.sub("ID", line) for line in text.splitlines())

fixtures = [p for p in changed if p.startswith(FIXTURES)]
mismatched = []
for p in fixtures:
    before = git("show", f"{base}:{p}")
    with open(p) as f:
        after = f.read()
    if norm(before, old) != norm(after, new):
        mismatched.append(p)
if mismatched:
    sys.exit(f"FAIL: {len(mismatched)} fixture(s) differ beyond the version: {mismatched[:10]}")
leftover = subprocess.run(
    ["git", "grep", "-l", f"waybill-{old}", "--", FIXTURES],
    capture_output=True, text=True,
).stdout.split()
if leftover:
    sys.exit(f"FAIL: waybill-{old} still present in {leftover[:10]}")
print(f"OK: {len(fixtures)} fixture(s) changed, all version-only ({old} -> {new})")
