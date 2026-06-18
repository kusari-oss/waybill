# Phase 0: Research — Smarter root component selection

This research note resolves the open implementation questions left over from `/speckit-clarify`. Two are forward-looking (perf budget on otel-collector scale, the next free C-row in the catalog); the others are existing-code-path patterns we'll reuse.

## R1 — Native-field audit for `mikebom:root-selection-heuristic` (Principle V)

**Decision**: New `mikebom:root-selection-heuristic` document-scope annotation. JSON object `{"heuristic": <name>, "confidence": <float>}`. Emitted only when a tiebreaker beyond the existing count==1 fast path fires.

**Rationale**: Constitution Principle V (post v1.4.0) requires every new `mikebom:*` annotation to first audit each target format for a native construct. Audit results:

| Format | Native field surveyed | Verdict |
|---|---|---|
| CycloneDX 1.6 | `metadata.component` carries the BOM-subject identity. `evidence.identity` exists at component scope and carries `{confidence, technique}` per [CDX 1.6 schema](https://cyclonedx.org/docs/1.6/json/#metadata_component_evidence_identity). | **Component-scoped only.** No native field describes WHICH selection method elected the document-scope subject. |
| SPDX 2.3 | `documentDescribes` is a flat list of `SPDXID` refs; the [SPDX 2.3 spec §5.16](https://spdx.github.io/spdx-spec/v2.3/document-creation-information/#516-document-describes) does not describe how the implementation chose them. | **No native equivalent.** |
| SPDX 3.0.1 | `rootElement` is a single ref ([SPDX 3.0.1 §6.5.2 Element.rootElement](https://spdx.github.io/spdx-spec/v3.0.1/model/Core/Properties/rootElement)). Selection method is implementation-private. | **No native equivalent.** |

Conclusion: no native construct in any of the three formats. The annotation is justified under Principle V's parity-bridging clause. The audit narrative goes into `docs/reference/sbom-format-mapping.md`'s new C-row entry.

**Alternatives considered**:

- Reuse the CDX component-scoped `evidence.identity` shape at the document level. Rejected: would invent a non-conforming field placement (CDX schema doesn't allow document-level `evidence`).
- Add a CDX `properties[]` entry at metadata-scope plus an SPDX 2.3 `creationInfo.comment` blob plus an SPDX 3 element-level annotation — three different shapes. Rejected: parity catalog (milestone 071) covers exactly this case with one canonical key + per-format projection rules. Reuse the existing pattern.

## R2 — Ecosystem-priority order: `[golang, cargo, maven, npm, pip, gem, generic]`

**Decision**: Fixed at compile time as a `const ECOSYSTEM_PRIORITY: &[EcosystemPriority]`. Not surfaced as a CLI flag or env var in this milestone.

**Rationale**:

1. Operators reporting wrong-root bugs (Mario, both #366 and #367) are Go-primary projects. Putting Go first matches empirical evidence.
2. Cargo, npm, pip, gem are next because they're real ecosystems with main-module readers and `pkg:<type>/…` PURLs.
3. Maven sits after the modern-ecosystem package managers because the most common Maven-with-Go polyglot shape (argo-workflows: Java test client embedded in a Go project) is precisely the case where Maven should LOSE. Pure-Java projects are unaffected — no Go main-module exists, so the ladder falls past the ecosystem-priority branch into the Maven `scan_target_coord` branch as today.
4. Generic last as a final tiebreaker for hypothetical future fallback paths.

**Alternatives considered**:

- Operator-configurable order via `--ecosystem-priority golang,cargo,...`. Rejected for v1 — adds a flag surface without a strong "I need this NOW" operator ask. Tracked as a follow-up if the demand materializes.
- Order driven by which reader emitted the LARGEST number of components in the scan. Rejected: violates the "deterministic, the same on every machine for every operator" property in US2 AC#3.
- No priority: when multiple `is_workspace_root` main-modules exist, emit a `pkg:generic/<--path-basename>@0.0.0` placeholder with the warning. Rejected: this is the bug we're trying to fix; we want a real ecosystem PURL not the placeholder.

## R3 — `is_workspace_root` detection: equality vs ancestor

**Decision**: A main-module's `is_workspace_root == true` iff `canonicalize(manifest_file.parent()) == canonicalize(--path)`. Strict directory equality, NOT "manifest is at OR under `--path`".

**Rationale**: Every main-module-tagged component is already inside the `--path` walked tree (by construction of `safe_walk`). What distinguishes the "the root" main-module from "a nested one" is whether its manifest sits at the scan root. Using strict equality is unambiguous and matches the milestone-053 single-main-module fast path's implicit semantics (when one Go module's go.mod is at the scan root, count==1 always fires AND that go.mod is at the scan root — so the fast path picks the same component the new heuristic would pick).

**Alternatives considered**:

- Use `--path` as a prefix match (manifest at `--path` OR any descendant). Rejected: every nested main-module would test true, defeating the purpose.
- Use git's `.git`-discovered repo root. Rejected per Q1 / FR-005 / Assumptions — operators frequently scan sub-trees of monorepos where git root would be wrong.

## R4 — Path canonicalization for symlink dedup (FR-010)

**Decision**: Use `std::fs::canonicalize(manifest_path)` exactly once per main-module at the start of selection. Store results in a `BTreeMap<PathBuf, &ResolvedComponent>` to dedupe by canonical key.

**Rationale**: The existing `scan_fs/walk.rs` already canonicalizes for its visited-set; this is the same pattern. `std::fs::canonicalize` returns an `io::Result` — we treat I/O errors here as "leave the path uncanonicalized" (the symlinked-submodule dedup is best-effort; failure means we may double-count but never miscount). This matches `safe_walk`'s permissive posture.

**Alternatives considered**:

- `path_clean::clean` (pure-string normalization, no I/O). Rejected: doesn't follow symlinks, so the FR-010 case isn't covered.
- Skip canonicalization entirely. Rejected: explicit FR-010 requirement.

## R5 — Performance budget on otel-collector scale (55 main-modules)

**Decision**: Target ≤1 ms overhead at the metadata.component selection step. Verify via the milestone-094 perf benchmark by adding the otel-collector fixture to the benchmark target list.

**Rationale**: Realistic ceiling on main-module count is ~100 (otel-collector's 55 is the largest known case). For each: one `canonicalize` syscall (~10 µs on a warm-cache local filesystem) → ~1 ms total. LCP computation: O(n·m) where n=55 modules, m=~120 chars → ~7K char-compares ≈ 0.1 ms. Total well under the 1 ms budget.

**Alternatives considered**:

- Cache canonicalization results across scans. Rejected for v1: scans are typically one-shot CLI invocations; a cross-scan cache adds complexity without a clear benefit on the dominant use case. The milestone-090 fixture cache covers test-time amortization.
- Defer canonicalization to scan-end. Rejected: data flow is cleaner if the `is_workspace_root` bool is set at main-module emission time, alongside the other ecosystem-specific annotation work.

## R6 — Maven reader main-module + `scan_target_coord` dedup (FR-012)

**Decision**: The Maven reader (`mikebom-cli/src/scan_fs/package_db/maven.rs`) will check, at the point of emitting the main-module `PackageDbEntry`, whether the JAR walker's `scan_target_coord` is set AND matches the same `pkg:maven/<group>/<artifact>@<version>` PURL. When both match, suppress the `scan_target_coord` synthesis by clearing the `scan_target_coord` field on the ScanResult before it propagates to the metadata.component builder.

**Rationale**: The clean dedup point is "before the metadata.component ladder runs," which makes `scan_fs::scan_path` (currently in `mikebom-cli/src/scan_fs/mod.rs`) the right hook. The Maven reader knows its own main-module's coord; it can compare against `scan_target_coord` once and short-circuit. No new control flow elsewhere.

**Alternatives considered**:

- Dedupe inside `generate/cyclonedx/metadata.rs:269-309` (downstream). Rejected: keeps the duplicate signal alive longer than necessary; the `scan_target_coord` field is consumed by both metadata.rs AND spdx/document.rs AND spdx/v3_document.rs — three places where the dedup check would have to fire. Doing it once at signal generation is simpler.
- Always-clear `scan_target_coord` when any Maven main-module exists. Rejected: too aggressive — if the Maven main-module's coord doesn't match `scan_target_coord` (e.g., shaded fat-jar with a different `<finalName>`), the two ARE genuinely different facts and both should reach the ladder.

## R7 — Next free C-row in the catalog

**Decision**: Defer to PR review — read `mikebom-cli/src/parity/extractors/catalog.rs` (or equivalent) at PR time, pick the next free integer, and use it. The recent CHANGELOG entries reference C64 (produces-binaries), C65 (declared source-tier), C66 (supplement-cdx), C67 (assertion-conflict), C68 (kmp-source-set). Working assumption: **C69** is free; verify before merging.

**Rationale**: Catalog row assignment is a one-line code change that conflicts trivially when multiple in-flight specs assign the same integer; better resolved at merge time than spec time.

## R8 — Existing similar pattern reference: milestone 077 `RootComponentOverride`

**Decision**: Model the heuristic-ladder source-of-truth file (`generate/root_selector.rs`) on milestone 077's `RootComponentOverride` pattern in `generate/mod.rs:193-245`. That code already:

- Lives at the right level (`generate/`), called from all three format emitters
- Has the right shape (a small struct + a `build_subject_purl` helper that all emitters call into identically)
- Was tested end-to-end via `mikebom-cli/tests/identifiers_root_purl_control.rs`

**Rationale**: The new selector wraps the existing `RootComponentOverride` priority (override always wins) — so `root_selector.rs` becomes a thin layer above `RootComponentOverride::build_subject_purl`, adding the new tiebreaker branches before falling into the existing fast path. Familiar to reviewers; minimal architectural surface.

## Open questions

None. All `/speckit-clarify` answers resolved.
