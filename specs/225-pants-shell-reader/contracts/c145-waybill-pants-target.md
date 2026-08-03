# Contract: New parity-catalog row C145 `waybill:pants-target`

**Row ID**: C145
**Annotation key**: `waybill:pants-target`
**Row disposition**: **KEEP-NO-NATIVE** (per Principle V bullet 5 audit)
**Directionality**: `SymmetricEqual`
**Order-sensitive**: `false`
**Value regex**: `^[^,\s]+(:[^,\s]+)?(,[^,\s]+(:[^,\s]+)?)*$` — one or
more Pants target addresses (`<dir>:<name>` or bare `<name>` for
root-BUILD-file targets), comma-separated when multiple, lexically
sorted when multiple. No whitespace inside address components.

---

## Semantic definition

Per-component `properties[]` / annotation carrying the Pants target
address(es) that own the component. For file-tier components
emitted by the milestone-225 `pants_shell` reader (script files),
the value is the target address of the `shell_source` /
`shell_sources` / `shunit2_test` / `shunit2_tests` declaration in
whose `source=` / `sources=[...]` expression the file resolved.

**Single-owner example** (common case):

- `scripts/BUILD` declares `shell_source(name="deploy", source="deploy.sh")`.
- Emitted component for `scripts/deploy.sh` carries
  `waybill:pants-target = "scripts:deploy"`.

**Multi-owner example** (rare — same file resolved by two targets):

- `scripts/BUILD` declares both
  `shell_source(name="single", source="waybill-fixture-x.sh")` AND
  `shell_sources(name="glob", sources=["waybill-fixture-*.sh"])`.
- Emitted component for `scripts/waybill-fixture-x.sh` carries
  `waybill:pants-target = "scripts:glob,scripts:single"` (lexical
  sort).

**Cross-BUILD-file multi-owner example** (rarer):

- `scripts/BUILD` has `shell_source(name="a", source="x.sh")`.
- `scripts/BUILD` ALSO has `shell_sources(name="b", sources=["**/*.sh"])`
  which recursively globs the same `x.sh`.
- Both target addresses appear in the annotation:
  `waybill:pants-target = "scripts:a,scripts:b"`.

---

## Companion to C143 `waybill:pants-resolve`

C143 (m223-shipped) carries the Pants **resolve name** for
lockfile-tier components (e.g., `"default"`, `"mypy"`). C145
carries the Pants **target address** for BUILD-file-tier
components (e.g., `"scripts:deploy"`).

The two rows are **orthogonal**: a component can carry both when
provenance chains cross tiers (e.g., a future Pants Go BUILD-file
walker discovers a Go module declared by `go_source(name="deploy",
source="main.go")` from a specific resolve). No component
currently emits both — the milestone-225 shell reader emits only
C145 because shell scripts have no resolve concept.

The two rows also **must not** be conflated: some SBOM consumers
group components by resolve for CVE-scan scoping; grouping by
target address is a different lens (per-service-boundary review,
CODEOWNERS mapping, dep-freshness dashboards).

---

## Format carriers

### CDX 1.6

Per-component `properties[]` entry:

```json
{
  "name": "waybill:pants-target",
  "value": "scripts:deploy"
}
```

Multi-owner:

```json
{
  "name": "waybill:pants-target",
  "value": "scripts:glob,scripts:single"
}
```

### SPDX 2.3 JSON

Per-Package `annotations[]` entry with `MikebomAnnotationCommentV1`
envelope in `comment`:

```json
{
  "annotator": "Tool: mikebom-...",
  "annotationDate": "...",
  "annotationType": "OTHER",
  "comment": "{\"schema\":\"waybill-annotation/v1\",\"field\":\"waybill:pants-target\",\"value\":\"scripts:deploy\"}"
}
```

### SPDX 3.0.1 JSON-LD

Per-Package `Annotation.statement` element with the same
`MikebomAnnotationCommentV1` envelope shape as SPDX 2.3.

---

## Rejected native-carrier alternatives (Principle V bullet 5 audit)

- **CDX `evidence.identity[].technique`**: carries the parse-time
  technique waybill used to identify a component (e.g.,
  `manifest-analysis`, `binary-signature`) — this is a
  per-parse-technique enum, NOT a build-system target address.
  Wrong shape.
- **CDX `evidence.callstack[]`**: describes runtime call flow, not
  build-time ownership. Wrong domain.
- **CDX `pedigree.commits[]` / `pedigree.patches[]`**: describe
  upstream lineage of a component, not the local build-system
  declaration that owns it. Wrong shape.
- **SPDX 2.3 `Package.builtDate` / `Package.releaseDate`**:
  timestamp fields, not identifier fields. Wrong type.
- **SPDX 3 `software_Package.additionalPurpose`**: a per-component
  role enum (`application` / `library` / `data`, etc.) — not a
  build-system target address. Wrong shape.
- **SPDX 3 `SoftwareArtifact.attributionText`**: free-text license
  attribution field. Wrong semantic.

**Conclusion**: no CDX / SPDX 2.3 / SPDX 3 native carrier fits
"which build-system target declared this component". The waybill
annotation is the canonical carrier across all three formats. This
matches the C143 disposition exactly (that row rejected the same
alternatives for the same reason).

---

## Extractor contract

Three functions ship alongside the row (`c145_cdx`, `c145_spdx23`,
`c145_spdx3`) with the same signature as every other component-scope
extractor in `waybill-cli/src/parity/extractors/`:

```rust
// cdx.rs
cdx_anno!(c145_cdx,     "waybill:pants-target", component);
// spdx2.rs
spdx23_anno!(c145_spdx23, "waybill:pants-target", component);
// spdx3.rs
spdx3_anno!(c145_spdx3,   "waybill:pants-target", component);
```

Same macro pattern as C143 (see
`waybill-cli/src/parity/extractors/cdx.rs:859` and siblings for
reference).

**Registration** in `parity/extractors/mod.rs::EXTRACTORS`:

```rust
// Milestone 225 US1 (feature 225-pants-shell-reader): C145
// per-component pants-target address (owning build-system target).
ParityExtractor {
    row_id: "C145",
    label: "waybill:pants-target",
    cdx: c145_cdx,
    spdx23: c145_spdx23,
    spdx3: c145_spdx3,
    directional: Directionality::SymmetricEqual,
    order_sensitive: false,
},
```

Per memory `feedback_sbom_format_mapping_extractor_gate`: adding
the C145 row to `docs/reference/sbom-format-mapping.md` WITHOUT
this extractor registration would fail
`parity::extractors::tests::every_catalog_row_has_an_extractor`.
Both changes must land together in the same commit.

---

## Documentation contract

The row's `docs/reference/sbom-format-mapping.md` entry follows
the same 5-column template as C143 / C144 (see
`docs/reference/sbom-format-mapping.md:180-184` for the sibling
rows):

| Column | Content |
|--------|---------|
| Row ID | `C145` |
| Annotation key | `waybill:pants-target` |
| Semantic description | (per the section above; ~150 words) |
| CDX 1.6 carrier | Per-component `properties[]` entry example |
| SPDX 2.3 carrier | Per-Package `annotations[]` envelope example |
| SPDX 3 carrier | Per-Package `Annotation` element example |
| KEEP-* disposition + rationale | `KEEP-NO-NATIVE` per the Principle V audit above |

---

## Non-goals for C145 v1

- **Cross-tier composition with C143**: no waybill reader today
  emits both C143 and C145 on the same component. When a future
  Pants Go BUILD walker (hypothetical m226) does, that feature's
  spec covers the composition contract; C145 imposes no
  restriction on co-existence.
- **Address canonicalization beyond lex sort of multi-owner
  values**: waybill preserves the address format Pants itself
  uses (`<dir>:<name>` with `:` separator). No canonicalization
  to `//`-prefixed Bazel-style addresses; no rewriting of
  path separators on Windows (Pants uses `/` internally on all
  platforms).
