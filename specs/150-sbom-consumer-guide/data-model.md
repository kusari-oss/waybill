# Data Model — milestone 150

Phase 1 output. The "entities" here are documentation-structure entities — sections and per-signal rendering shape — rather than runtime data structures. No code changes; no Rust types introduced.

## Doc-structure entities

### 1. Top-level section TOC

The new doc at `docs/reference/reading-a-mikebom-sbom.md` MUST contain the following sections in this order:

| # | Section title | Purpose | Spec FR refs |
|---|---|---|---|
| 1 | Opening positioning | "mikebom strict-conforms to CDX/SPDX; `mikebom:*` annotations are parity-bridges per Constitution V; here's what they mean" | FR-002 |
| 2 | How to read this doc | Reader-facing map of clusters + appendix + cross-refs | — |
| 3 | Signals mikebom makes available — by use case | 4 thematic clusters with 12 depth-covered signals total | FR-003 + FR-004 + SC-005 |
| 3.1 | Vulnerability scanning | 3 signals: `mikebom:lifecycle-scope`, `mikebom:layer-digest`, `mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected` | research §C cluster 1 |
| 3.2 | Compliance auditing | 3 signals: `mikebom:license-concluded-source`, `mikebom:component-tier`, `mikebom:demoted-from-main-module` | research §C cluster 2 |
| 3.3 | Build provenance | 3 signals: `mikebom:source-type`, `mikebom:generation-context`, `mikebom:source-document-binding` | research §C cluster 3 |
| 3.4 | Transparency / completeness gaps | 3 signals: `mikebom:file-inventory-mode`, `mikebom:graph-completeness` + `mikebom:graph-completeness-reason`, `mikebom:peer-edge-targets` | research §C cluster 4 |
| 4 | The `mikebom-annotation/v1` envelope | Envelope shape (3 fields) + per-format examples (CDX `properties[]` value, SPDX 2.3 annotation `comment`, SPDX 3 annotation `statement`) + canonical-source links | FR-005 |
| 5 | Cross-format reading patterns | Same signal, where to look in each format (point to `sbom-format-mapping.md` for full wire-shape) | research §D |
| 6 | Stability | Catalog C-row numbers are durable identifiers; envelope shape is stable; opt-in / experimental flags called out | FR-012 |
| 7 | For tool authors | Tool-author-specific summary: envelope schema + carrier-mapping table + stability statement | US2 |
| 8 | Cross-references | List of related docs (5 topical refs + catalog + changelog) | FR-007 + FR-008 |
| Appendix A | Annotation key index | All 102 `mikebom:*` keys at milestone-150 ship time, alphabetical, with one-line descriptions + links to catalog C-rows | FR-006 + SC-002 |
| Appendix B | Milestone-citation map | Each depth-covered signal cites the milestone that introduced or stabilized it | FR-013 + Edge Case 6 |

### 2. Per-signal rendering invariant

Each of the 12 depth-covered signals (sections 3.1–3.4) MUST follow this consistent rendering shape:

```text
### `<annotation-key>`

> **What it is**: <plain-language description, 1-2 sentences>
> **Where it lives**:
> - CDX 1.6: <wire-shape — e.g., `component.properties[]` entry / `metadata.properties[]` / `metadata.lifecycles[]` ...>
> - SPDX 2.3: <wire-shape — typically envelope annotation; or native field name>
> - SPDX 3: <wire-shape>
> **What to do with it**: <action-oriented consumer guidance, 1-3 sentences>
> **Milestone**: <N — added/stabilized/extended>
> **Catalog**: [<C-row>](sbom-format-mapping.md#<anchor>)

```jq
<verified-runnable jq recipe>
```

Expected output:
```
<the actual output the recipe produces against a real fixture>
```
```

Every depth-covered signal MUST conform to this shape — same field order, same code-block formatting. Consumer can scan-read with predictable cadence.

### 3. Appendix A entry invariant

Each appendix entry MUST follow:

```text
- **`mikebom:<key>`** — <one-line description> ([C<row>](sbom-format-mapping.md#c<row>-mikebom-<key>))
```

Alphabetically sorted by `<key>`. Anchor format `c<row>-mikebom-<key>` matches GitHub's auto-generated heading anchors for the catalog table rows. Consumer encountering an unknown key in a real SBOM searches the appendix in O(log N) reading time and gets to the catalog row in one click.

### 4. Cross-reference target shape

Each section that touches a topic covered in depth by an existing ref doc MUST include a `> For full depth, see [<doc title>](<doc path>).` pointer. The summary in the new doc MUST be ~1 paragraph (5-8 sentences) — enough to set context, not enough to duplicate.

### 5. `jq` recipe correctness invariant

Per spec FR-011 + SC-004: each recipe MUST:
- Be valid `jq` syntax (parseable by `jq` 1.6+)
- Produce the documented "Expected output" when run against a real mikebom-emitted SBOM with the appropriate flags
- Be verified at doc-authoring time per research §E

## Out of model

- **No new types**: docs-only milestone.
- **No public API changes**: the mikebom binary is unchanged.
- **No new schema files**: per spec Assumption 7, the doc cites existing canonical sources for the envelope schema.
- **No multi-file split**: single deliverable file per spec Assumption 8.
- **No translations or l10n**: per spec Out of Scope §5.
