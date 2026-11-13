#!/usr/bin/env python3
"""Attribute each NVD_DICTIONARY_OBSERVED entry to a committed evidence file.

For every (vendor, product, entries) tuple in waybill-cli/src/generate/cpe.rs,
search every tracked file under specs/ and scripts/ for the pair and a count,
in either recorded shape: `vendor:product<ws>N` (the dictionary probes), or a
`product<TAB>vendor<TAB>N` row (the #1490 vendor census). Report which files
record exactly the observed count. Run from the repository root:

    python3 -I -B specs/1491-operator-cpe-table/measurements/evidence-attribution.py

Output: one TSV line per observation —
    vendor  product  entries  exact_files(;-joined)  other_counts(file=count;...)
followed by a summary. An observation with no exact file has no committed
evidence that a data-file row could point at.
"""
import re
import subprocess
import sys

src = open("waybill-cli/src/generate/cpe.rs", encoding="utf-8").read()
start = src.index("const NVD_DICTIONARY_OBSERVED")
end = src.index("];", start)
obs = re.findall(r'\(\s*"([^"]+)",\s*"([^"]+)",\s*(\d+)\s*\)', src[start:end])

files = subprocess.run(
    ["git", "ls-files", "specs", "scripts"], capture_output=True, text=True, check=True
).stdout.split()
texts = {}
for f in files:
    try:
        texts[f] = open(f, encoding="utf-8").read()
    except (UnicodeDecodeError, IsADirectoryError):
        pass

# One pass per file: index every `vendor:product<ws>N` occurrence.
token = re.compile(r"(?<![\w.-])([\w.+-]+):([\w.+-]+)[ \t]+(\d+)\b")
tsv_row = re.compile(r"(?m)^([\w.+-]+)\t([\w.+-]+)\t(\d+)\b")
index = {}
for f, t in texts.items():
    if ":" not in t:
        continue
    for m in token.finditer(t):
        index.setdefault((m.group(1), m.group(2)), []).append((f, m.group(3)))
    # The #1490 vendor-census TSV: `product<TAB>vendor<TAB>entries<TAB>...`.
    for m in tsv_row.finditer(t):
        index.setdefault((m.group(2), m.group(1)), []).append((f, m.group(3)))

no_exact = 0
for vendor, product, entries in obs:
    hits = index.get((vendor, product), [])
    exact_files = sorted({f for f, c in hits if c == entries})
    other_s = sorted({f"{f}={c}" for f, c in hits if c != entries})
    if not exact_files:
        no_exact += 1
    print("\t".join([vendor, product, entries, ";".join(exact_files) or "-", ";".join(other_s) or "-"]))

print(f"# observations={len(obs)} with_exact_evidence={len(obs) - no_exact} without={no_exact}", file=sys.stderr)
