# Research — milestone 150 (SBOM consumer-facing reading guide)

Phase 0 output. Resolves five doc-structure design questions before Phase 1.

## §A — Envelope schema canonical location

**Decision**: link to TWO existing canonical sources from the new doc; publish NO new JSON Schema artifact in this milestone.

**Verification** (grep at plan-phase time):

- `mikebom-cli/src/generate/spdx/annotations.rs:31` defines `pub const ENVELOPE_SCHEMA_V1: &str = "mikebom-annotation/v1"`.
- `mikebom-cli/src/generate/spdx/annotations.rs:44-67` defines `pub struct MikebomAnnotationCommentV1 { schema, field, value }` + a `new()` constructor + serialization helper.
- `mikebom-cli/src/parity/extractors/common.rs:185` defines the symmetric decoder — verifies `v.get("schema")?.as_str()? != "mikebom-annotation/v1"` and extracts the `field` + `value` payload.

The Rust source IS the canonical schema today. Both Rust files have clear doc-comments explaining the envelope shape. Linking to both lines from the new doc gives consumers a stable canonical reference + lets them inspect the exact (de)serialization round-trip.

**Alternative considered**: publish a JSON Schema artifact at `docs/reference/mikebom-annotation-v1.schema.json` referenced from the new doc. **Rejected** for this milestone scope — would require defining the schema's authoritative location (this milestone is docs-only, no new schema files per spec Assumption 7), reviewing the schema's semantic version + extension policy, and ongoing maintenance of the JSON Schema alongside the Rust struct. A future milestone can promote the schema to a first-class artifact; out of scope here.

**Doc treatment**: in the new doc's "Envelope schema" section, give the shape inline (3 fields with example), then cite both Rust file lines as the canonical references. ~10–15 lines of Markdown.

## §B — Annotation key inventory + appendix-coverage decision

**Decision**: appendix index covers all 102 unique `mikebom:*` keys present in the catalog at milestone-150 ship time. Each appendix entry has fields `{key, one-line-description, link-to-catalog-C-row}`. Sorted alphabetically.

**Verification**: `grep -E "^\| C[0-9]+\b" docs/reference/sbom-format-mapping.md | grep -oE "mikebom:[a-z0-9-]+" | sort -u | wc -l` returns 102 keys.

Representative key set (first 60 alphabetically):
```
mikebom:also-detected-via, mikebom:assembly-version-informational,
mikebom:assembly-version-informational-stripped, mikebom:assertion-conflict,
mikebom:bazel-archive-name, mikebom:bbappend-applied, mikebom:binary-class,
mikebom:binary-packed, mikebom:binary-stripped, mikebom:build-inclusion,
mikebom:build-inclusion-derivation, mikebom:build-reference,
mikebom:buildinfo-status, mikebom:co-owned-by, mikebom:component-role,
mikebom:component-tier, mikebom:confidence, mikebom:cpe-candidates,
mikebom:demoted-from-main-module, mikebom:depends-unresolved,
mikebom:deps-dev-match, mikebom:detected-cargo-auditable, mikebom:detected-go,
mikebom:download-url, mikebom:duplicate-purl-divergent,
mikebom:elf-build-id, mikebom:elf-compiler-stamps, mikebom:elf-debuglink,
mikebom:elf-runpath, mikebom:evidence-kind, mikebom:exclude-path,
mikebom:file-inventory-mode, mikebom:file-inventory-skipped-oversize,
mikebom:file-inventory-skipped-special-files, mikebom:file-inventory-unreadable,
mikebom:file-paths, mikebom:file-paths-truncated, mikebom:fingerprint-confidence,
mikebom:fingerprint-corpus-sha, mikebom:generation-context,
mikebom:go-vcs-modified, mikebom:go-vcs-revision, mikebom:go-vcs-time,
mikebom:graph-completeness, mikebom:graph-completeness-reason,
mikebom:identifiers, mikebom:kmp-source-set, mikebom:layer-digest,
mikebom:license-concluded-source, mikebom:lifecycle-scope,
mikebom:lifecycle-scope-derivation, mikebom:linkage-kind,
mikebom:macho-build-tools, mikebom:macho-build-version,
mikebom:macho-codesign-flags, mikebom:macho-codesign-identifier,
mikebom:macho-codesign-team-id, mikebom:macho-min-os, mikebom:macho-rpath,
mikebom:macho-uuid
```

Remaining 42 keys (alphabetical tail) covered in the appendix per the same shape.

**Decision rationale**: per spec FR-006, the appendix is a flat alphabetical lookup table. Maintaining 102 entries at milestone-150 ship time is tractable manually. Per spec Assumption 4 the appendix is a snapshot — future milestones land new annotations in the catalog only, not in the guide's appendix. The catalog remains the canonical source-of-truth for new signals.

**Alternative considered**: restrict appendix to only the parity-bridging `mikebom:*` annotations (skip Section A native-field rows). **Rejected** — Section A native fields don't have `mikebom:` prefixed keys (they map to spec-native fields like `component.name`, `package.versionInfo`); they wouldn't appear in the appendix anyway. The 102 keys ARE all parity-bridging or transparency-marker annotations.

## §C — Depth-coverage signal selection (curated list)

**Decision**: depth-cover ~12 signals across 4 thematic clusters per FR-003 + SC-005 + SC-006 (≥8 signals minimum, 12 chosen for headroom). The remaining ~90 keys ride the appendix index only.

**Curated depth-coverage list** (organized by spec FR-003's thematic clusters):

### Cluster 1 — Vulnerability scanning (3 signals)

| Signal | Why a vulnerability scanner cares |
|---|---|
| `mikebom:lifecycle-scope` | Suppress dev/test/build-scoped deps from production-only CVE alerting. Native CDX `component.scope: excluded` is the coarse signal; this annotation carries the finer `development` / `build` / `test` split. |
| `mikebom:layer-digest` | OCI image scans — correlate vulnerable component → which layer introduced it (forensics + remediation prioritization). |
| `mikebom:duplicate-purl-divergent` + `mikebom:purl-collisions-detected` | When the same PURL maps to divergent content (Cargo `(name, version)` collision), surface it so the scanner doesn't silently merge into one vulnerability assessment. |

### Cluster 2 — Compliance auditing (3 signals)

| Signal | Why a compliance auditor cares |
|---|---|
| `mikebom:license-concluded-source` | Distinguish operator-asserted license conclusions (`--conclude-licenses`) from external-enrichment-derived ones (ClearlyDefined, deps.dev). Determines whether the conclusion carries a human-review claim or not. |
| `mikebom:component-tier` (when `"file"`) | File-tier components carry no PURL; identify content by SHA-256 + observed paths. Closes the orphan / unattributed-content gap (Constitution VIII Completeness). |
| `mikebom:demoted-from-main-module` (milestone 149) | When `--root-name` operator override is active + `--preserve-manifest-main-module` is set, marks the library entry as the demoted manifest-derived main-module. Auditors verify the operator-override doesn't lose manifest provenance. |

### Cluster 3 — Build provenance (3 signals)

| Signal | Why a build-provenance consumer cares |
|---|---|
| `mikebom:source-type` | Distinguishes `eBPF-traced` vs `lockfile-derived` vs `declared-not-cached` provenance. Trace-observed components have stronger ground truth than enrichment-only ones. |
| `mikebom:generation-context` (document-scope) | Doc-level signal of whether the SBOM was generated from a build trace, source-tree scan, image scan, or hybrid. |
| `mikebom:source-document-binding` (milestone 072) | Cross-tier binding from a build SBOM back to its source SBOM via content hash + IRI. Enables source ↔ build ↔ deploy correlation. |

### Cluster 4 — Transparency / completeness gaps (3 signals)

| Signal | Why a consumer evaluating completeness cares |
|---|---|
| `mikebom:file-inventory-mode` (document-scope) | Marks SBOMs scanned with `--file-inventory=full` so consumers know the file-tier set may duplicate package/binary tier coverage (Strict Boundary §5). Without this flag the SBOM is in default `orphan` mode. |
| `mikebom:graph-completeness` + `mikebom:graph-completeness-reason` (document-scope) | Go dependency graph completeness signal — distinguishes `Complete` vs `Partial` graphs with reason codes for the latter. |
| `mikebom:peer-edge-targets` (milestone 147) | npm peerDependency edges, with the install-vs-functional distinction preserved. Consumers can filter the dep graph by edge kind. |

**12 signals total across 4 clusters.** Each gets full per-format wire-shape + plain-language meaning + `jq` recipe + action-oriented consumer guidance per FR-004.

**Alternatives considered**:
- **Cover 20+ signals in depth** — rejected: bloats the doc + duplicates catalog content. The 12-signal curated set is enough for SC-005 (≥4 clusters) + SC-006 (≥8 signals).
- **Cover only the top 3 signals per cluster (12 signals)** — chosen. Tight focus.
- **Defer per-cluster signal selection to authoring time** — rejected: tasks.md needs to enumerate the targets so an implementer can scope per-signal work.

## §D — Cross-reference targets

**Decision**: 5 existing reference docs get linked from the new doc; each gets a ~1-paragraph summary in the new doc + a "for full depth, see X" pointer.

| Cross-ref target | New doc treatment | Why summary + link, not duplication |
|---|---|---|
| `docs/reference/sbom-format-mapping.md` | Linked from every depth-covered signal + the appendix index entries. THE canonical wire-shape catalog. | The catalog is exhaustive (98+ C-rows) and code-review-grade; duplicating it in the new doc defeats the layered-docs approach. |
| `docs/reference/identifiers.md` | Linked from the `mikebom:identifiers` mention + the cross-tier identity discussion. Summary: "mikebom carries `repo:` / `git:` / `image:` / `attestation:` / user-defined identifiers via spec-native carriers + an annotation envelope". | Identifier model is rich (4-layer); a 1-paragraph summary directs the reader to the dedicated ref doc. |
| `docs/reference/sbom-types.md` | Linked from the CISA SBOM Type discussion. Summary: "mikebom emits CISA SBOM Types via native `metadata.lifecycles[]` (CDX) / `creationInfo.comment` (SPDX 2.3) / `software_Sbom.software_sbomType[]` (SPDX 3) per the `--sbom-type` flag". | Type model has 6 enum values + emission semantics per format; dedicated ref doc covers depth. |
| `docs/reference/component-tiers.md` | Linked from the `mikebom:component-tier` deep-dive + the file-tier orphan-coverage discussion. Summary: "mikebom classifies components into package-tier, binary-tier, and file-tier per the content-shape allowlist; file-tier components fill the unattributed-content gap". | Tier model has FR-011 hybrid dedup + 3 modes + content-shape allowlist; ref doc has the full taxonomy. |
| `docs/reference/cross-tier-binding.md` | Linked from the `mikebom:source-document-binding` discussion + the `--bind-to-source` / `verify-binding` mention. | Binding-hash-v1 algorithm + verify CLI surface lives in the dedicated ref doc. |

**Plus**: link to the CHANGELOG (`../../CHANGELOG.md`) for milestone-by-milestone signal-introduction history, per spec Edge Case 6 + FR-013.

## §E — `jq` recipe verification plan

**Decision**: each `jq` recipe in the new doc MUST be verified at doc-authoring time against a real mikebom-emitted SBOM. Verification approach:

1. Run `mikebom sbom scan` against the existing `tests/fixtures/cargo/lockfile-v3` (or equivalent ecosystem fixture) with appropriate flags to trigger the signal (e.g., `--include-dev` for lifecycle-scope examples, `--root-name X --root-version Y --preserve-manifest-main-module` for the demote-from-main-module example).
2. Pipe through the doc's exact `jq` recipe.
3. Confirm the output matches the doc's claimed output (or update one to match the other).

Per spec FR-011: each recipe MUST be correct as written. Per spec SC-004: at least 5 recipes verified runnable.

**Authoring artifact**: a `verify-recipes.sh` shell script in the spec dir (or appendix to the doc itself) listing the exact scan-then-jq commands for each recipe. NOT shipped to the repo — kept in the spec dir as authoring evidence. Operator can re-run post-merge if doubt arises about a specific recipe.

**Alternative considered**: ship the verification script as part of the doc itself (in a "Verifying recipes" appendix). **Rejected** — bloats the doc; the recipes-are-correct invariant is a doc-authoring quality check, not a consumer-facing surface. The doc text itself includes worked examples; the verification script is implementation detail.

## Summary of decisions feeding Phase 1

- **§A**: Envelope schema cited from two existing Rust locations (`spdx/annotations.rs:31-67` + `parity/extractors/common.rs:185`). No new JSON Schema artifact.
- **§B**: Appendix indexes all 102 unique `mikebom:*` keys at milestone-150 ship time, alphabetical, snapshot.
- **§C**: 12 signals depth-covered across 4 clusters (vulnerability scanning + compliance auditing + build provenance + transparency/completeness gaps).
- **§D**: 5 cross-references to existing topical refs + CHANGELOG link.
- **§E**: `jq` recipe verification via shell script in spec dir (authoring artifact, not shipped).
- **No new Rust code, no new schema files, no wire-format changes.**
