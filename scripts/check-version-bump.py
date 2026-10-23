#!/usr/bin/env python3
"""Verify a version-bump diff touched nothing but the version (#1133).

Usage: check-version-bump.py OLD NEW [BASE_REF]

Fails if any file was added or deleted, if any path outside Cargo.toml,
Cargo.lock, waybill-ebpf/Cargo.lock, CHANGELOG.md and
waybill-cli/tests/fixtures/ changed, or if a
changed file differs from its BASE_REF version (default HEAD) by anything
other than the version:
  - Cargo.toml and both Cargo.lock files: only `version = "OLD"` lines may
    change, to
    `version = "NEW"`, line for line. Nothing may be added, removed or moved,
    so a dependency or a lockfile source cannot slip in with the bump.
  - fixtures: the tool version in the two places it appears (`waybill-OLD`
    and `"version": "OLD"`), plus the content-addressed IDs derived from it.
CHANGELOG.md is prose and is not compared. A dependency that happens to be at
version OLD makes this fail rather than pass, which leaves the bump for a
human, never ships a wrong golden.

A second pass then greps the fixture tree for the old version, to catch a
golden the regen sweep never reached -- one that did not change at all cannot
differ beyond the version, so the comparison above is silent about it. It
greps BOTH forms, because each covers a different format: SPDX documents carry
`waybill-OLD` in a creator or annotator string, CycloneDX documents carry
`"version": "OLD"` under metadata.tools. Checking only the first left every
CycloneDX golden outside this pass.

The post-release workflow applies a patch produced by a job that ran the
whole build, and uses this script as the boundary that patch must pass
(#1133), so it must stay standard-library only and must be run from the
base revision, never from the patched tree.
"""
import json, re, subprocess, sys

old, new = sys.argv[1], sys.argv[2]
base = sys.argv[3] if len(sys.argv) > 3 else "HEAD"
ALLOWED = ("Cargo.toml", "Cargo.lock", "waybill-ebpf/Cargo.lock", "CHANGELOG.md")
FIXTURES = "waybill-cli/tests/fixtures/"
# Fixtures a test reads as *input* rather than compares against. Nothing in
# scripts/regen-goldens.sh rewrites them -- their README names the scan command
# that does, to be run only when the scan's shape changes -- so the tool version
# they carry is frozen test data, naming the build that emitted them, not a
# claim about this one. They are exempt from the completeness grep alone; if one
# does change in a bump, it is still compared like any other fixture.
INPUT_FIXTURES = ("waybill-cli/tests/fixtures/sbom_edit/",)

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
    if path == "Cargo.toml":  # the two lockfiles share the Cargo.lock rule
        headers = [l for l in lines[:i] if l.startswith("[")]
        return bool(headers) and headers[-1] == "[workspace.package]"
    # Cargo.lock: a [[package]] block with no `source` line is a local crate.
    start = max(j for j in range(i + 1) if lines[j] == "[[package]]")
    end = next((j for j in range(i, len(lines)) if lines[j] == ""), len(lines))
    return not any(l.startswith("source = ") for l in lines[start:end])

# Line for line, not by substitution: a dependency already at OLD or NEW must
# compare equal, and a substitution would rewrite it on one side only.
for p in ("Cargo.toml", "Cargo.lock", "waybill-ebpf/Cargo.lock"):
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
def norm(text):
    """Mask the tool version so only other differences survive.

    BOTH versions are masked on BOTH sides, not each side's own. Masking only
    its own left the comparison asymmetric wherever a fixture carried a
    dependency at the old version: on the base side that dependency became
    VERSION along with the tool, on the bumped side it stayed a literal, and
    the fixture read as differing beyond the version when nothing about it had
    changed. Same root cause as the completeness pass below (#1251), and
    reachable by any unmasked golden -- the corpus ones mask the tool version
    and so never change, which is the only reason this half went unseen while
    the grep failed two releases in a row.

    Masking `new` on the base side costs nothing: a version-only bump does not
    move a dependency, so a fixture where one goes from `old` to `new` is not a
    case this check is defined over.
    """
    for v in (old, new):
        text = text.replace(f"waybill-{v}", "waybill-VERSION")
        text = text.replace(f'"version": "{v}"', '"version": "VERSION"')
    return sorted(ids.sub("ID", line) for line in text.splitlines())

fixtures = [p for p in changed if p.startswith(FIXTURES)]
mismatched = []
for p in fixtures:
    before = git("show", f"{base}:{p}")
    with open(p) as f:
        after = f.read()
    if norm(before) != norm(after):
        mismatched.append(p)
if mismatched:
    sys.exit(f"FAIL: {len(mismatched)} fixture(s) differ beyond the version: {mismatched[:10]}")
def grep_fixtures(pattern):
    # Not git(): grep exits 1 when nothing matches, which is the passing case.
    # -F because a version is dotted, and an unanchored regex `.` would match
    # a fixture that merely looks like one.
    return subprocess.run(
        ["git", "grep", "-lF", pattern, "--", FIXTURES],
        capture_output=True, text=True,
    ).stdout.split()

def declares_tool_version(path, version):
    """Does this fixture still name waybill's OWN version?

    `"version": "<v>"` is the shape EVERY CycloneDX component uses, so the
    grep that looks for it alone cannot tell the tool apart from a dependency
    that happens to sit at the version being released. That is not
    hypothetical: the automated bump after v0.12.1 failed on
    `pkg:pypi/pathspec@0.12.1` in two corpus goldens, having already failed
    the v0.12.0 cut on `pkg:hackage/tasty-quickcheck@0.11.1` (#1251). Both
    were real dependencies, correctly left alone by the regen sweep.

    waybill's own entry is the one object carrying both the version and its
    name, so that pair is what identifies it. Measured across this fixture
    tree: the only non-dependency shape is `metadata.tools.components[]`.

    Fails closed. A fixture this cannot parse is reported rather than
    skipped, because the pass exists to catch a golden the sweep never
    reached and a file we cannot read is exactly that case unexamined.
    """
    try:
        with open(path, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError):
        return True
    stack = [doc]
    while stack:
        node = stack.pop()
        if isinstance(node, dict):
            if node.get("version") == version and str(node.get("name", "")).startswith("waybill"):
                return True
            stack.extend(node.values())
        elif isinstance(node, list):
            stack.extend(node)
    return False

# `waybill-<old>` names the tool directly -- it is its own name and version
# joined -- so a textual hit is conclusive and needs no structural check.
# `"version": "<old>"` is ambiguous, and gets one.
leftover = sorted(
    {path for path in grep_fixtures(f"waybill-{old}")
     if not path.startswith(INPUT_FIXTURES)}
    | {path for path in grep_fixtures(f'"version": "{old}"')
       if not path.startswith(INPUT_FIXTURES) and declares_tool_version(path, old)}
)
if leftover:
    sys.exit(f"FAIL: version {old} still present in {leftover[:10]}")
print(f"OK: {len(fixtures)} fixture(s) changed, all version-only ({old} -> {new})")
