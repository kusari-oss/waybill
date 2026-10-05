#!/usr/bin/env python3
"""Verify a version-bump diff touched nothing but the version (#1133).

Usage: check-version-bump.py OLD NEW [BASE_REF]

Fails if any file was added or deleted, if any path outside Cargo.toml,
Cargo.lock, CHANGELOG.md and waybill-cli/tests/fixtures/ changed, or if a
changed file differs from its BASE_REF version (default HEAD) by anything
other than the version:
  - Cargo.toml and Cargo.lock: only `version = "OLD"` lines may change, to
    `version = "NEW"`, line for line. Nothing may be added, removed or moved,
    so a dependency or a lockfile source cannot slip in with the bump.
  - fixtures: the tool version in the two places it appears (`waybill-OLD`
    and `"version": "OLD"`), plus the content-addressed IDs derived from it.
CHANGELOG.md is prose and is not compared. A dependency that happens to be at
version OLD makes this fail rather than pass, which leaves the bump for a
human, never ships a wrong golden.

The post-release workflow applies a patch produced by a job that ran the
whole build, and uses this script as the boundary that patch must pass
(#1133), so it must stay standard-library only and must be run from the
base revision, never from the patched tree.
"""
import re, subprocess, sys

old, new = sys.argv[1], sys.argv[2]
base = sys.argv[3] if len(sys.argv) > 3 else "HEAD"
ALLOWED = ("Cargo.toml", "Cargo.lock", "CHANGELOG.md")
FIXTURES = "waybill-cli/tests/fixtures/"

def git(*a):
    return subprocess.run(["git", *a], check=True, capture_output=True, text=True).stdout

status = [l.split("\t", 1) for l in git("diff", "--name-status", "--no-renames", base).splitlines() if l]
not_modified = [p for s, p in status if s != "M"]
untracked = [p for p in git("ls-files", "--others", "--exclude-standard").splitlines() if p]
if not_modified or untracked:
    sys.exit(f"FAIL: files added or deleted: {(not_modified + untracked)[:10]}")
changed = [p for _, p in status]
bad = [p for p in changed if p not in ALLOWED and not p.startswith(FIXTURES)]
if bad:
    sys.exit(f"FAIL: changes outside the version bump: {bad}")

def workspace_version_line(path, lines, i):
    """Is line i the workspace's own version, not a dependency's?"""
    if path == "Cargo.toml":
        headers = [l for l in lines[:i] if l.startswith("[")]
        return bool(headers) and headers[-1] == "[workspace.package]"
    # Cargo.lock: a [[package]] block with no `source` line is a local crate.
    start = max(j for j in range(i + 1) if lines[j] == "[[package]]")
    end = next((j for j in range(i, len(lines)) if lines[j] == ""), len(lines))
    return not any(l.startswith("source = ") for l in lines[start:end])

# Line for line, not by substitution: a dependency already at OLD or NEW must
# compare equal, and a substitution would rewrite it on one side only.
for p in ("Cargo.toml", "Cargo.lock"):
    if p in changed:
        before = git("show", f"{base}:{p}").splitlines()
        with open(p) as f:
            after = f.read().splitlines()
        ok = len(before) == len(after) and all(
            b == a or (b == f'version = "{old}"' and a == f'version = "{new}"'
                       and workspace_version_line(p, before, i))
            for i, (b, a) in enumerate(zip(before, after))
        )
        if not ok:
            sys.exit(f"FAIL: {p} differs beyond `version = \"{old}\"` -> `\"{new}\"`")

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
