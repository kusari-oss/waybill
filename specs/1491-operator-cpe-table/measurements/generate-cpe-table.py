#!/usr/bin/env python3
"""Generate waybill-cli/src/generate/cpe_table.toml from the Rust tables (T008).

One-time conversion for spec 1491. Run from the repository root at the branch
base, before cpe.rs loses its tables:

    python3 -I -B specs/1491-operator-cpe-table/measurements/generate-cpe-table.py \
        > waybill-cli/src/generate/cpe_table.toml

What it reads:
- `GENERIC_LIBRARY_CPES` rows and the comment lines directly above each row;
- `NVD_DICTIONARY_OBSERVED` counts;
- the #1490 census (`nvd-vendor-cves-2026-11-13.tsv`), the dictionary evidence
  every pair points at (research R1). Two counts differ from it, and take the
  census value: openssl:openssl and libssh:libssh;
- the six committed title files, for each product's `=== <product> (` query
  (research R2). Where several hold one product, the most recently committed
  file is used.

Exits non-zero if any pair lacks a census row or a title query, so a gap is
loud, never a silently missing field.

The row comments are copied verbatim. They were then edited by hand in the
TOML to remove vulnerability counts and advisory identifiers, which belong in
specs/ evidence and not in a file the binary embeds (research R10). The data
fields are exactly what this script writes.
"""
import re
import subprocess
import sys

CPE_RS = "waybill-cli/src/generate/cpe.rs"
CENSUS = "specs/1375-generic-cpe-table/measurements/nvd-vendor-cves-2026-11-13.tsv"
CENSUS_DATE = "2026-11-13"
TITLE_FILES = [
    "specs/1375-generic-cpe-table/measurements/nvd-titles.txt",
    "specs/1375-generic-cpe-table/measurements/nvd-titles-existing.txt",
    "specs/1395-nix-python-cpe/measurements/nvd-titles.txt",
    "specs/nix-curated-cpe-gaps/measurements/nvd-titles.txt",
    "specs/1283-nix-allow-ifd/measurements/cpe/nvd-titles.txt",
    "specs/1375-generic-cpe-table/measurements/nvd-titles-2026-11-12.txt",
]

src = open(CPE_RS, encoding="utf-8").read().splitlines()

# --- GENERIC_LIBRARY_CPES: rows with the comment block directly above each.
start = next(i for i, l in enumerate(src) if l.startswith("const GENERIC_LIBRARY_CPES"))
end = next(i for i in range(start, len(src)) if src[i] == "];")
row_re = re.compile(r'^\s*\("([^"]+)",\s*&\[(.*)\]\),\s*$')
pair_re = re.compile(r'\("([^"]+)",\s*"([^"]+)"\)')
rows, pending = [], []
first_row_seen = False
header_comments = []
for line in src[start + 1 : end]:
    stripped = line.strip()
    if stripped.startswith("//"):
        pending.append(stripped[2:].removeprefix(" ") if stripped != "//" else "")
        continue
    m = row_re.match(line)
    if m:
        if not first_row_seen:
            # The block above the first row is about the table, not the row.
            header_comments, pending = pending, []
            first_row_seen = True
        rows.append((m.group(1), pair_re.findall(m.group(2)), pending))
        pending = []
    elif stripped:
        sys.exit(f"unparsed table line: {line!r}")

# --- NVD_DICTIONARY_OBSERVED.
ostart = next(i for i, l in enumerate(src) if l.startswith("const NVD_DICTIONARY_OBSERVED"))
oend = next(i for i in range(ostart, len(src)) if src[i] == "];")
observed = {
    (v, p): int(n)
    for v, p, n in re.findall(r'\("([^"]+)",\s*"([^"]+)",\s*(\d+)\)', "\n".join(src[ostart:oend]))
}

# --- The census, read by header name, `#` lines skipped (research R1).
census = {}
header = None
for line in open(CENSUS, encoding="utf-8"):
    line = line.rstrip("\n")
    if line.startswith("# product\t"):
        header = line[2:].split("\t")
        continue
    if not line or line.startswith("#"):
        continue
    if header is None:
        sys.exit("census has no header line")
    rec = dict(zip(header, line.split("\t")))
    census[(rec["vendor"], rec["product"])] = int(rec["dictionary-entries"])


# --- Title files, most recently committed first.
def added(path):
    out = subprocess.run(
        ["git", "log", "--diff-filter=A", "--format=%cI", "--", path],
        capture_output=True, text=True, check=True,
    ).stdout.split()
    return out[-1] if out else ""


titles = sorted(TITLE_FILES, key=added, reverse=True)
title_text = {f: open(f, encoding="utf-8").read() for f in titles}


def title_for(product):
    for f in titles:
        if f"=== {product} (" in title_text[f]:
            return f
    return None


def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


out = []
problems = []
changed = []
for slug, pairs, comments in rows:
    if out:
        out.append("")
    for c in comments:
        out.append(f"# {c}".rstrip())
    out.append("[[row]]")
    out.append(f"key = {q('pkg:generic/' + slug)}")
    out.append("pair = [")
    for vendor, product in pairs:
        n = census.get((vendor, product))
        if n is None:
            problems.append(f"{vendor}:{product}: no census row")
            continue
        if observed.get((vendor, product)) != n:
            changed.append(f"{vendor}:{product} {observed.get((vendor, product))} -> {n}")
        t = title_for(product)
        if t is None:
            problems.append(f"{vendor}:{product}: no title query for {product}")
            continue
        out.append(
            "  { "
            + ", ".join(
                [
                    f"vendor = {q(vendor)}",
                    f"product = {q(product)}",
                    f"dictionary_entries = {n}",
                    f"observed = {q(CENSUS_DATE)}",
                    f"dictionary_evidence = {q(CENSUS)}",
                    f"title_evidence = {q(t)}",
                ]
            )
            + " },"
        )
    out.append("]")

if problems:
    sys.exit("\n".join(problems))

print("# HEADER-PLACEHOLDER")
for c in header_comments:
    print(f"# {c}".rstrip())
print()
print("\n".join(out))
print(
    f"# rows={len(rows)} pairs={sum(len(p) for _, p, _ in rows)} "
    f"census-updated: {'; '.join(changed) or 'none'}",
    file=sys.stderr,
)
