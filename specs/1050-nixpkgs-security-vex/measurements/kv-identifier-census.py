#!/usr/bin/env python3
"""What identifier shapes occur in nixpkgs `meta.knownVulnerabilities`? (#1051)

The #1050 classifier treats an entry as a vulnerability claim when it names a
CVE (`CVE-\\d{4}-\\d+`) and as prose otherwise. #1051 found non-CVE advisory
ids (`Sonatype-2015-0286`) filed as prose. Before widening the pattern, this
enumerates every string literal inside every `knownVulnerabilities` list in a
nixpkgs tree and classifies its shape, so the pattern can be chosen from what
occurs rather than from what might.

Static: reads `.nix` source, does not evaluate. It therefore sees literals only;
an entry built by interpolation (`"CVE-${x}"`) is reported separately as
interpolated, not guessed at. That is the trade for covering the whole tree in
seconds rather than evaluating all of nixpkgs.

Usage: kv-identifier-census.py <nixpkgs-checkout>
"""
import collections
import os
import re
import sys

ROOT = sys.argv[1]

CVE = re.compile(r"\bCVE-\d{4}-\d{4,}\b")
# Advisory-id shapes worth asking about: PREFIX-YEAR-NUMBER, GHSA, OSV-style.
SHAPES = [
    ("CVE", CVE),
    ("GHSA", re.compile(r"\bGHSA(-[23456789cfghjmpqrvwx]{4}){3}\b")),
    ("PREFIX-YYYY-N", re.compile(r"\b[A-Z][A-Za-z]{1,15}-(19|20)\d{2}-\d{3,}\b")),
]
STRING = re.compile(r'"((?:[^"\\]|\\.)*)"|\'\'(.*?)\'\'', re.S)


def lists_in(text):
    """Yield the bracketed body after each `knownVulnerabilities`."""
    for m in re.finditer(r"knownVulnerabilities\s*=", text):
        i = text.find("[", m.end())
        # Only follow a list that starts within this binding (allows
        # `lib.optionals cond [` and similar wrappers).
        if i == -1 or ";" in text[m.end():i]:
            continue
        depth = 0
        for j in range(i, len(text)):
            if text[j] == "[":
                depth += 1
            elif text[j] == "]":
                depth -= 1
                if depth == 0:
                    yield text[i + 1 : j]
                    break


entries = []  # (file, literal)
files = 0
for dirpath, _, names in os.walk(os.path.join(ROOT, "pkgs")):
    for n in names:
        if not n.endswith(".nix"):
            continue
        p = os.path.join(dirpath, n)
        try:
            text = open(p, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        if "knownVulnerabilities" not in text:
            continue
        files += 1
        for body in lists_in(text):
            for m in STRING.finditer(body):
                lit = m.group(1) if m.group(1) is not None else m.group(2)
                entries.append((os.path.relpath(p, ROOT), " ".join(lit.split())))

shape_counts = collections.Counter()
non_cve_ids = collections.Counter()
interpolated = 0
examples = {}
for f, lit in entries:
    if "${" in lit:
        interpolated += 1
    hit = None
    for name, rx in SHAPES:
        if rx.search(lit):
            hit = name
            break
    if hit is None:
        hit = "prose"
    shape_counts[hit] += 1
    examples.setdefault(hit, (f, lit[:90]))
    if hit != "CVE" and hit != "prose":
        for name, rx in SHAPES[1:]:
            for mm in rx.finditer(lit):
                non_cve_ids[mm.group(0).split("-")[0]] += 1

print(f"files declaring knownVulnerabilities: {files}")
print(f"string-literal entries:              {len(entries)}  (distinct: {len(set(l for _, l in entries))})")
print(f"entries containing interpolation:    {interpolated}")
print("entries by first-matching shape:")
for k, v in shape_counts.most_common():
    print(f"  {k:<14} {v:>4}   e.g. {examples[k][1]!r}  ({examples[k][0]})")
print("non-CVE identifier prefixes found:", dict(non_cve_ids) or "none")
# Prose entries that LOOK identifier-ish but match no shape: the false-negative
# surface of any pattern chosen from the table above.
loose = re.compile(r"\b[A-Za-z]+-\d{2,}(-\d+)*\b")
suspicious = sorted({lit for _, lit in entries if not any(rx.search(lit) for _, rx in SHAPES) and loose.search(lit)})
print(f"prose entries containing other dash-number tokens: {len(suspicious)}")
for s in suspicious[:15]:
    print("   ", s[:110])
