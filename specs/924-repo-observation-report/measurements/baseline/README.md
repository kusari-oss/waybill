# Pre-change SBOM baseline (T001, T003)

Captured **before** any change to `walk_registry/walker.rs`. SC-009 asserts
byte-identity against these, and they cannot be reconstructed once T004 lands.

## Conditions

| | |
|---|---|
| Binary | `waybill 0.9.0`, release profile |
| Commit | `cd759e02` on `924-repo-observation-report` |
| Captured | 2026-09-21T13:41:58Z |
| Network | `--offline` — no enrichment, no resolution fetches |
| deps.dev cache | irrelevant (offline); not consulted |
| ClearlyDefined cache | irrelevant (offline); not consulted |
| Host | Darwin arm64 |

A baseline whose conditions are unstated is not a baseline. Both enrichment
caches are named explicitly even though `--offline` makes them moot — in
milestone 923 a comparison was drawn between a warm-cache run and a cold-cache
run and read as a 85x regression that did not exist.

## Targets and flags

| target | flags |
|---|---|
| this repository | `--offline --exclude-path target` |
| polyglot reference repo (2,291 packages) | `--offline` |

**Why `--exclude-path target` on this repository**: maven's jar walker
declares a `descend_into` override for `target/` (m664 contract C10, task
T039), so scanning waybill's own tree walks its Rust build artifacts and
produces a **419 MB** CycloneDX document. Excluded, it is 12.8 MB. SC-009 is a
before/after comparison under identical flags, so the exclusion is sound —
but it is worth knowing that a default scan of this repository is pathological,
and it is exactly the kind of thing #932's report is meant to make visible.

## Files

| file | bytes |
|---|---|
| `poly.cyclonedx-json.json` |  4232052 |
| `poly.spdx-2.3-json.json` |  7580772 |
| `poly.spdx-3-json.json` |  11576271 |
| `self.cyclonedx-json.json` |  12832493 |
| `self.spdx-2.3-json.json` |  21466906 |
| `self.spdx-3-json.json` |  31375734 |

`*.masked` siblings are produced by `../mask.py` and are what the
comparison actually uses.

## The mask is verified, not assumed

Two consecutive runs of the **same** binary produce byte-identical output after
masking — checked at capture time. Without that property SC-009 could never
pass, and any diff seen later would be noise rather than signal.

Masked fields: `serialNumber`, `timestamp`, `created`, `creationInfo`,
tool-name strings beginning `waybill-`, plus any embedded RFC-3339 timestamp
or `urn:uuid:` literal. Derived from the m923 T009 finding that exactly two
leaves differ between two offline CycloneDX runs of 4.2 MB.

---

## T007 result — the walker change is inert

Re-run after `walk_registry/walker.rs` gained the census hook:

```
✓ cyclonedx-json   self  byte-identical
✓ cyclonedx-json   poly  byte-identical
✓ spdx-2.3-json    self  byte-identical
✓ spdx-2.3-json    poly  byte-identical
✓ spdx-3-json      self  byte-identical
✓ spdx-3-json      poly  byte-identical
```

6/6. SC-009 holds at the foundational gate. Checked here rather than at the end
of the milestone so that a regression on the hot path is attributed to the one
commit that could have caused it.
