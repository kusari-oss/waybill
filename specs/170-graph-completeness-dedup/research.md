# Phase 0 Research: m170 Graph-Completeness Dedup

**Feature**: 170-graph-completeness-dedup
**Date**: 2026-07-06

## R1 — Complete inventory of `go_graph_completeness` flow

**Question**: Where does the `go_graph_completeness: Option<GraphCompleteness>` value flow through the emission pipeline, and where does it produce the C44 annotation output?

**Decision**: Remove the C44 emission at THREE parallel emission sites (one per output format) and prune the entire upstream plumbing chain that fed it.

**Data collected** (via `grep -rn 'go_graph_completeness' mikebom-cli/src`):

**Emission sites** (produce the C44 annotation):
| File | Lines | Format |
|---|---|---|
| `mikebom-cli/src/generate/cyclonedx/metadata.rs` | 228-245 | CDX 1.6 |
| `mikebom-cli/src/generate/spdx/annotations.rs` | 546-567 | SPDX 2.3 |
| `mikebom-cli/src/generate/spdx/v3_annotations.rs` | 524-539 | SPDX 3.0.1 |

**Plumbing** (fields that feed the value into the emitters):
| File | Lines | Role |
|---|---|---|
| `mikebom-cli/src/generate/mod.rs` | 74-79 | `SbomEmission` struct fields `go_graph_completeness` + `go_graph_completeness_reason` |
| `mikebom-cli/src/cli/scan_cmd.rs` | 1975-1976, 2616-2617 | Populates the struct from the scan result |
| `mikebom-cli/src/generate/cyclonedx/builder.rs` | 56-62, 141-142, 296-302, 499-500 | `with_go_graph_completeness` setter + call site |
| `mikebom-cli/src/generate/cyclonedx/metadata.rs` | 53 | Function parameter of `build_metadata` |
| `mikebom-cli/src/generate/cyclonedx/mod.rs` | 59 | Threading call |
| `mikebom-cli/src/generate/spdx/document.rs` | 462-463, 492-493 | Two `SpdxAnnotationScan` construction sites (SPDX 2.3 + SPDX 3) |
| `mikebom-cli/src/generate/spdx/v3_document.rs` | 99-100 | Threading to SPDX 3 annotation writer |

**Structs that carry `None`-typed stubs** (must have the field removed, else `struct literal missing field` errors on other construction sites):
| File | Lines | Struct |
|---|---|---|
| `mikebom-cli/src/generate/openvex/mod.rs` | 246-247 | `SbomEmission` stub for OpenVEX (doesn't consume it) |
| `mikebom-cli/src/generate/spdx/mod.rs` | 388-389 | SPDX artifact struct stub |
| `mikebom-cli/src/generate/spdx/packages.rs` | 724-725 | Test-harness stub |
| `mikebom-cli/src/generate/spdx/relationships.rs` | 345-346 | Test-harness stub |
| `mikebom-cli/src/generate/spdx/document.rs` | 1169-1170 | Test-harness stub |

**Source-side consumers** (populate the value pre-emission):
| File | Lines | Role |
|---|---|---|
| `mikebom-cli/src/scan_fs/mod.rs` | ~12 references | The scan pipeline that computes `GraphCompleteness` and returns it in `ScanResult` |
| `mikebom-cli/src/scan_fs/package_db/mod.rs` | ~8 references | The `GraphCompleteness` enum definition + its producers |

**Scope decision**: Remove the fields from `SbomEmission` and every emitter/plumbing site. **KEEP** the underlying `GraphCompleteness` enum + its producers in `scan_fs/`, since scan_fs may internally track it for future reuse (issue #516 investigation). We only prune the emission chain and the `SbomEmission` struct fields — not the source-side calculation.

**Alternatives considered**:
- Keep the plumbing but stub the emission block. **Rejected**: leaves dead fields on `SbomEmission`, invites future contributors to accidentally reintroduce the emission. Cleaner to prune.
- Prune all the way back into `scan_fs/`, deleting the `GraphCompleteness` enum entirely. **Rejected**: over-aggressive. Issue #516's investigation may reveal we want to re-home the signal; keeping the source-side calculation preserves optionality at ~40 lines' cost.

## R2 — Duplicate-label absoluteness in the EXTRACTORS table

**Question**: Do any catalog rows besides C44/C104 share a `label` value? If so, the FR-004 duplicate-label gate needs an allowlist rather than an absolute rule.

**Decision**: **Absolute rule, no allowlist needed.** The only duplicate label in the current EXTRACTORS table is `"mikebom:graph-completeness"` between C44 and C104. Post-m170 (after C44 removal), every label will be unique.

**Data collected** (via `grep -oE 'label: "[^"]*"' mikebom-cli/src/parity/extractors/mod.rs | sort | uniq -d`):

```
label: "mikebom:graph-completeness"
```

That's the only match. No `E1 ecosystem completeness` vs some hypothetical peer; no non-mikebom labels (`D1 evidence — identity`, `E1 ecosystem completeness`, etc.) collide with anything. The gate can be a simple `HashMap<&str, Vec<&str>>` construction + `any(|v| v.len() > 1)` assertion.

**Alternatives considered**:
- Allowlist-based gate (small vec of known-allowed duplicate labels). **Rejected**: unnecessary complexity for zero cases.

## R3 — Affected goldens: exact shape of the C44 removal

**Question**: What byte-level changes will show up in the golden diff when C44 is removed?

**Decision**: Golden diffs will show removal of one `mikebom:graph-completeness` entry + its trailing `mikebom:graph-completeness-reason` entry (when present) from the document-scope properties/annotations arrays. Exact ranges per format:

**CDX 1.6** (`tests/fixtures/golden/cyclonedx/golang.cdx.json:911-914`):
```json
      {
        "name": "mikebom:graph-completeness",
        "value": "partial"
      },
```
This is the C44 emission. The C104 emission at line 931-934 stays. The `mikebom:graph-completeness-reason` at line 935-938 stays (C104-associated per m158's emission logic — only fires when C104's value != complete AND reason codes exist).

Note: the C44 emission in this golden does NOT have a `-reason` companion (m061's C44 emission only fired the reason when `go_graph_completeness_reason` was non-empty; in this scan it apparently was empty). So the golden diff is ONLY the removal of the 4-line properties entry — no other bytes touched.

**SPDX 2.3** — analogous position in the `annotations[]` array of the document element. Sibling-repo `mikebom-test-fixtures/tests/fixtures/spdx/golang/*.spdx.json` needs regen.

**SPDX 3.0.1** — analogous position as a graph-element `Annotation` typed entry in the `@graph[]` array. Sibling-repo `mikebom-test-fixtures/tests/fixtures/spdx3/golang/*.spdx3.json` needs regen.

**Alternatives considered**:
- Manual sed-style edit of the goldens. **Rejected**: violates the `MIKEBOM_UPDATE_GOLDENS=1 cargo test` workflow that guarantees emission-code + golden are in lockstep. Also risks byte-drift on other fields.

## R4 — SPDX 3 conformance under duplicate annotation removal

**Question**: Does removing one of the two duplicate SPDX 3 typed Annotation elements pose any risk to the milestone-078 `spdx3-validate==0.0.5` conformance gate?

**Decision**: Zero risk. The SPDX 3 spec explicitly allows any number of `Annotation` elements per subject, including zero. Removing one of two identical Annotations reduces the count from 2 → 1, which the validator's structural rules do not distinguish from any other Annotation-count transition. The validator's `Violation of type` marker set only fires on structural/typing errors (missing required properties, wrong types), not on element multiplicity.

**Data**: milestone 078 documented the validator's rule set at `specs/078-spdx3-conformance/research.md`; no rule references multi-Annotation multiplicity constraints.

## R5 — m090 sibling-repo golden regeneration workflow

**Question**: What's the exact process to regenerate the Go-ecosystem SPDX goldens in the sibling `mikebom-test-fixtures` repo?

**Decision**: Follow the established m090 process:
1. Set `MIKEBOM_FIXTURES_UPDATE=1` in the environment.
2. Run `cargo test --test '*'` from the mikebom workspace root — the m090 test-harness detects the env var, regenerates each golden, and writes to `$MIKEBOM_FIXTURES_DIR/tests/fixtures/{spdx,spdx3,cyclonedx}/**` in the sibling checkout.
3. Commit the changes in the sibling repo, tag with the pinned SHA-signal that matches the mikebom PR's commit hash.
4. Update `mikebom-cli/build.rs` to bump the pinned fixture SHA when the sibling commit lands.

**Data**: milestone 090's `specs/090-split-test-fixtures-repo/contracts/fixture-path-helper.md` and the `feedback_cross_host_goldens` memory both describe this.

**Alternatives considered**:
- Regenerate only the CDX golden and skip SPDX. **Rejected**: violates FR-005 (goldens for all three formats must be in lockstep). SC-008 requires no unrelated byte-changes on any ecosystem golden — including SPDX ones for Go.

## R6 — Companion PR sequencing

**Question**: How do we sequence the mikebom PR and the sibling-fixtures PR so CI doesn't fail on either during the transition?

**Decision**: **Two-phase merge** matching m090 precedent:
1. Open the sibling-repo PR first with the regenerated Go SPDX goldens.
2. Do NOT merge the sibling PR yet.
3. Open the mikebom PR pointing at the sibling PR's commit SHA in `mikebom-cli/build.rs`'s fixture-repo pin.
4. Merge the sibling PR first (its CI is trivial — YAML-shaped diff review).
5. Rebase the mikebom PR on top of main; its CI now sees the new pinned SHA + regenerated goldens; both green together.

**Alternatives considered**:
- Single mikebom PR with both changes squashed. **Rejected**: violates the two-repo separation the m090 split established.
- Merge mikebom first, break sibling later. **Rejected**: breaks CI on main during the interregnum.

## R7 — Does the CDX 1.6 spec allow duplicate `properties[]` entries?

**Question**: Was the duplicate emission technically spec-non-compliant, or just semantically ambiguous?

**Decision**: Duplicates are **schema-legal but consumer-ambiguous**. CDX 1.6's JSON schema declares `properties[]` as `{"type": "array", "items": {"type": "object", "properties": {"name": ..., "value": ...}}}` with no `uniqueItems` constraint. So two entries with the same `name` pass the schema. But every mainstream consumer (Syft, Trivy, Snyk, GitHub Dependency Graph) treats a name-value collection as a map when they read it; behavior on duplicates is implementation-defined (usually "last wins" in JS/Python, undefined in Rust `HashMap` iteration).

**Consumer harm class**: Silent data-quality bug rather than a schema violation. m170 elevates emission quality above what the spec strictly requires — matching the "reader-side de-facto contract" the reading guide already documents.

**Data**: verified against the CDX 1.6 schema at `mikebom-cli/tests/fixtures/schemas/cyclonedx-1.6-json.schema.json`.

## Consolidated open questions

Every question raised by the spec's Assumptions section is resolved:

- ✅ Universal m158 emission is the canonical home (spec's assumption, verified by R1's flow trace)
- ✅ C110 is the modern canonical home for Go-transitive-coverage (spec's assumption, verified by inspecting `mikebom:go-transitive-coverage` emission at metadata.rs:497)
- ✅ No consumer relies on the specific index of the duplicate entries (universal-consumer-behavior assumption, backed by R7's schema analysis)
- ✅ Duplicate-label gate can be an absolute rule (R2 finding)
- ✅ `MIKEBOM_UPDATE_GOLDENS=1 cargo test` workflow is established (spec's assumption, backed by R5)
- ✅ Change scope is user-space Rust only (verified by grep — no `mikebom-ebpf` references, no new deps)
- ✅ SPDX 3 conformance won't break (R4 finding)
