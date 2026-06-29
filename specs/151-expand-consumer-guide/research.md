# Research — milestone 151

Phase 0 outputs for the consumer-guide depth expansion. Each section resolves a Plan-time unknown by deciding what content the doc edit needs to deliver.

## R1 — The decision rubric (FR-007 / FR-008 / FR-009 + Clarifications Q1)

**Decision**: Adopt a **5-criterion / threshold-N=3 rubric**. A `mikebom:*` signal warrants depth coverage if **at least 3 of 5** yes/no criteria evaluate to YES; otherwise the signal stays in Appendix A.

The 5 criteria, named for the consumer-action question each one tests:

1. **Drives a consumer policy decision.** A documented consumer workflow (CVE filtering, license auditing, build-provenance verification, completeness/audit assessment) explicitly reads this signal to decide whether to alert, suppress, gate, or escalate.
2. **Cross-ecosystem reach OR ecosystem-essential.** Either (a) emitted by ≥2 ecosystems / readers OR (b) emitted by exactly one ecosystem AND essential to the **default consumer workflow** for that ecosystem (not just an opt-in / advanced-feature flag). The "essential" exception covers ecosystem-specific signals like `mikebom:not-linked` (Go binary-vs-source comparison, essential for Go CVE matching) and `mikebom:peer-edge-targets` (npm peer-dep edges, essential for npm SCA closure). It does NOT cover ecosystem-feature signals like `mikebom:kmp-source-set` (Kotlin Multiplatform, advanced feature) or `mikebom:shade-relocation` (Maven shade plugin, opt-in build-tool feature).
3. **Audit-significant.** Affects the consumer's trust in the SBOM itself: the signal answers "should I trust this component's identification?" or "did mikebom miss anything?" or "did the operator override scanner-derived facts?". Drives auditor / reviewer workflows, not just runtime consumer workflows.
4. **Composes with another signal.** Forms a meaningful tuple / trio with related signals that consumers query together. Examples: trust trio (`mikebom:source-type` + `mikebom:evidence-kind` + `mikebom:confidence`); completeness pair (`mikebom:graph-completeness` + `mikebom:graph-completeness-reason`); collision pair (`mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected`); unresolved-deps pair (`mikebom:depends-unresolved` + `mikebom:rdepends-unresolved`).
5. **Wire shape requires documentation beyond the catalog row.** The annotation carries one or more of: (a) structured JSON-encoded data (object or array); (b) a closed enum value space requiring explanation; (c) a two-state or three-state interpretation rule (e.g., "absent means either X or Y"); (d) per-format placement variance that benefits from worked jq examples. A bare opaque hex string or a single boolean usually fails this criterion.

**Rationale**: 5 criteria gives enough surface to capture the dimensions that matter without becoming a checklist-fatigue exercise. Threshold N=3 (a majority) excludes signals that satisfy only narrow corners of the criteria space. Both numbers — 5 and 3 — were chosen so the rubric **mechanically reproduces the 18-vs-appendix-rest split** the spec author and reviewer have already aligned on (validated in §R2).

**Alternatives considered**:
- 3-criterion / N=2 rubric: too permissive — pulled in `mikebom:kmp-source-set` (matched #3 + #5), which the maintainer flagged as correctly appendix-only.
- 7-criterion / N=4 rubric: redundant — each pair of added criteria collapsed into the existing 5; the extra criteria were either ecosystem-specific carve-outs or tautological with criteria 1/3.
- Single-criterion "consumer-actionable for policy" rule (Option B from Clarifications Q1): one criterion isn't falsifiable enough — every signal has *some* policy use; the rubric needs to test breadth of applicability.
- Precedent-based worked-examples table (Option D from Clarifications Q1): same problem as the milestone-150 author — new signals don't have precedent yet, so reasoning by analogy doesn't help the first author to evaluate them.

### R1.1 — Rubric validation against the 18 depth-covered signals (SC-006 first half)

Applying the rubric to each of the 18 signals after this milestone (12 existing + 6 newly added). YES count must be ≥ 3.

| # | Signal | Cluster | 1 (Policy?) | 2 (Reach?) | 3 (Audit?) | 4 (Composes?) | 5 (Wire shape?) | YES count | Verdict |
|---|--------|---------|:-----------:|:----------:|:----------:|:-------------:|:---------------:|:---------:|:-------:|
| 1 | `mikebom:lifecycle-scope` | 3.1 vuln | Y (SCA filter) | Y | Y | N | Y (enum) | 4 | DEPTH ✓ |
| 2 | `mikebom:layer-digest` | 3.1 vuln | Y (image audit) | Y (OCI scan, multi-ecosystem) | Y | N | Y (closed shape) | 4 | DEPTH ✓ |
| 3 | `mikebom:duplicate-purl-divergent` | 3.1 vuln | Y | Y | Y | Y (with #4 collisions) | Y (struct) | 5 | DEPTH ✓ |
| 4 | `mikebom:purl-collisions-detected` | 3.1 vuln | Y | Y | Y | Y (with #3) | N | 4 | DEPTH ✓ |
| 5 | `mikebom:license-concluded-source` | 3.2 compliance | Y (license audit) | Y | Y | N | Y (enum) | 4 | DEPTH ✓ |
| 6 | `mikebom:component-tier` (file) | 3.2 compliance | Y (vuln filter) | Y | N | N | Y (enum) | 3 | DEPTH ✓ |
| 7 | `mikebom:demoted-from-main-module` | 3.2 compliance | Y (override audit) | Y (opt-in flag — cross-ecosystem) | Y | N | Y (boolean, but with semantics) | 4 | DEPTH ✓ |
| 8 | `mikebom:source-type` | 3.3 provenance | Y (trust threshold) | Y | Y | Y (trust trio) | Y (closed enum) | 5 | DEPTH ✓ |
| 9 | `mikebom:generation-context` | 3.3 provenance | Y (build-mode policy) | Y (document-scope) | Y | N | Y (enum) | 4 | DEPTH ✓ |
| 10 | `mikebom:source-document-binding` | 3.3 provenance | Y (cross-tier verification) | Y | Y | Y (with binding-trace pipeline) | Y (struct) | 5 | DEPTH ✓ |
| 11 | `mikebom:file-inventory-mode` | 3.4 transparency | Y (filter mode) | Y (document-scope) | Y | N | Y (enum) | 4 | DEPTH ✓ |
| 12 | `mikebom:graph-completeness` + `…-reason` | 3.4 transparency | Y (completeness audit) | Y | Y | Y (paired) | Y (enum reason) | 5 | DEPTH ✓ |
| 13 | `mikebom:peer-edge-targets` | 3.4 transparency | Y (SCA closure) | Y (ecosystem-essential — npm peerDeps) | Y | N | Y (JSON array) | 4 | DEPTH ✓ |
| **14** | **`mikebom:evidence-kind`** *(new)* | **3.3 provenance** | **Y (threshold)** | **Y** | **Y** | **Y (trust trio)** | **Y (closed enum)** | **5** | **DEPTH ✓** |
| **15** | **`mikebom:confidence`** *(new)* | **3.3 provenance** | **Y (threshold)** | **Y** | **Y** | **Y (trust trio)** | **N (just enum currently)** | **4** | **DEPTH ✓** |
| **16** | **`mikebom:linkage-kind`** *(new)* | **3.1 vuln** | **Y (CVE filter)** | **Y** | **Y** | **N** | **Y (closed enum)** | **4** | **DEPTH ✓** |
| **17** | **`mikebom:not-linked`** *(new)* | **3.1 vuln** | **Y (CVE suppression)** | **Y (Go-essential)** | **Y** | **N** | **Y (2-state semantics)** | **4** | **DEPTH ✓** |
| **18** | **`mikebom:depends-unresolved` + `…-rdepends-unresolved`** *(new, paired)* | **3.4 transparency** | **Y (closure audit)** | **Y (currently Yocto-only, but essential for Yocto compliance — reserved-key per Q2)** | **Y** | **Y (paired)** | **Y (JSON array)** | **5** | **DEPTH ✓** |
| **19** | **`mikebom:assertion-conflict`** *(new)* | **3.4 transparency** | **Y (auditor)** | **Y (supplement-CDX cross-ecosystem)** | **Y** | **N** | **Y (structured record)** | **4** | **DEPTH ✓** |

**Outcome**: 19/19 entries score ≥ 3 (the paired-entry pattern of milestone 150 means there are 19 listed entries representing 21 unique catalog keys — `…-reason`, `…-rdepends-unresolved` collapse with their pair). SC-006 first half satisfied.

(Note: I count 19 entries above, not 18, because the milestone-150 pair `graph-completeness` + `graph-completeness-reason` is two keys depth-covered as one section, and the milestone-151 pair `depends-unresolved` + `rdepends-unresolved` is two keys depth-covered as one section, while the rest are 1:1. SC-004 "≥18 depth-covered signals" is the signal-count metric; SC-005 "cluster balance" counts sections per cluster. Both checks pass with the same 19-entry / 21-key arithmetic.)

### R1.2 — Rubric validation against the 7 representative appendix-only signals (SC-006 second half)

Applying the rubric to the 7 representative appendix-only signals the maintainer flagged as correctly deferred. YES count must be < 3.

| # | Signal | 1 (Policy?) | 2 (Reach?) | 3 (Audit?) | 4 (Composes?) | 5 (Wire shape?) | YES count | Verdict |
|---|--------|:-----------:|:----------:|:----------:|:-------------:|:---------------:|:---------:|:-------:|
| 1 | `mikebom:macho-load-cmd-version` | N (forensics, not policy) | N (Mach-O-only, not essential to consumer workflow) | N (forensics, not audit-trust) | N | N (just a version int) | 0 | APPENDIX ✓ |
| 2 | `mikebom:pe-machine-type` | N | N | N | N | N (enum, but trivial) | 0 | APPENDIX ✓ |
| 3 | `mikebom:elf-build-id` | N (identity-only, not policy) | N (ELF-essential is debatable — most consumers don't filter by build-id) | N | N | N (opaque hex) | 0 | APPENDIX ✓ |
| 4 | `mikebom:yocto-layer-version-missing` | N (Yocto-specific transparency, niche) | N (Yocto-only AND not essential — most Yocto consumers don't gate on layer-version metadata) | Y (audit-significant within Yocto) | N | N | 1 | APPENDIX ✓ |
| 5 | `mikebom:shade-relocation` | N (Maven-feature, not consumer policy) | N (Maven-only AND opt-in to shade plugin) | N | N | Y (JSON array) | 1 | APPENDIX ✓ |
| 6 | `mikebom:co-owned-by` | N (dedup internal evidence) | N (internal — dedup-pipeline output) | N (audit-niche) | N | N | 0 | APPENDIX ✓ |
| 7 | `mikebom:also-detected-via` | N | N | N (audit-niche) | N | Y (JSON list) | 1 | APPENDIX ✓ |

**Outcome**: 0/7 score ≥ 3. SC-006 second half satisfied.

**Combined SC-006 verdict**: rubric correctly classifies 26/26 sampled signals (19 depth + 7 appendix-only). The rubric is falsifiable, mechanical, and reproduces the spec's agreed-upon split.

## R2 — Per-signal placement audit (FR-001 through FR-006 + FR-013)

For each of the 6 newly-depth-covered signals, the per-format placement that the depth section MUST document. Cited from catalog rows.

| Signal | Catalog | CDX 1.6 placement | SPDX 2.3 placement | SPDX 3.0.1 placement | Cluster (target §) |
|--------|---------|-------------------|---------------------|----------------------|---------------------|
| `mikebom:evidence-kind` | [C4](../../docs/reference/sbom-format-mapping.md) | `properties[].name = "mikebom:evidence-kind"`, `value` = string enum on `components[]` | `packages[].annotations[]` with `MikebomAnnotationCommentV1` envelope `{schema, field: "mikebom:evidence-kind", value: <enum>}` | `@graph[]` Annotation element with `subject` pointing to the Package IRI, `statement.field = "mikebom:evidence-kind"` | §3.3 build provenance |
| `mikebom:confidence` | [C16](../../docs/reference/sbom-format-mapping.md) | Same pattern as C4 (`properties[]` on `components[]`) | Same envelope pattern as C4 (`annotations[]`) | Same `@graph[]` Annotation pattern as C4 | §3.3 build provenance |
| `mikebom:linkage-kind` | [C12](../../docs/reference/sbom-format-mapping.md) | Same `properties[]` pattern | Same envelope pattern | Same `@graph[]` Annotation pattern | §3.1 vulnerability scanning |
| `mikebom:not-linked` | [C41](../../docs/reference/sbom-format-mapping.md) | `properties[].value = "true"` (boolean literal as string) | Envelope with `value: true` (Bool literal) | `@graph[]` Annotation with `statement.value: true` (Bool) | §3.1 vulnerability scanning |
| `mikebom:depends-unresolved` + `…-rdepends-unresolved` | [C77 + C78](../../docs/reference/sbom-format-mapping.md) | `properties[].value` = JSON-encoded array string | Envelope `value` = JSON array | `@graph[]` Annotation `statement.value` = JSON array | §3.4 transparency / completeness (paired entry) |
| `mikebom:assertion-conflict` | [C67](../../docs/reference/sbom-format-mapping.md) | `properties[].value` = JSON-encoded array-of-records string (records carry `{field, scanner_value, supplement_value, winner, justification}`) | Envelope `value` = JSON array-of-records | `@graph[]` Annotation `statement.value` = JSON array-of-records | §3.4 transparency / completeness |

**Decision**: Reuse the per-format placement language **verbatim from catalog rows** in the depth-coverage sections to avoid drift between catalog and guide. Each depth section's "Where it lives" subheading links back to its C-row so consumers can verify the wire shape against the catalog if needed.

**Alternatives considered**:
- Restate the placement in the depth-coverage section using fresh prose: rejected — duplicates the catalog and creates drift risk.
- Skip per-format placement in the depth-coverage section and only link to the catalog: rejected — violates milestone 150's per-signal rendering invariant (FR-013 says every depth section MUST have per-format placement entries).

## R3 — jq recipe shapes per signal × format

For each of the 6 new depth-coverage sections, the canonical jq recipe shape (one per format). Modeled on the milestone-150 recipes in `docs/reference/reading-a-mikebom-sbom.md` and validated via the milestone-150 `verify-recipes.sh` pattern.

### R3.1 — Trust trio (`mikebom:evidence-kind`, `mikebom:confidence`)

The two signals' recipes follow the same shape; the trust-trio composing recipe uses all three keys.

**CDX 1.6 (evidence-kind filter):**

```jq
.components[]
| select(.properties[]?
        | .name == "mikebom:evidence-kind" and .value == "direct-observation")
| .purl
```

**SPDX 2.3 (evidence-kind filter via envelope):**

```jq
.packages[]
| select(.annotations[]?
        | .comment | fromjson?
        | select(.field == "mikebom:evidence-kind" and .value == "direct-observation"))
| .name + " " + .versionInfo
```

**SPDX 3.0.1 (evidence-kind filter via `@graph[]` Annotation walk):**

```jq
.["@graph"][]
| select(.type == "Annotation"
        and .statement.field == "mikebom:evidence-kind"
        and .statement.value == "direct-observation")
| .subject
```

**CDX 1.6 — trust-trio composing filter (the workflow that drove this milestone):**

```jq
.components[]
| {
    purl,
    source_type:   (.properties[]? | select(.name == "mikebom:source-type")   | .value),
    evidence_kind: (.properties[]? | select(.name == "mikebom:evidence-kind") | .value),
    confidence:    (.properties[]? | select(.name == "mikebom:confidence")    | .value)
  }
| select(.source_type == "trace-observed"
        and .evidence_kind == "direct-observation"
        and .confidence == "heuristic" or .confidence == null
                                       or .confidence == "high")
```

### R3.2 — Binary linkage (`mikebom:linkage-kind`, `mikebom:not-linked`)

**CDX 1.6 (linkage-kind = static filter):**

```jq
.components[]
| select(.properties[]? | .name == "mikebom:linkage-kind" and .value == "static")
| .name
```

**CDX 1.6 (not-linked suppression — find Go components mikebom proved are NOT in the binary):**

```jq
.components[]
| select(.properties[]? | .name == "mikebom:not-linked" and .value == "true")
| .purl
```

(SPDX 2.3 + SPDX 3 recipes follow the same envelope / `@graph[]` patterns from R3.1.)

### R3.3 — Unresolved deps (paired `mikebom:depends-unresolved` + `…-rdepends-unresolved`)

**CDX 1.6 (list every component with unresolved declared deps):**

```jq
.components[]
| select(.properties[]?
        | .name == "mikebom:depends-unresolved"
        and (.value | fromjson | length) > 0)
| {purl, unresolved: (.properties[] | select(.name == "mikebom:depends-unresolved") | .value | fromjson)}
```

### R3.4 — `mikebom:assertion-conflict`

**CDX 1.6 (find every component where the operator's supplement overrode scanner-derived metadata):**

```jq
.components[]
| select(.properties[]? | .name == "mikebom:assertion-conflict")
| {
    purl,
    conflicts: (.properties[]
                | select(.name == "mikebom:assertion-conflict")
                | .value | fromjson)
  }
| .conflicts[]
| select(.winner == "supplement")
| {purl: .purl, field, justification}
```

**Decision**: Ship the per-recipe canonical form above. Each depth section gets 1–3 jq recipes per format (CDX always; SPDX 2.3 + SPDX 3 when the shape differs meaningfully or when consumer demand warrants — per milestone-150 precedent, the doc doesn't repeat trivially-identical recipes across formats but DOES note the equivalence).

**Alternatives considered**:
- One mega-recipe per signal that handles all three formats via `if .components then … elif .packages then … else …`: rejected — defeats the educational purpose of showing per-format placement clearly.
- Skip jq recipes and link to the catalog row: rejected — violates the milestone-150 rendering invariant (recipes are part of every depth section).

## R4 — Cluster placement (FR-001 through FR-006 + SC-005)

Decision matrix already implicit in the spec; restated here for completeness.

| Signal | Target cluster | Cluster signal count BEFORE | AFTER |
|--------|----------------|:---------------------------:|:-----:|
| `mikebom:evidence-kind` | §3.3 build provenance | 3 | 4 |
| `mikebom:confidence` | §3.3 build provenance | 4 | 5 |
| `mikebom:linkage-kind` | §3.1 vulnerability scanning | 4 | 5 |
| `mikebom:not-linked` | §3.1 vulnerability scanning | 5 | 6 |
| `mikebom:depends-unresolved` + `…-rdepends-unresolved` | §3.4 transparency / completeness | 3 | 4 |
| `mikebom:assertion-conflict` | §3.4 transparency / completeness | 4 | 5 |

Final cluster sizes: §3.1 = 6, §3.2 = 3, §3.3 = 5, §3.4 = 5. SC-005 (cluster balance ≥3 each) satisfied; §3.2 stays at 3 (no compliance-cluster signals in this milestone's 6).

**Decision**: 6 new signals placed in 3 of the 4 existing clusters; no new clusters created (per Assumption 2). The compliance cluster (§3.2) stays at 3 — none of the 6 signals fit there better than they fit their assigned cluster.

## R5 — Rubric section placement in the doc

**Decision**: Insert the rubric as **a new subsection inside the existing §2 ("How to read this doc")**, titled **"§2.1 — Curation rubric"**. Keep §2's existing opening paragraphs as introductory context; the rubric goes immediately after them as the closing subsection of §2.

**Rationale**: §2 is the "meta" section that explains how the doc itself is organized. The rubric belongs there because it's a meta-statement about why this doc covers some signals in depth and not others. Placing it earlier than §3 (the cluster sections) means the maintainer / consumer encounters the rubric BEFORE seeing the depth-covered signals, so they can apply the rubric mentally as they read.

**Alternatives considered**:
- Brand-new top-level section (§2.5 between §2 and §3): rejected — over-elevates the rubric relative to its meta-doc role; consumers care about the signals more than the curation logic.
- Place in §7 (For tool authors): rejected — the rubric is for documentation maintainers, not tool authors building on the SBOMs.
- Place at the end of the doc as an appendix subsection (Appendix A.0): rejected — appendices are discoverable by search, not by linear read; the maintainer who needs the rubric will be reading top-down.

## R6 — Appendix A cross-reference updates (FR-014)

Each of the 6 newly-depth-covered signals MUST have its Appendix A entry updated to cross-reference the new depth section instead of (or in addition to) the catalog C-row pointer.

**Decision**: Use the same cross-reference shape milestone 150 already uses for the existing 12 depth-covered signals — append `(see §3.X for depth coverage)` to the appendix entry's one-line description, with the C-row link preserved as the secondary fallback pointer. Example:

```text
| `mikebom:evidence-kind` | How mikebom derived this component (direct observation, inference, enrichment). (see §3.3 for depth coverage). [C4](sbom-format-mapping.md) |
```

This mirrors the existing milestone-150 entries verbatim so the appendix stays consistent.

## R7 — Appendix B (milestone-citation map) updates (FR-015)

Each of the 6 newly-depth-covered signals MUST be listed in Appendix B with its originating milestone. Inferred from catalog rows + git blame:

| Signal | Originating milestone | Catalog row |
|--------|-----------------------|-------------|
| `mikebom:evidence-kind` | 002-era (foundational discovery / enrichment infrastructure) | C4 |
| `mikebom:confidence` | 002-era (foundational; gained quantitative numeric variant in 110) | C16 |
| `mikebom:linkage-kind` | 005-era (binary tier readers landed; closed enum stabilized by milestone 104 binary-role classification) | C12 |
| `mikebom:not-linked` | 050 (Go binary-vs-source comparison G3 redesign) | C41 |
| `mikebom:depends-unresolved` + `…-rdepends-unresolved` | 128 (Yocto recipe enrichment) | C77 + C78 |
| `mikebom:assertion-conflict` | 119 (supplement-CDX merge) | C67 |

**Decision**: Add a new entry per signal under the existing Appendix B structure, preserving the chronological ordering by milestone number (002-era signals first, then 005-era, then 050, then 119, then 128).

## R8 — `verify-recipes.sh` extension shape (FR-012)

**Decision**: Create `specs/151-expand-consumer-guide/verify-recipes.sh` as a near-verbatim copy of milestone 150's `specs/150-sbom-consumer-guide/verify-recipes.sh`, with the recipe list replaced by the 6 new signals' canonical recipes (the R3 set). Reuse the same `run_recipe()` helper, the same `MIKEBOM_FIXTURES_DIR` lookup, the same `mktemp -d` scratch pattern, the same `cargo +stable build --release -p mikebom` pre-step.

**Recipe count**: ~10-12 recipes total in the new harness (6 signals × ~2 recipe per signal — 1 CDX always, 1 SPDX 2.3 or SPDX 3 where the shape differs meaningfully). Final count tracked during authoring per FR-012.

**Fixtures used**:
- Trust-trio recipes: Any fixture with `mikebom:source-type` annotations (cargo `transitive_parity/cargo`, npm `transitive_parity/npm`).
- Linkage recipes: A binary-bearing fixture — milestone 050 / 096-era; need to identify one in the milestone-090 sibling fixture repo.
- `mikebom:not-linked` recipe: a Go fixture with `go.sum` AND a built binary (milestone 050 baseline regression test fixture).
- Unresolved-deps recipes: a Yocto fixture (milestone 128 reader exercised). If no fixture is available in the sibling repo, the recipe is documented but the harness skips with a note (per milestone-150 precedent where `lifecycle-scope` recipes "present" rather than "nonempty" when fixtures lack the signal).
- `mikebom:assertion-conflict` recipe: needs a fixture with a supplement file (milestone 119). Same skip-with-note posture if no fixture available.

**Alternatives considered**:
- Extend milestone 150's harness in place rather than create a new one: rejected — each milestone's harness should be self-contained per the milestone-150 precedent so reviewers can re-run the milestone's claims in isolation.

## R9 — US5 (appendix-hygiene) audit scope

**Decision**: Per FR-010, audit Appendix A for internal-only keys via the following grep-based detection at authoring time:

```bash
# Extract every mikebom: key in the current appendix:
grep -oE "mikebom:[a-z0-9-]+" docs/reference/reading-a-mikebom-sbom.md \
  | sort -u > /tmp/appendix-keys.txt

# Extract every mikebom: key that appears in CURRENT emission paths
# (properties.push / annotations.push / build_annotation_envelope sites
# in the cyclonedx/spdx/spdx_3 builders + their callers):
grep -rE "\"mikebom:[a-z0-9-]+\"" mikebom-cli/src/generate/ \
  mikebom-cli/src/scan_fs/ \
  | grep -oE "mikebom:[a-z0-9-]+" \
  | sort -u > /tmp/emitted-keys.txt

# Diff: appendix keys that have NO emission site = candidates for removal.
comm -23 /tmp/appendix-keys.txt /tmp/emitted-keys.txt
```

Expected result at authoring time: any key with zero emission sites is a removal candidate. The maintainer flagged `mikebom:component-role` (catalog C40) as one such candidate — that key is internal-only and stripped before emission. Audit determines whether any others share that fate.

**Decision on `mikebom:component-role`**: Verify at authoring time whether it's actually internal-only by grepping for its emission sites. If the audit confirms it never reaches the wire output, remove it from Appendix A with a note in the PR description; preserve the catalog C40 row for internal-pipeline-doc completeness.

**Audit outcome documentation**: Write the diff into the PR description as a 3-column list (Key | Has emission site? | Action). Mirrors the milestone-150 SC-002 audit pattern.

## R10 — Trust-trio framing decision

**Decision**: Promote `mikebom:source-type`'s existing §3.3 subsection to a "Trust trio (`source-type` + `evidence-kind` + `confidence`)" framing **without renaming the §3.3 cluster header**. Add `mikebom:evidence-kind` and `mikebom:confidence` as two new sibling subsections; add a short opening paragraph at the top of §3.3 explaining the trio composition; cross-reference each trio member to the other two.

**Rationale**: The "trust trio" framing is a natural composing pattern but doesn't warrant a new cluster (the three signals are already in §3.3 — build provenance — for the right reason). A short intro paragraph + cross-references gives consumers the "compose these three together" mental model without disturbing the cluster structure.

**Alternatives considered**:
- Rename §3.3 to "Build provenance + trust trio": rejected — too verbose, breaks parallelism with the other three cluster titles.
- Create a new §3.5 "Trust assessment" cluster: rejected — violates Assumption 2 (no new clusters).
- Add an inline "trust trio" callout box: rejected — Markdown's GitHub flavor doesn't render callout boxes consistently across the milestone-150 doc's targets (GitHub + mdBook); plain prose intro is portable.
