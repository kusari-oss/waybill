# Contract — `reading-a-mikebom-sbom.md` document structure

Phase 1 output. Defines the doc-structure contract that the implementer follows when authoring the new consumer-facing reference doc.

## File location + naming

| Aspect | Value |
|---|---|
| File path | `docs/reference/reading-a-mikebom-sbom.md` |
| Visibility | Public (part of the published `docs/` tree) |
| Format | GitHub-flavored Markdown |
| Estimated size | 600–900 lines |
| Linked from | `docs/index.md` Reference material section (one-line addition) |

## Document outline (mandatory)

The doc MUST contain the following top-level sections in this order. Section count = 8 + 2 appendices = 10 sections total.

```text
# Reading a mikebom SBOM

1. Opening positioning (~30 lines)
2. How to read this doc (~20 lines)
3. Signals mikebom makes available — by use case (~400 lines, 12 depth-covered signals)
   3.1 Vulnerability scanning (3 signals)
   3.2 Compliance auditing (3 signals)
   3.3 Build provenance (3 signals)
   3.4 Transparency / completeness gaps (3 signals)
4. The mikebom-annotation/v1 envelope (~40 lines)
5. Cross-format reading patterns (~60 lines)
6. Stability (~40 lines)
7. For tool authors (~60 lines)
8. Cross-references (~30 lines)
Appendix A. Annotation key index (~200 lines for 102 entries)
Appendix B. Milestone-citation map (~30 lines)
```

## Section content contracts

### Section 1 — Opening positioning (~30 lines)

MUST include:
- Statement that mikebom strict-conforms to CDX 1.6 / SPDX 2.3 / SPDX 3.0.1.
- Statement that most data lives in spec-native fields (not `mikebom:*` annotations).
- Definition of "parity-bridging" — `mikebom:*` annotations are introduced ONLY when no native field in the target format(s) carries the signal, per Constitution Principle V.
- One-sentence framing of the doc's job: "tell consumers what each parity-bridging annotation (and a few notable native-field-usage patterns) mean and how to use them".
- Per the 2026-06-29 clarification (Q1 Option D): the doc focuses on what mikebom emits, NOT on comparing mikebom against named competing tools.

MUST NOT include:
- Names of specific competing SBOM tools (syft / trivy / cdxgen / snyk / anchore). Use phrasing like "the CDX/SPDX spec baseline" or "standard SBOM tool output" when contrast is needed.

### Section 2 — How to read this doc (~20 lines)

MUST include:
- A "navigation map" — readers who want vulnerability-scanning signals jump to §3.1; compliance auditors jump to §3.2; tool authors jump to §7; etc.
- A pointer to Appendix A for the full key index.
- A pointer to `docs/reference/sbom-format-mapping.md` for full wire-shape catalog depth.

### Section 3 — Signals by use case (~400 lines, 12 signals)

4 subsections (3.1–3.4), each covering 3 signals per research §C cluster assignments. Each signal renders per the per-signal invariant in `data-model.md` §2:

- "What it is" — plain-language description (1–2 sentences)
- "Where it lives" — per-format wire-shape (CDX / SPDX 2.3 / SPDX 3)
- "What to do with it" — action-oriented consumer guidance (1–3 sentences)
- "Milestone" — milestone N (added/stabilized/extended)
- "Catalog" — link to the corresponding C-row in `sbom-format-mapping.md`
- A working `jq` recipe + its expected output

12 depth-covered signals MUST appear (per research §C):
- 3.1: `mikebom:lifecycle-scope`, `mikebom:layer-digest`, `mikebom:duplicate-purl-divergent` (+ `mikebom:purl-collisions-detected` mentioned as the document-scope companion)
- 3.2: `mikebom:license-concluded-source`, `mikebom:component-tier` (file value), `mikebom:demoted-from-main-module`
- 3.3: `mikebom:source-type`, `mikebom:generation-context`, `mikebom:source-document-binding`
- 3.4: `mikebom:file-inventory-mode`, `mikebom:graph-completeness` (+ `mikebom:graph-completeness-reason`), `mikebom:peer-edge-targets`

### Section 4 — The mikebom-annotation/v1 envelope (~40 lines)

MUST include:
- Envelope schema (3 fields: `schema`, `field`, `value`) shown inline.
- One example per format showing the envelope embedded:
  - CDX 1.6 `properties[]` entry: `{name: "mikebom:lifecycle-scope", value: "development"}` (envelope NOT used — CDX uses native `properties[].value` string carrier).
  - SPDX 2.3 `annotations[]` entry: `{comment: "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:lifecycle-scope\",\"value\":\"development\"}", ...}`.
  - SPDX 3 `Annotation` element: `{statement: "{\"schema\":\"mikebom-annotation/v1\",\"field\":\"mikebom:lifecycle-scope\",\"value\":\"development\"}", ...}`.
- Pointers to canonical Rust sources:
  - Encoder: `mikebom-cli/src/generate/spdx/annotations.rs:31-67` (ENVELOPE_SCHEMA_V1 constant + MikebomAnnotationCommentV1 struct).
  - Decoder: `mikebom-cli/src/parity/extractors/common.rs:185` (envelope verification + payload extraction).
- Statement that the envelope schema string `mikebom-annotation/v1` is the stability anchor — future envelope evolutions would bump the version (`v2`).

### Section 5 — Cross-format reading patterns (~60 lines)

MUST include:
- Table or list showing the SAME signal in 3 formats for 4–5 representative depth-covered signals, demonstrating the per-format carrier shape.
- Pointer to `sbom-format-mapping.md` as the canonical wire-shape source for the full 98-row catalog.
- Note about SPDX 3 subject-routing quirks where applicable (e.g., `mikebom:demoted-from-main-module` routes to synth-root IRI per milestone-149 C102 docs).

### Section 6 — Stability (~40 lines)

MUST include:
- Every `C*` row in the catalog is a stable wire shape; the row number is the durable identifier.
- The `mikebom-annotation/v1` envelope shape is stable.
- Opt-in / experimental flags affecting emission MUST be called out:
  - `--file-inventory=full` (override marker via `mikebom:file-inventory-mode`)
  - `--preserve-manifest-main-module` (milestone 149)
  - `--include-dev` (affects which `mikebom:lifecycle-scope` values get emitted in `metadata.lifecycles[]`)
  - Other opt-ins as authoring discovers them.
- Versioning: mikebom emissions follow the `v*-alpha.*` tag sequence; consumers can map binary version → signal availability via the milestone citations in Appendix B.

### Section 7 — For tool authors (~60 lines)

MUST include:
- Envelope schema location pointer (cross-ref to §4).
- Per-format carrier-mapping table (cross-ref to §5 + catalog).
- Stability statement (cross-ref to §6).
- Suggested integration patterns:
  - Filter dep-graph by `mikebom:lifecycle-scope` for production-vulnerability suppression.
  - Walk `mikebom:layer-digest` for layer-attribution in OCI scans.
  - Correlate `mikebom:duplicate-purl-divergent` for divergence-detection workflows.
- Pointer to GitHub Issues for bug reports or signal-shape concerns.

### Section 8 — Cross-references (~30 lines)

MUST include links to:
- `docs/reference/sbom-format-mapping.md` (the catalog — canonical wire-shape contract)
- `docs/reference/identifiers.md` (identifier model)
- `docs/reference/sbom-types.md` (CISA SBOM types)
- `docs/reference/component-tiers.md` (package / binary / file tier model)
- `docs/reference/cross-tier-binding.md` (source ↔ build ↔ deploy binding)
- `../CHANGELOG.md` (milestone-by-milestone release history)

### Appendix A — Annotation key index (~200 lines for 102 entries)

Per `data-model.md` §3 invariant. Each entry: `- **\`mikebom:<key>\`** — <one-line description> ([C<row>](sbom-format-mapping.md#<anchor>))`. Alphabetically sorted.

MUST cover all 102 unique `mikebom:*` keys present in `sbom-format-mapping.md` at milestone-150 ship time (per spec FR-006 + SC-002).

### Appendix B — Milestone-citation map (~30 lines)

MUST include a table mapping each depth-covered signal (the 12 from §3) to its introducing/stabilizing milestone number with a brief verb (`added` / `stabilized` / `extended`). Format:

```text
| Signal | Milestone | Verb |
|---|---|---|
| `mikebom:lifecycle-scope` | 052 | added |
| ... | ... | ... |
```

Per spec FR-013 + Edge Case 6.

## Index update contract

`docs/index.md`'s **Reference material** section MUST add the following one-line entry (positioned to fit the existing list's alphabetical / topical ordering):

```markdown
- [Reading a mikebom SBOM](reference/reading-a-mikebom-sbom.md) — consumer-facing
  guide to mikebom-emitted signals (what they mean, where to find them per format,
  how to use them). Cross-references the [SBOM format mapping](reference/sbom-format-mapping.md)
  catalog for full per-row wire-shape detail.
```

## Negative-space contract (what MUST NOT happen)

- The doc MUST NOT name specific competing SBOM tools (syft / trivy / cdxgen / snyk / anchore). Per the 2026-06-29 clarification Q1 Option D.
- The doc MUST NOT introduce new `mikebom:*` annotations. Pure docs milestone.
- The doc MUST NOT change the wire format mikebom emits. Reads from existing catalog only.
- The doc MUST NOT duplicate `sbom-format-mapping.md`'s per-row wire-shape detail at row-precision. Summary + link is the layered-docs approach.
- The doc MUST NOT include marketing language or unverifiable claims about ecosystem positioning.
- The doc MUST NOT include translations or non-English content.
- The doc MUST NOT include implementation-specific Rust code paths (consumer-facing surface only).

## Test surface

| Test | Asserts |
|---|---|
| Operator-cadence read-through audit | SC-001 5-question test — the doc reader can answer all 5 questions without consulting other docs |
| Appendix-coverage audit | SC-002 — every `mikebom:*` key in `sbom-format-mapping.md` is also in Appendix A |
| `docs/index.md` linkback | SC-003 — the new doc is linked from index.md's Reference material section |
| `jq` recipes runnable count | SC-004 — at least 5 recipes verified runnable at doc-authoring time |
| Cluster coverage count | SC-005 — at least 4 distinct thematic clusters, each with at least 2 signals |
| Depth-covered signal count | SC-006 — at least 8 signals depth-covered (12 chosen for headroom) |
| Pre-PR gate | SC-007 — `./scripts/pre-pr.sh` passes (docs-only milestone; gate is essentially a no-op) |
| Catalog reverse-link audit | SC-008 — `sbom-format-mapping.md` is linked from the new doc at least once |
