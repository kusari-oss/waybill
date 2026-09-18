# Quickstart: verifying per-resolve SBOMs

**Feature**: `911-per-resolve-sboms` (#902 items 1, 3, 4) | **Date**: 2026-09-17

The fixture built for #910 already has the shape most of this needs — two
declared resolves, one shared package name — and lives at
`waybill-cli/tests/fixtures/pants_resolve_edges/`. What it lacks is a package
pinned by **both** resolves at the **same** version, which is the case this
feature turns on.

---

## 0. See the defect before changing anything

Add a package at one version to both lockfiles, scan, and count:

```bash
./target/release/waybill sbom scan --path <fixture> --offline \
  --format cyclonedx-json --output /tmp/before.json

python3 - <<'PY'
import json, collections
d = json.load(open('/tmp/before.json'))
names = collections.Counter()
for c in d.get('components', []):
    for p in c.get('properties', []):
        if p['name'] == 'waybill:pants-resolve':
            names[p['value']] += 1
print('distinct resolve names on components:', len(names))
print(dict(names))
PY
```

**Pre-change**: the shared package names exactly one resolve, and which one
depends on read order. That is the baseline every later check needs.

Also worth capturing, because it is the framing fact: on the reported monorepo
20 of 24 resolves survive and four are empty. Confirm that number still holds
on current `main` before assuming this feature is aimed at the right thing —
#910 and #901 landed after it was measured.

---

## 1. SC-001 / SC-003 — both resolves name a shared package, every time

```bash
# same scan, twice, with read order perturbed
for i in 1 2; do
  ./target/release/waybill sbom scan --path <fixture> --offline \
    --format cyclonedx-json --output /tmp/run$i.json
done
```

**Pass**: the shared package's value is `["app","tools"]` in both runs,
byte-identical. **Fail**: either a single name, or two runs that disagree —
order-dependence is how the defect hid, since any single run looks
self-consistent.

---

## 2. SC-004 / C-1 — the single-resolve case uses the array form too

```bash
jq -r '.components[].properties[]? | select(.name=="waybill:pants-resolve") | .value' /tmp/run1.json | sort -u
```

**Pass**: every value is a JSON array, including the one-resolve ones
(`["app"]`). **Fail**: a bare string anywhere. That is the shape-varies-with-
cardinality trap the clarify step rejected, and it will reach consumers before
anyone notices.

---

## 3. SC-005 / C-3 — all three formats agree

```bash
./target/release/waybill sbom scan --path <fixture> --offline \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json --output-dir /tmp/three
```

Extract the membership for one package from each and compare, **including
order**. Three readers write this annotation (Pants Pex, Pants JVM, uv) — a
fixture exercising only one of them does not verify C-3.

---

## 4. SC-006 — declared and discovered are distinguishable from the document

Two fixtures: one declaring `[python.resolves]`, one relying on the
`3rdparty/python/*.lock` convention.

```bash
jq -r '.metadata.properties[]? | select(.name=="waybill:resolve-ownership") | .value' /tmp/*.json
```

**Pass**: the value names which resolves were declared and which were
discovered, and the two lists together account for every resolve appearing on
any component. **Fail**: counts only — that is today's behaviour and the gap
this closes.

---

## 5. SC-006a — the assumption that justified not anchoring

The convention-only fixture must still partition:

```bash
./target/release/waybill sbom scan --path <convention-fixture> --offline \
  --split=resolve --output-dir /tmp/split-discovered
```

**Pass**: one document per resolve, containing that resolve's packages, with
no anchor anywhere in the input. **Fail**: an empty or partial result — in
which case the clarify decision not to anchor discovered resolves is the thing
to reopen, not something to work around. Research R1 establishes this works
only because the split filters by membership rather than walking from a seed;
if the implementation drifts back to a walk, this is the check that catches it.

---

## 6. SC-007 / C-5 — a shared package appears in each document, undiminished

```bash
for f in /tmp/split/*.cdx.json; do
  echo "== $f"
  jq -r '.components[] | select(.name=="<shared>") | .properties[]?
         | select(.name=="waybill:pants-resolve") | .value' "$f"
done
```

**Pass**: the package is present in both documents, and in **both** it reads
`["app","tools"]`. **Fail**: narrowed to the containing document's own
resolve — that recreates the under-reporting this feature fixes, moved from
component scope to document scope and unrecoverable without re-scanning.

---

## 7. C-5a — each sub-document has a sensible root

```bash
jq -r '.metadata.component | "\(.name) \(.purl // "-")"' /tmp/split/*.cdx.json
```

**Pass**: each names its own resolve. **Fail**: a synthetic placeholder, or
the same name across every document — the failure m215 hit and left a comment
about at `split.rs:355-380`, where 23 of 25 sub-SBOMs named the repository
instead of themselves.

---

## 8. C-5b — an unpartitionable repository says so

Scan a Pants repository with no resolves, with `--split=resolve`.

**Pass**: a stated outcome and the single-document fallback. **Fail**: an
empty directory and exit zero.

---

## 9. C-6 — the existing split modes are untouched

```bash
cargo +stable test --workspace
```

**Pass**: `--split=workspace` and `--split=directory` goldens unchanged.

---

## Golden refresh

Corpus goldens are CI-generated — never locally. 72 corpus components carry
this annotation (django 34, jvm 27, python 11) and all of them churn, because
the encoding changes for single-resolve components too.

This is the third consecutive feature to churn Pants goldens. Refresh **once**,
after the whole feature lands, rather than per user story.

---

## What "done" looks like

| Check | Expectation |
|---|---|
| Distinct resolve names on components | equals the number of resolves containing packages |
| Two scans, perturbed order | byte-identical membership |
| Single-resolve component | array form |
| Convention-only repository | partitions without anchors |
| Shared package under split | full membership in every document |
| Reported monorepo re-measured | no resolve empty that is not genuinely empty |
