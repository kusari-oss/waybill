# Phase 0 research — what `eligible_unmapped` is actually counting

**Feature**: `1373-hackage-cpe-decision` | **Issue**: #1375

Nine findings. Five came from probing an external service, and four of those
contradicted something written in the issue or in this spec's own first draft.

---

## R1 — The issue's figures were stale before planning began

#1375 was filed against a 942-component document. Milestone 1372 (#1366)
landed in between and took it to 977, moving every number quoted.

| | as filed | now |
| --- | --- | --- |
| `pkg:hackage` components | 138 | **163** |
| `eligible_unmapped` | 264 | **284** |

**Decision**: the issue was corrected in place rather than left to be read as
current, and its title updated. **Rationale**: a stale number in an open issue
is read as a measurement.

## R2 — NVD's dictionary has 1 of 163 Haskell names

All 171 `haskell:<name>` pairs probed — every name in the closure plus the 8
capitalised ones re-probed lowercased. **Zero unanswered.** Controls
`openssl:openssl` = 385 and the fixture negative = 0.

**One pair has an entry**: `haskell:aeson`, 95. Case makes no difference.

**Decision**: no `hackage` arm (FR-007). **Rationale**: the ratio is the weak
argument. The strong one is that `base`, `Cabal` and `process` carry live
HSEC advisories *and* have no dictionary entry, so a blanket arm emits
identifiers matching nothing for exactly the packages here that have known
vulnerabilities — the silent zero `probe-cpe-dictionary.sh` exists to prevent.
**Alternative rejected**: a dictionary-verified single-entry table on the
`GENERIC_LIBRARY_CPES` pattern. One row, for one package, in one closure, that
OSV already reaches through its PURL, against a standing obligation to re-probe
as NVD changes.

## R3 — OSV reaches waybill's own emitted PURLs, end to end

Six of 163 return advisories when queried with the exact PURL strings in the
document. Version filtering is demonstrably applied: `aeson` returns 2 by name
and 1 by versioned PURL, `scientific` 2 and 1, and `cmark-gfm` drops out
entirely because its version is outside every affected range.

**Decision**: `pkg:hackage` is feed-addressable. **Rationale**: measured on
waybill's identifiers rather than on names, which is the property that matters.

## R4 — `pkg:generic` is addressable by nothing, and that is the real gap

| PURL | advisories |
| --- | --- |
| `pkg:pypi/urllib3@1.26.4` | 22 |
| `pkg:npm/lodash@4.17.11` | 6 |
| `pkg:hackage/aeson@2.0.0.0` | 2 |
| `pkg:generic/openssl@3.0.0` | **0** |
| `pkg:generic/zlib@1.2.11` | **0** |
| `pkg:generic/curl@7.60.0` | **1** — `OSV-2017-79`, an OSS-Fuzz finding |

`openssl 3.0.0` and `zlib 1.2.11` both carry well-known CVEs and return
nothing: `pkg:generic` names no registry to index against. For these
components a CPE is the only route to any feed, which is why
`GENERIC_LIBRARY_CPES` exists.

## R5 — A non-zero probe is not sufficient, and the first rule was wrong

This spec's FR-003 originally read "traceable to a recorded probe" with a
non-zero result as the test. **That rule classifies `generic` as addressable**,
on the strength of the single OSS-Fuzz record above.

OSV publishes its ecosystem list. Fetched 2026-11-07: **47 entries**, with
`Hackage`, `PyPI`, `npm`, `Go`, `crates.io`, `Maven`, `Debian`, `Alpine`,
`RubyGems` and `NuGet` present and **no `generic` or `Generic`**.

**Decision**: a type is addressable only when it is in OSV's declared list
**and** a probe returns that ecosystem's advisories. **Rationale**: either
half alone admits a false positive — list membership without a probe asserts
coverage that may not exist, and a probe without list membership counts
OSS-Fuzz project-name hits as registry coverage. The list is committed as
`measurements/osv-ecosystems-2026-11-07.txt`.

## R6 — C209 is **not** closure-gated, and this spec said it was

Four statements in the first draft assumed C209 appears only on closure
documents. It appears on **every** document: one production site sets it, and
it is always `Some`.

| | files carrying C209 |
| --- | --- |
| local goldens | **66** |
| corpus goldens | **48** |

**Consequence**: replacing `eligible_unmapped` churns all of them. This is the
**inverse of milestone 1372**, where zero local goldens moved, and larger than
milestone 1367, which added this row and churned 34. FR-005 and SC-003 were
rewritten; the original text is preserved in FR-005 so the error is visible
rather than erased.

## R7 — `image-postgres16` lacks C209 because its golden is stale

It is the one target reading no C209 at all, which looked like a second code
path. It is not: the milestone-1372 regeneration artifact for the same target
**does** carry the row. Its committed golden predates milestone 1367 and was
deliberately excluded from this repository's last two golden refreshes because
of the unrelated layer-digest drift in #1372.

**Decision**: it stays excluded here too, and the plan says so rather than
letting a 17th changed target appear unexplained.

## R8 — The partition, and the half of the story that inverted

| bucket | components | composition |
| --- | --- | --- |
| `with_candidates` | **78** | 72 `pypi`, 6 curated `generic` |
| `addressable_without_cpe` | **166** | 163 `hackage`, 3 `pypi` |
| `unaddressable` | **733** | 732 `generic`, 1 `github` |
| | **977** | equals the component count |

**`eligible_unmapped: 284` was understating the real gap**, not only
overstating the Haskell share. Its release-shape gate excluded most
`pkg:generic` components, so the count of components no feed can address is
**733**. The issue asked whether 284 overstated a gap; it did both, in
different places, and the understatement is the one that costs an operator
something.

Across the corpus the current row is similarly misleading in both directions —
`npm-express` reads `eligible_unmapped: 0`, which looks like complete
coverage, while 43 of its 45 components carry no CPE at all and are reachable
only because npm is an OSV ecosystem.

**Decision**: the three counts partition **every** component, not the subset
passing the synthesizer's eligibility gate. **Rationale**: it makes FR-002
checkable against the component count, and it is the only shape that accounts
for the 24 versionless `pkg:hackage` components, which are in no count today.

## R9 — Three harness failures, each of which produced a plausible wrong answer

Recorded because each would have been reported as a finding.

1. **The first NVD sweep was orphaned** after 4 of 171 pairs — launched with
   `( … ) &` in a shell that then exited — and its wrapper still reported exit
   0. Four rows, all zero, read exactly like "every pair is absent". Caught by
   the row count, not by the status.
2. **The second interleaved `2>&1` tracebacks into its data lines**, fusing
   some of them, so rows had to be recovered by anchoring on the pair name
   rather than the line start. 64 of 171 were left unanswered, and an `ERR`
   counted as a zero would have understated dictionary presence.
3. **The first OSV probe returned `ERR` for every input including its positive
   control** — a shell-quoting defect, not an empty ecosystem. Uniform results
   across cases that must differ are the signature of a broken harness.

Every figure quoted above comes from a run whose controls behaved.

---

## Constitution alignment

| Principle | Check |
| --- | --- |
| **I. Pure Rust, statically linked** | No new dependency. Probes are measurement-time only and ship nothing. |
| **IV. Type-driven correctness** | The three buckets become an enum-driven classification with an exhaustive match, not an allowlist that falls out of date (`feedback_enum_allowlist_falls_out_of_date`). |
| **V. Specification compliance** | No native CDX/SPDX field expresses "which feed can address this component", so a `waybill:*` row remains correct. C209 already exists; this changes its payload. |
| **IX. Accuracy** | The whole feature is this principle: the current row is wrong in both directions. |
| **X. Transparency** | A reader can verify the counts against the component count, which they cannot do today. |
| **XII. External enrichment** | No scan-time network access is added. The ecosystem list is a committed, dated observation, not a runtime lookup. |
