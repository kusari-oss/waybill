# Phase 0 research — `unaddressable` counts patches and tarballs as software

**Feature**: `1374-exclude-members-from-coverage` | **Issue**: #1380

Eight findings. Two corrected the spec; one of those corrected a requirement
written only hours earlier, because the spec's figures were measured on one
host and the corpus pins another.

---

## R1 — The defect, and the asymmetry that hid it

Milestone 1373's counting loop runs over every resolved component.
`fill_missing_cpes` already refuses closure members for synthesis, at
`scan_cmd.rs:6616`, on the stated grounds that a member *is not installed
software*. The metric does not apply the same exclusion, so 539 patches and
tarballs are counted as software no feed can address.

**Why nothing caught it.** The file-tier version of this error was caught
during milestone 1373 because it made the emitted row **disagree with the
emitted document** on nine corpus targets. The member version makes nothing
disagree — both sides count members — so no cross-check could see it.

**Decision**: exclude by member role. **Rationale**: the synthesizer's own
reasoning, already written down, applies unchanged.

## R2 — The delta, per host, because they differ

| | darwin | linux (corpus) |
| --- | --- | --- |
| `with_candidates` | 78 → **78** | 6 → **6** |
| `addressable_without_cpe` | 166 → **166** | 163 → **162** |
| `unaddressable` | 733 → **194** | 1,446 → **360** |
| `excluded_members` | — → **539** | — → **1,087** |
| `excluded_files` | — → **0** | — → **0** |

The linux closure carries **twice** the members. Quoting darwin's figures
against the corpus is the error milestone 1366 already recorded once.

## R3 — `with_candidates` is structural; `addressable_without_cpe` is not

**0 of 539 members on darwin and 0 of 1,087 on linux carry a CPE.** That is
guaranteed by the synthesizer's guard rather than observed, so a member in
that bucket would mean the guard had changed.

**One member on linux carries an OSV-addressable PURL**: `pkg:hackage/time`,
role `fragment`. So `addressable_without_cpe` moves by 1 there and not at all
on darwin.

**This falsified FR-004 as written**, which said both counts must not change.
The spec had only darwin figures, and a rule that holds on one host read as a
general one. Corrected; the original text is preserved in the requirement so
the error is visible rather than erased.

> That `time` — a GHC boot library — is classified a build `fragment` is
> itself questionable, and is #1374's subject. It does not change what this
> feature does with it.

## R4 — File-tier is a second excluded population, and it is large

| target | components | file-tier |
| --- | --- | --- |
| `image-postgres16` | 263 | **117** |
| `haskell-language-server` | 399 | **29** |
| `maven-guice` | 61 | 13 |
| `rust-ripgrep` | 68 | 7 |

Present on **11 of 17** corpus targets. The row already excludes them, which
was verified rather than assumed: `row-sum + file-tier = total` holds exactly
today on `rust-ripgrep` (61 + 7 = 68) and `haskell-language-server`
(370 + 29 = 399).

**Decision**: count them as `excluded_files`, separately from
`excluded_members`. **Rationale**: with members counted and files not, the sum
property holds only where file inventory is off — which is 6 of 17 targets.
**Alternative rejected**: one combined `excluded` field. It sums identically
and collapses "is a source tarball" into "is a file", repeating this
milestone's own conflation one level down.

## R5 — The ordering constraint, located exactly

```text
scan_cmd.rs:4907   let cpe_coverage = fill_missing_cpes(&mut components);
scan_cmd.rs:4914   let cpe_coverage_json = cpe_coverage.wire();
scan_cmd.rs:5213   file_inventory_mode = FileInventoryMode::parse(...)
scan_cmd.rs:5315   components.extend(entries.map(into_resolved_component))  ← file-tier joins here
scan_cmd.rs:5536   cpe_coverage: Some(cpe_coverage_json.as_str())
```

File-tier components are **not in the slice** when the counts are computed —
they join 408 lines later. They are ordinary `ResolvedComponent`s carrying
`pkg:generic/file-tier?content-sha256=…`, so had they been in the slice they
would have been counted `Unaddressable`; the measurement in R4 confirms they
are not.

**Decision**: compute the five counts, including `excluded_files`, after line
5315 and before 5536. **Rationale**: it is the only window where both
populations are known. **Alternative rejected**: counting file-tier at 4907
from the walker's statistics — it would duplicate a number the component
vector already holds, and could drift from it.

## R6 — Schema `/v3`, not an amended `/v2`

`waybill-cpe-coverage/v2` has shipped in **no release**: the newest tag
`v0.14.1-nightly.20261107` was built 2026-11-06T23:12 and milestone 1373
merged 2026-11-07T07:35.

**Decision**: bump to `/v3`. **Rationale**: v2 is already in 81 committed
goldens; one version string must never describe two shapes. **Alternative
rejected**: amending v2 in place. A version that lived one day is cheaper to
spend than a version number that means two things.

## R7 — The five counts partition every corpus target

Computed over the committed goldens: the five sum to the emitted component
count on **17 of 17**, `image-postgres16` included. The contract carries the
per-target table; a target whose sum does not close is a finding.

## R8 — Blast radius

| | files |
| --- | --- |
| local goldens carrying C209 | **33** |
| corpus goldens carrying C209 | **48** |
| corpus targets whose value changes | **16 of 17** |

`image-postgres16` carries no C209 at all (its golden predates milestone 1367)
and stays excluded for #1372's drift, so it alone will not gain `/v3`.

**Counting rule, learned in milestone 1373**: `grep -rl` over the golden
directory reports 66, half of which are `*.actual.json` diagnostics rather
than tracked goldens. The tracked figure is 33.

---

## Constitution alignment

| Principle | Check |
| --- | --- |
| **I. Pure Rust, statically linked** | No dependency change. |
| **IV. Type-driven correctness** | The exclusion is a new arm on the existing closed `FeedAddressability` enum, exhaustively matched — not a filter applied before it, which would leave the enum describing a population it no longer covers. |
| **V. Specification compliance** | No native CDX/SPDX construct expresses this; C209 already exists and only its payload changes. |
| **IX. Accuracy** | The feature is this principle: the row currently calls a patch file unaddressable software. |
| **X. Transparency** | Both excluded populations get their own count rather than vanishing. |
| **XII. External enrichment** | No scan-time lookup; the OSV ecosystem list is unchanged from milestone 1373. |
