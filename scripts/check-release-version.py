#!/usr/bin/env python3
"""Refuse a patch release that adds something (#1219).

Usage: check-release-version.py [CHANGELOG [CARGO_TOML]]

`post-release.yml` leaves `Cargo.toml` holding the next *patch* version as a
placeholder (RELEASING.md §2 step 10). A release that inherits that placeholder
without being asked about it ships new surface under a version that promises
none: cargo treats `0.Y.Z` and `0.Y.Z+1` as one compatibility range, so a caret
requirement resolves a patch release silently.

This refuses the one case a machine can decide. If the release being cut is a
patch and its changelog section has a non-empty `### Added`, it is not a patch.
That is SemVer's definition, not a heuristic, so a failure means either the
version is wrong or an entry is filed under the wrong heading.

**What it cannot decide**, and why RELEASING.md §2 step 1 still asks a human:
whether an entry under `### Changed` or `### Fixed` is observable to a consumer.
`waybill:root-selection-heuristic` changing from an envelope to a bare string is
a bug fix and a wire break at once, and no amount of reading the changelog's
shape establishes which. v0.11.0 carried five of those. This check would have
caught that release on its `### Added` alone — which is the point: it is a floor,
not a verdict.

Deciding what is being released, from files only, no git:

  NEW  = [workspace.package] version in Cargo.toml
  the changelog's first released section (`## [X.Y.Z]`, skipping
  `[Unreleased]`) is the release being cut

  If that section's version != NEW, `Cargo.toml` is ahead of the changelog:
  a development state between releases, where the placeholder is correct and
  there is nothing to judge. Reported as such, never as a pass.

  Otherwise OLD = the next released section down, and the two decide the class.
"""
import re
import sys

CHANGELOG = sys.argv[1] if len(sys.argv) > 1 else "CHANGELOG.md"
CARGO = sys.argv[2] if len(sys.argv) > 2 else "Cargo.toml"

SEMVER = re.compile(r"^(\d+)\.(\d+)\.(\d+)(?:-(.+))?$")


def parse(v):
    m = SEMVER.match(v.strip())
    if not m:
        return None
    major, minor, patch, pre = m.groups()
    return (int(major), int(minor), int(patch), pre)


def workspace_version(path):
    """The `version` under [workspace.package], not a dependency's."""
    section = None
    for line in open(path, encoding="utf-8"):
        line = line.rstrip("\n")
        if line.startswith("["):
            section = line
            continue
        if section == "[workspace.package]" and line.startswith("version = "):
            return line.split('"')[1]
    return None


def released_sections(text):
    """[(version, body), ...] in file order, `[Unreleased]` excluded."""
    parts = re.split(r"^## \[([^\]]+)\]([^\n]*)\n", text, flags=re.M)
    out = []
    for i in range(1, len(parts), 3):
        ver, body = parts[i], parts[i + 2]
        if ver.lower() == "unreleased":
            continue
        out.append((ver, body))
    return out


def added_entries(body):
    """Top-level bullets under `### Added`. Nested bullets are indented."""
    m = re.search(r"^### Added\s*$(.*?)(?=^### |\Z)", body, flags=re.M | re.S)
    if not m:
        return []
    return [ln for ln in m.group(1).splitlines() if ln.startswith("- ")]


def main():
    new_raw = workspace_version(CARGO)
    if new_raw is None:
        sys.exit(f"FAIL: no [workspace.package] version in {CARGO}")
    new = parse(new_raw)
    if new is None:
        sys.exit(f"FAIL: {CARGO} version {new_raw!r} is not SemVer")

    with open(CHANGELOG, encoding="utf-8") as f:
        sections = released_sections(f.read())
    if not sections:
        sys.exit(f"FAIL: {CHANGELOG} has no released sections")

    top_ver, top_body = sections[0]
    if top_ver != new_raw:
        print(
            f"NOT A RELEASE: {CARGO} is at {new_raw}, the changelog's latest "
            f"released section is {top_ver}. This is a development state "
            f"between releases; the patch placeholder is correct here and "
            f"there is nothing to judge."
        )
        return

    if new[3] is not None:
        print(f"PRE-RELEASE: {new_raw} carries a pre-release suffix; the patch rule is for stable releases.")
        return

    # The predecessor is the previous *stable* release, and never this one's
    # own pre-releases: `0.10.0-alpha.2` parses to the same (0, 10, 0) numerics
    # as `0.10.0`, which would read as a patch bump from itself and let any
    # `### Added` through. 0.10.0's predecessor is 0.9.0.
    prev = None
    for ver, _ in sections[1:]:
        cand = parse(ver)
        if cand is None or cand[3] is not None:
            continue
        if cand[:3] == new[:3]:
            continue
        prev = cand
        break
    if prev is None:
        print(f"FIRST RELEASE: {new_raw} has no stable predecessor in {CHANGELOG}.")
        return

    if new[0] != prev[0]:
        cls = "major"
    elif new[1] != prev[1]:
        cls = "minor"
    else:
        cls = "patch"

    added = added_entries(top_body)
    if cls == "patch" and added:
        first = added[0][:96]
        sys.exit(
            f"FAIL: {new_raw} is a patch bump from "
            f"{prev[0]}.{prev[1]}.{prev[2]}, but its changelog section has "
            f"{len(added)} `### Added` entr{'y' if len(added) == 1 else 'ies'}. "
            f"Something added is not a patch (RELEASING.md §2 step 1).\n"
            f"       first entry: {first}\n"
            f"       Either the version is wrong, or that entry belongs under "
            f"Changed or Fixed."
        )

    print(
        f"OK: {new_raw} is a {cls} bump from "
        f"{prev[0]}.{prev[1]}.{prev[2]}, with {len(added)} `### Added` "
        f"entr{'y' if len(added) == 1 else 'ies'}."
    )


main()
