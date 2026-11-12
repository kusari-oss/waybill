#!/usr/bin/env python3
"""Produce waybill's GHC boot-library table (milestone 966, #966).

Usage:
    python3 -I -B scripts/ghc-boot-libraries.py \
        > waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv

Contract: specs/966-ghc-boot-versions/contracts/boot-library-table.md

Sources, in precedence order:
  1. GHC's release notes, "Included libraries" table (release-notes).
  2. Stack's global-hints.yaml, pinned to STACK_SHA, for libraries the notes
     omit and for releases with no notes table (stack-global-hints).
  3. OVERRIDES: disagreements between 1 and 2, settled from GHC's own
     ghc-X.Y.Z-release tag (ghc-release-tag). Each entry cites what it read.

The probe fails on any notes-vs-Stack disagreement that has no override, so a
new disagreement cannot land silently. Downloads go to a temporary directory
that is removed on exit; nothing is written into the repository.

Stdlib only.
"""

import datetime
import html.parser
import os
import re
import shutil
import sys
import tempfile
import urllib.error
import urllib.request

STACK_SHA = "e677b42ee39c72ea19975674db996d7a37c23e4a"
STACK_URL = (
    "https://raw.githubusercontent.com/commercialhaskell/stackage-content/"
    + STACK_SHA
    + "/stack/global-hints.yaml"
)
NOTES_URL = "https://downloads.haskell.org/ghc/{v}/docs/users_guide/{v}-notes.html"
TAG_URL = "https://github.com/ghc/ghc/tree/ghc-{v}-release"

# Libraries a GHC ships only on Windows.
WINDOWS_ONLY = {"Win32"}

# (ghc, library) -> (version, citation). Applied after the notes/Stack merge.
OVERRIDES = {
    ("9.12.3", "process"): (
        "1.6.26.1",
        "tag ghc-9.12.3-release pins libraries/process at submodule commit "
        "f7d51387ba7f7f6079f3a9d5ce011ad9359b7dbb, whose process.cabal says "
        "version: 1.6.26.1 (release notes agree; Stack says 1.6.26.0)",
    ),
}

VERSION_RE = re.compile(r"^[0-9]+(\.[0-9]+)*$")
RELEASE_RE = re.compile(r"^ghc-([0-9]+)\.([0-9]+)\.([0-9]+):\s*$")
ENTRY_RE = re.compile(r"^\s+(\S+):\s*'?([^'\s]+)'?\s*$")


def fetch(url, dest):
    """Fetch url to dest. Returns the HTTP status (404 is not an error)."""
    req = urllib.request.Request(url, headers={"User-Agent": "waybill-ghc-boot-probe"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r, open(dest, "wb") as f:
            shutil.copyfileobj(r, f)
            return r.status
    except urllib.error.HTTPError as e:
        if e.code == 404:
            return 404
        raise


def parse_hints(path):
    """global-hints.yaml -> {"9.10.3": {lib: version}} for 9.x and later."""
    out = {}
    cur = None
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = RELEASE_RE.match(line)
            if m:
                major = int(m.group(1))
                cur = None
                if major >= 9:
                    cur = "{}.{}.{}".format(m.group(1), m.group(2), m.group(3))
                    out[cur] = {}
                continue
            if not line.startswith(" "):
                cur = None
                continue
            m = ENTRY_RE.match(line)
            if m and cur is not None:
                out[cur][m.group(1)] = m.group(2)
    return out


class IncludedLibraries(html.parser.HTMLParser):
    """Collects (package, version) rows of the first table after the element
    whose id is "included-libraries"."""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.in_section = False
        self.in_table = False
        self.done = False
        self.in_cell = False
        self.row = None
        self.cell = []
        self.rows = []

    def handle_starttag(self, tag, attrs):
        if self.done:
            return
        if dict(attrs).get("id") == "included-libraries":
            self.in_section = True
        if not self.in_section:
            return
        if tag == "table":
            self.in_table = True
        elif self.in_table and tag == "tr":
            self.row = []
        elif self.in_table and tag == "td" and self.row is not None:
            self.in_cell = True
            self.cell = []

    def handle_endtag(self, tag):
        if self.done or not self.in_table:
            return
        if tag == "td" and self.in_cell:
            self.in_cell = False
            self.row.append(" ".join("".join(self.cell).split()))
        elif tag == "tr" and self.row is not None:
            if len(self.row) >= 2 and self.row[0]:
                self.rows.append((self.row[0], self.row[1]))
            self.row = None
        elif tag == "table":
            self.in_table = False
            self.done = True

    def handle_data(self, data):
        if self.in_cell:
            self.cell.append(data)


def parse_notes(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        p = IncludedLibraries()
        p.feed(f.read())
    return dict(p.rows)


def vkey(v):
    return tuple(int(x) for x in v.split("."))


def main():
    tmp = tempfile.mkdtemp(prefix="ghc-boot-probe-")
    try:
        hints_path = os.path.join(tmp, "global-hints.yaml")
        if fetch(STACK_URL, hints_path) != 200:
            sys.exit("could not fetch " + STACK_URL)
        hints = parse_hints(hints_path)

        rows = []
        failures = []
        used_overrides = set()
        notes_releases = 0
        for v in sorted(hints, key=vkey):
            notes = {}
            npath = os.path.join(tmp, "notes-{}.html".format(v))
            if fetch(NOTES_URL.format(v=v), npath) == 200:
                notes = parse_notes(npath)
            if notes:
                notes_releases += 1
            stack = hints[v]
            for lib in sorted(set(notes) | set(stack)):
                in_notes = notes.get(lib)
                in_stack = stack.get(lib)
                override = OVERRIDES.get((v, lib))
                if override is not None:
                    version, source = override[0], "ghc-release-tag"
                    used_overrides.add((v, lib))
                elif in_notes is not None:
                    if in_stack is not None and in_stack != in_notes:
                        failures.append(
                            "{} {}: release notes {} vs stack {} (no override)".format(
                                v, lib, in_notes, in_stack
                            )
                        )
                        continue
                    version, source = in_notes, "release-notes"
                else:
                    version, source = in_stack, "stack-global-hints"
                if not VERSION_RE.match(version):
                    failures.append("{} {}: version {!r} is not dotted digits".format(v, lib, version))
                    continue
                platform = "windows" if lib in WINDOWS_ONLY else "all"
                rows.append((v, lib, version, source, platform))

        unused = set(OVERRIDES) - used_overrides
        if unused:
            failures.append("overrides matched no row: {}".format(sorted(unused)))
        if failures:
            for f in failures:
                print("ghc-boot-libraries: " + f, file=sys.stderr)
            sys.exit(1)

        rows.sort(key=lambda r: (vkey(r[0]), r[1].encode()))
        out = sys.stdout
        out.write("# observed: {}\n".format(datetime.date.today().isoformat()))
        out.write("# sources: {}#included-libraries\n".format(NOTES_URL.replace("{v}", "<v>")))
        out.write("#          {}\n".format(STACK_URL))
        out.write("#          {} (overrides only)\n".format(TAG_URL.replace("{v}", "<v>")))
        for (v, lib), (ver, cite) in sorted(OVERRIDES.items()):
            out.write("# override: {} {} {}: {}\n".format(v, lib, ver, cite))
        out.write(
            "# releases: {}; with release-notes tables: {}; rows: {}\n".format(
                len(hints), notes_releases, len(rows)
            )
        )
        out.write(
            "# regenerate: python3 -I -B scripts/ghc-boot-libraries.py > "
            "waybill-cli/src/scan_fs/package_db/nix/haskell_packages/ghc_boot_libraries.tsv\n"
        )
        out.write("ghc\tlibrary\tversion\tsource\tplatform\n")
        for r in rows:
            out.write("\t".join(r) + "\n")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
