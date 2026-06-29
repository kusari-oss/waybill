# Data model — milestone 151

This milestone is docs-only — the "data model" here describes the **content structures** the doc edit produces (per-signal rendering invariant, rubric structure, appendix entry shape), not Rust types. Anchored to milestone 150's already-shipped invariants where applicable.

## §1 — Per-signal rendering invariant (reused from milestone 150 data-model §2)

Every depth-covered signal section in the doc MUST conform to this 7-element shape. The 6 new sections added by this milestone are no exception.

| Element | Purpose | Example |
|---------|---------|---------|
| **Heading** | `#### mikebom:<key>` at depth 4 (inside the depth-3 cluster section). | `#### mikebom:evidence-kind` |
| **What it is** | One-sentence consumer-facing summary of the signal's meaning. NOT the catalog row's prose; rewritten for consumer audience. | "How mikebom derived this component's identity: from direct trace observation, from inference (e.g., lockfile parsing), or from external enrichment (e.g., deps.dev lookup)." |
| **Where it lives (per-format)** | Per-format placement: CDX 1.6 / SPDX 2.3 / SPDX 3.0.1. Cited from catalog row C-number. | "CDX 1.6: `properties[]` on each `components[]` entry. SPDX 2.3: `packages[].annotations[]` with the `mikebom-annotation/v1` envelope. SPDX 3.0.1: `@graph[]` Annotation element with `subject` pointing to the Package IRI." |
| **Value space** | The full set of values the consumer should expect. Closed enums spelled out; open enums noted as open. | "Closed enum: `direct-observation` / `inference` / `enrichment`." |
| **What to do with it** | Consumer-actionable guidance: a use case + the policy decision the signal informs. | "Use as a filter in threshold-based vulnerability-scanner policies: alert on `direct-observation` + `confidence ≥ 0.85`; downgrade `inference` and `enrichment` to advisory." |
| **Milestone** | Originating milestone (or milestone range) for the signal. Cited for traceability. | "Milestone 002-era (foundational discovery / enrichment infrastructure)." |
| **Catalog link** | Explicit `[C<N>](sbom-format-mapping.md)` link to the catalog row. | `[C4](sbom-format-mapping.md)` |
| **jq recipe + Expected output** | A working jq snippet against a real mikebom-emitted SBOM, with the expected output shape annotated as a code-block comment or a following sentence. ≥1 per format where shape differs meaningfully; the depth section MAY collapse to a single CDX recipe + a "SPDX 2.3 / SPDX 3 follow the envelope / `@graph[]` patterns from §4" cross-reference per milestone-150 precedent for trivially-identical-across-format recipes. | (See research.md §R3 for canonical recipe shapes per signal.) |

## §2 — Curation rubric structure (FR-007 + FR-008 + FR-009 + Clarifications Q1)

The rubric section in the doc (§2.1 — Curation rubric, per research.md §R5) carries this structure:

| Element | Purpose | Source |
|---------|---------|--------|
| **Threshold N** | The integer count of YES answers above which a signal warrants depth coverage. | research.md §R1: N=3 |
| **5 yes/no criteria** | Each criterion is a yes/no question with: (a) a one-sentence question statement; (b) a one-paragraph elaboration explaining what "yes" means + an exception clause; (c) one or two example signals that exemplify YES and NO answers. | research.md §R1 |
| **Worked-example table** | A 5-column table listing every depth-covered signal's rubric scoring (5 criteria + YES count + verdict). Doubles as the SC-006 validation evidence visible to consumers. | research.md §R1.1 |
| **Counter-example table** | A parallel table listing 3-5 representative appendix-only signals with their scores, demonstrating the rubric's exclusion behavior. | research.md §R1.2 |
| **"How to apply this to a new signal" guidance** | A 3-step instruction block aimed at the future maintainer adding a new `mikebom:*` key: (1) score the signal against each criterion; (2) sum YES count; (3) if ≥ N, add to a cluster + Appendix A; if < N, add to Appendix A only. | spec.md FR-007 |

The rubric MUST be self-contained — a maintainer reading only §2.1 (without scrolling to §3 cluster sections or appendices) should be able to apply it to a hypothetical `mikebom:foo-bar` and produce a depth-vs-appendix verdict in under 5 minutes (SC-001 question 8 + US4 independent test).

## §3 — Trust-trio composing structure (R10 + new §3.3 intro)

The §3.3 cluster section gets a new opening paragraph **before** the first depth-covered signal:

```
**Trust trio composition.** Three signals in this cluster
compose to support threshold-based vulnerability-scanner
policies: `mikebom:source-type` (where the evidence came
from), `mikebom:evidence-kind` (how it was derived), and
`mikebom:confidence` (how strongly mikebom backs the claim).
Consumers building risk-weighting filters MUST consider all
three together, not in isolation.
```

Each trio member's depth section also gets a **"Composes with"** cross-reference line (added after the "What to do with it" element) pointing at the other two:

```
**Composes with**: [`mikebom:source-type`](#mikebom-source-type) (where), [`mikebom:confidence`](#mikebom-confidence) (how strongly).
```

This is a 3rd-element addition specific to the trio; other depth sections may add their own "Composes with" lines where pairing semantics exist (e.g., `graph-completeness` + `graph-completeness-reason`).

## §4 — Paired entry structure (FR-005 + reused from milestone 150's `graph-completeness` precedent)

The `mikebom:depends-unresolved` + `mikebom:rdepends-unresolved` paired entry uses a single depth section with a **dual-heading + dual-recipe** shape, mirroring milestone 150's existing `graph-completeness` + `…-reason` pair:

```
#### mikebom:depends-unresolved + mikebom:rdepends-unresolved

**What it is**: …
**Where it lives (per-format)**: … (covers BOTH keys)
**Value space**: … (notes the per-key differentiation)
**What to do with it**: …
**Milestone**: 128
**Catalog link**: [C77 + C78](sbom-format-mapping.md)
**jq recipe + Expected output**: … (one recipe per key, two total)
**Currently emitted by**: the Yocto recipe reader only (the key namespace is reserved for future cross-ecosystem use per Clarifications Q2).
```

The "Currently emitted by" element is a paired-entry-specific addition (not part of the §1 invariant for solo signals); it implements the reserved-key framing from Clarifications Q2.

## §5 — `verify-recipes.sh` extension structure

The new harness at `specs/151-expand-consumer-guide/verify-recipes.sh` mirrors milestone 150's structure exactly. Per-recipe shape:

```bash
run_recipe "evidence-kind-cdx" "cyclonedx-json" "transitive_parity/cargo" "" \
    '.components[]
     | select(.properties[]?
              | .name == "mikebom:evidence-kind"
                and (.value == "direct-observation" or .value == "inference"))
     | .purl' \
    "present"
```

Arguments (positional, same as milestone 150):

| Arg | Purpose | Example |
|-----|---------|---------|
| 1 | Recipe name (for diagnostic output) | `"evidence-kind-cdx"` |
| 2 | mikebom format flag passed to `--format` | `"cyclonedx-json"` / `"spdx-2.3-json"` / `"spdx-3-json"` |
| 3 | Fixture path relative to `$MIKEBOM_FIXTURES_DIR` | `"transitive_parity/cargo"` |
| 4 | Extra flags passed to `mikebom sbom scan` | `""` or `"--include-dev"` or `"--root-name foo --root-version 1.0"` |
| 5 | The jq recipe string | (multi-line jq expression) |
| 6 | Expectation: `"nonempty"` requires non-null non-empty jq output; `"present"` only requires the jq runs without error (output may be empty if the fixture lacks the signal — used for opt-in or ecosystem-specific signals) | `"present"` or `"nonempty"` |

Final tally line: `Verification summary: $PASS passed, $FAIL failed`; exit 0 on all-pass, 1 on any-fail.

## §6 — Appendix A entry shape (FR-014 + R6)

Each newly-depth-covered signal's Appendix A entry transitions from:

```
| `mikebom:evidence-kind` | How mikebom derived this component (direct observation, inference, enrichment). [C4](sbom-format-mapping.md) |
```

to:

```
| `mikebom:evidence-kind` | How mikebom derived this component (direct observation, inference, enrichment). (see §3.3 for depth coverage). [C4](sbom-format-mapping.md) |
```

This is a minimal-edit appended `(see §3.X for depth coverage).` clause. The catalog C-row link is preserved as fallback per FR-011.

## §7 — Appendix B entry shape (FR-015)

Each newly-depth-covered signal MUST appear in Appendix B with its originating milestone, following the existing Appendix B shape (milestone → signal table, chronological by milestone number per R7).

## §8 — US5 audit output structure (R9)

The PR description for milestone 151 MUST include the audit output as a 3-column table:

| Key | Has emission site? | Action |
|-----|--------------------|--------|
| `mikebom:component-role` | NO (verified internal-only at authoring time) | Remove from Appendix A; preserve catalog C40 |
| `mikebom:<other>` | (per audit outcome) | (per audit outcome) |

The audit grep recipe (research.md §R9) produces this list. The full list lives in the PR description, not in the shipped doc.
