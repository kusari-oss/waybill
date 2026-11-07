# Quickstart: verifying this feature

**Feature**: `1373-hackage-cpe-decision` | **Issue**: #1375

An 81-file golden diff hides a wrong implementation, and every number here is
checkable against the document rather than against the code that produced it.
Four controls, in the order they catch things.

---

## Control 1 — the sum

```sh
jq -r '.metadata.properties[]|select(.name=="waybill:cpe-coverage").value' out.cdx.json
jq '.components|length' out.cdx.json
```

The three counts must sum to the component count. This is the property
`eligible_unmapped` never had, and it is the cheapest check that the
classification considered every component exactly once.

**If the sum is short**, a PURL type fell through a non-exhaustive match —
the failure mode `FeedAddressability` is an enum to prevent.

## Control 2 — recompute the partition without the implementation

```sh
python3 -I specs/1373-hackage-cpe-decision/measurements/partition.py \
    out.cdx.json specs/1373-hackage-cpe-decision/measurements/osv-ecosystems-2026-11-07.txt
```

Derives the three counts from the emitted document and the committed
ecosystem list alone. It must agree with the row exactly (SC-001).

**This is a check, not a model of the reader.** It reads the same document a
consumer reads and applies the same published list, which is precisely what a
consumer would do.

## Control 3 — no component's identifiers moved

```sh
diff <(jq -S '[.components[]|{purl,cpe,cpes}]' before.cdx.json) \
     <(jq -S '[.components[]|{purl,cpe,cpes}]' after.cdx.json)
```

Must be empty (FR-004, SC-004). This feature changes a metric; if a `cpes`
array moved, the synthesizer was touched and it should not have been.

## Control 4 — the only difference is the C209 value

Structural comparison against a pre-change binary, all three formats. The one
differing JSON path is the `waybill:cpe-coverage` record (SC-003).

**If anything else differs**, the change leaked out of the metric.

---

## The expectation, and what disagreeing with it means

On the pinned closure, aarch64-darwin:

```
with_candidates           78
addressable_without_cpe  166
unaddressable            733
                         ───
                         977  == component count
```

`unaddressable: 733` is **supposed** to be large, and is not a regression:
732 of them are `pkg:generic`, a PURL type no feed indexes. The old
`eligible_unmapped: 284` was understating this by a factor of 2.6 because its
release-shape gate excluded most of them.

> **x86_64-linux is a different closure.** The corpus pins it and its figures
> differ — `nix-closure-moat` there is 6 / 163 / 1,446 against 1,615
> components. Quoting one host's number against the other is the error
> milestone 1366 already recorded once.

---

## Re-running the external probes

The classification rests on observations of two external services. They are
committed, dated, and re-runnable:

```sh
# NVD's CPE dictionary — the 1-of-163 figure
scripts/probe-cpe-dictionary.sh haskell:aeson haskell:base

# OSV, by the PURL waybill actually emits
python3 -I specs/1373-hackage-cpe-decision/measurements/osv-purl-direct.py \
    pkg:generic/openssl@3.0.0 pkg:hackage/aeson@2.0.0.0 pkg:pypi/urllib3@1.26.4

# OSV's declared ecosystem list
curl -sS https://osv-vulnerabilities.storage.googleapis.com/ecosystems.txt
```

**Controls are not optional.** Three harnesses in this milestone produced
plausible wrong answers — an orphaned sweep that exited 0 after 4 of 171
pairs, a run that fused tracebacks into its data rows, and an OSV probe that
returned `ERR` for every input including its positive control. A probe whose
positive control does not come back non-zero is reporting on itself.
