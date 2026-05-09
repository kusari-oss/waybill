# Feature Specification: Go reader go.sum-based transitive fallback (closes #174)

**Feature Branch**: `091-go-sum-transitive-fallback`
**Created**: 2026-05-09
**Status**: Draft
**Input**: User description: "174"

## Background

Surfaced by milestone 083's transitive-parity audit (`mikebom-cli/tests/transitive_parity_go.rs`) against the `kubernetes-sigs/cri-tools @ v1.32.0` fixture. In the offline-and-cache-empty configuration (the typical CI runner state — `--offline` flag set, `$GOMODCACHE` empty), per-tool edge counts diverge:

| Tool | Edges emitted |
|---|---|
| **mikebom (alpha.27)** | **31** — direct deps from `go.mod`'s `require` block only |
| trivy 0.69.3 | 142 — full transitive closure |
| syft 1.27.0 | 0 |

Mikebom's milestone-055 Go-transitive-resolution 4-step ladder degrades to step 4 (no-edges-fallback) when offline + cache-empty: it synthesizes edges from the project's own `go.mod require` block only, capturing direct deps and dropping ~110 transitive deps that trivy successfully extracts from `go.sum` content alone.

`go.sum` is structurally a record of every package version the build system has fetched — a flattened transitive closure of the root module. Each line encodes `<module>@<version> h1:<base64-hash>` for either the module archive or its `.mod` file. Trivy parses this directly and synthesizes edges from the root module to each `(module, version)` pair, capturing the transitive set without needing any per-module `go.mod` files (which would require either `go mod graph`, a populated `$GOMODCACHE`, or proxy fetches — all blocked in the offline-cache-empty state).

This milestone closes the gap by adding a step 5 to the milestone-055 ladder: when steps 1–3 fail (consistent with the offline-cache-empty CI configuration), parse `go.sum` directly and emit (root_module → each_unique_module_version) edges. Step 4 (the existing direct-deps-only fallback) becomes step 6, used only when even `go.sum` is absent.

The trade-off is documented up front: `go.sum` doesn't encode parent-child topology (`mod1 → mod2`), only the flat set of all versions ever fetched. mikebom's step-5 output will therefore be topologically flatter than its milestone-055 cache-populated outputs (root → all transitives, no inter-transitive edges). This matches trivy's behavior and is materially more useful than today's direct-deps-only floor; an annotation on emitted edges (Constitution Principle X) makes the lower-fidelity provenance explicit so consumers can distinguish step-5 edges from step-1/2/3 edges.

## Clarifications

### Session 2026-05-09

- Q: Provenance-annotation granularity — per-edge or per-component? → A: **per-component**. Native in all 3 formats (CDX `Component.evidence.identity[].methods[]`, SPDX 2.3 `package.annotations[]`, SPDX 3 `software_Package.evidence`); Constitution Principle V (standards-native precedence) cleanly satisfied without `mikebom:*` field for the basic case. Per-edge would require inventing custom constructs in CDX 1.6 + SPDX 2.3 (no native per-relationship metadata in those formats — only SPDX 3 has it). The mikebom ladder runs one step per scan (no hybrid step-5+step-2 result), so a transitive component's provenance IS its enclosing edge's provenance — the loss-of-precision concern is theoretical, not practical. FR-002 + US3 acceptance scenarios re-frame as "per-component provenance" accordingly.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator on a CI runner gets a complete transitive Go graph (Priority: P1)

A security-conscious operator running `mikebom sbom scan --path <go-project> --offline` on a fresh CI runner (no populated `$GOMODCACHE`, no network egress allowed) sees an SBOM that contains every component listed in the project's `go.sum` — direct AND transitive — connected via DEPENDS_ON edges to the root module.

**Why this priority**: This is the entire reason for the milestone. The current state (31 edges vs trivy's 142, on the same fixture in the same environment) means CI-generated mikebom SBOMs for Go projects are missing ~78% of transitive components compared to what trivy captures. Operators using mikebom output for vuln-scanning, SLSA-attestation, or compliance checks against transitives are getting false-negatives at the offline-cache-empty path.

**Independent Test**: `cargo +stable test -p mikebom --test transitive_parity_go` post-091. Confirm mikebom's edge count for the cri-tools fixture rises from 31 to ≥130 (the post-dedup go.sum entry count); confirm at least 90% of trivy's 142 edges are now also in mikebom's set.

**Acceptance Scenarios**:

1. **Given** the milestone-083 cri-tools fixture (`go.mod` + `go.sum`, ~30 direct + ~110 transitive deps), **When** the operator runs `mikebom sbom scan --offline --path <fixture>` on a runner with `$GOMODCACHE` unpopulated, **Then** the emitted SBOM contains ≥130 cargo/golang components (direct + transitive) with DEPENDS_ON edges from the root module to each transitive.
2. **Given** the same fixture, **When** the operator runs `mikebom sbom scan --offline --path <fixture> --format spdx-2.3-json`, **Then** the SPDX 2.3 output contains a `DEPENDS_ON` Relationship from the root SPDXID to every `pkg:golang/<module>@<version>` PURL captured from `go.sum`.
3. **Given** mikebom's emitted SBOM contains step-5-derived edges, **When** the operator inspects an arbitrary edge's TARGET COMPONENT, **Then** the component carries a per-component provenance annotation (CDX `evidence.identity[].methods[].technique` set to `go-sum-fallback` or analogous SPDX 2.3 / SPDX 3 native equivalent — exact field names plan-level under Constitution Principle V's "audit native first" rule) distinguishing it from components discovered via the higher-fidelity step-1/2/3 paths.

---

### User Story 2 - Cache-populated path still works at full milestone-055 fidelity (Priority: P1)

A developer scanning the same Go project on their laptop (where `$GOMODCACHE` IS populated with `go mod download`'d transitive `.mod` files) continues to get the high-fidelity milestone-055 output — full per-transitive parent-child topology, NOT the flatter step-5 fallback.

**Why this priority**: Tied with US1. The whole point of the 4-step (now 5-step) ladder is to use the highest-fidelity signal available. A step-5 implementation that accidentally pre-empts step-2 would be a regression: developers in a populated-cache state should NOT lose the parent-child edge attribution they have today.

**Independent Test**: All milestone-055 cache-populated tests continue to pass at their current edge counts. Specifically `mikebom-cli/tests/scan_go.rs` integration tests that exercise the populated-cache path emit byte-identical output pre-vs-post-091.

**Acceptance Scenarios**:

1. **Given** a Go project AND a populated `$GOMODCACHE` containing every transitive's `.mod` file, **When** mikebom scans without `--offline`, **Then** the resolver hits step 1 or 2 of the ladder (NOT step 5) and emits the full per-transitive parent-child graph.
2. **Given** the milestone-055 regression tests for `scan_go_source_tree_emits_transitive_edges_when_cache_present`, **When** the maintainer runs `cargo +stable test -p mikebom`, **Then** every milestone-055 test passes with byte-identical output.

---

### User Story 3 - Operators see provenance distinguishing high- vs low-fidelity components (Priority: P2)

An operator parsing mikebom's emitted SBOM can tell whether each Go transitive component was discovered via a high-fidelity source (`go mod graph` / populated `$GOMODCACHE` — full topology) or via the lower-fidelity `go.sum` fallback (flattened root → transitive set, no inter-transitive structure). This lets downstream consumers either trust the topology or treat it as approximate based on the per-component provenance annotation.

**Why this priority**: Constitution Principle X (Transparency) requires this. Operators making vulnerability-impact-radius decisions need to know which transitive components are connected via precise topology vs which represent the "somewhere in the closure" set. Lower than P1 because the COMPONENT SET correctness (US1) is more important than the topology-fidelity annotation; P2 because spec-native annotation mechanisms exist (per Clarifications session 2026-05-09) and are mandatory under the constitution.

**Independent Test**: Inspect emitted CDX 1.6 / SPDX 2.3 / SPDX 3 documents post-091 against the cri-tools fixture. Confirm step-5-derived components carry a provenance discriminator that step-1/2/3 components don't.

**Acceptance Scenarios**:

1. **Given** the cri-tools fixture in offline-cache-empty configuration, **When** mikebom emits the SBOM, **Then** every transitive Go component reached via `go.sum` fallback carries a per-component provenance annotation distinguishing it from cache-derived components.
2. **Given** the same fixture rerun in cache-populated configuration, **When** mikebom emits the SBOM, **Then** transitive components reached via the high-fidelity path do NOT carry the `go-sum-fallback` annotation (or carry a different value indicating their actual discovery step).

---

### Edge Cases

- **`go.sum` missing**: project has only `go.mod` (no `go.sum` — possible for very simple modules with zero deps). Step 5 falls through to step 6 (the existing direct-deps-only step-4 path renamed). Same behavior as today.
- **Both `go.sum` archive-hash AND `/go.mod` lines for the same `(module, version)`**: dedup to one component + one edge. Use `<module>@<version>` (without the `/go.mod` suffix) as the canonical key.
- **`go.sum` entries with `+incompatible` version suffix** (Go's pre-modules legacy versioning): preserve the suffix in the emitted PURL — `pkg:golang/<module>@<version>+incompatible`.
- **`replace` directives in `go.mod`** redirect one module to another path/version: the replaced version still appears in `go.sum`, but with the replacement target's hash. trivy emits the original module name + replacement version; we'll match that. `replace` directives that point at LOCAL paths (no version) don't appear in `go.sum` and aren't visible to step 5 — those still rely on steps 1–3.
- **Multi-module projects** (a single repo with multiple `go.mod` files): each `go.mod` has its own `go.sum`; the resolver needs to walk and emit per-module subgraphs. The existing milestone-055 multi-module support continues to apply; step 5 runs per-module-root.
- **Empty `go.sum`** (zero-dep project): emits the root module only. No transitives to emit; no fallback needed.
- **Malformed `go.sum` lines**: `parse_go_sum` (already exists at `legacy.rs:353`) silently skips malformed lines per the existing milestone-055 contract; step 5 reuses that behavior.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: When mikebom's Go-transitive-resolution ladder reaches the post-step-3 state (steps 1, 2, 3 each failed or returned empty results) AND a `go.sum` file exists at the project root, mikebom MUST parse `go.sum`, dedup `<module>@<version>` vs `<module>@<version>/go.mod` lines into a single (module, version) set, and emit one DEPENDS_ON edge from the root module to each unique entry.
- **FR-002**: Each transitive component emitted via FR-001 (i.e., reached via the step-5 go.sum-fallback path) MUST carry a per-component provenance annotation using each format's native construct: CDX 1.6 `Component.evidence.identity[].methods[].technique` (or `confidence`-bounded equivalent), SPDX 2.3 `package.annotations[]`, SPDX 3 `software_Package.evidence` (or analogous). Exact field-name selection per format is plan-level under Constitution Principle V's "audit native first" rule. `mikebom:*` properties MUST NOT be introduced unless plan-level research confirms no native construct fits. Components reached via step-1/2/3 paths MUST NOT carry the `go-sum-fallback` provenance value (so the discriminator's presence is meaningful).
- **FR-003**: Steps 1, 2, and 3 of the milestone-055 ladder MUST run unchanged. mikebom MUST NOT silently downgrade from a high-fidelity cache-populated path to step 5; the fallback only fires when the higher-fidelity steps fail or return empty.
- **FR-004**: When `go.sum` is absent OR step 5's emitted edge set is empty (the zero-dep project case), mikebom MUST fall through to step 6 (the existing direct-deps-only step-4 path renamed in the new ladder). No regression for projects that lack `go.sum`.
- **FR-005**: Existing `mikebom-cli/tests/scan_go.rs` integration tests MUST pass post-091 with zero behavioral changes. The cache-populated path continues to emit its current per-transitive parent-child topology.
- **FR-006**: The milestone-083 `transitive_parity_go.rs` regression test MUST be updated post-091 to encode the new edge-count baseline (31 → ≥130) and add at least one representative `pkg:golang/<root>@<rev> → pkg:golang/<transitive>@<v>` edge that exercises the step-5 path. Standard milestone-083 baseline-bump pattern (per quickstart Recipe 3 of milestone 087).
- **FR-007**: Per-format scope: CDX 1.6 / SPDX 2.3 / SPDX 3 outputs MUST all carry the FR-002 provenance discriminator. Goldens for the milestone-013 `golang/simple-module` fixture (a minimal fixture, NOT the cri-tools transitive-parity audit fixture) MUST regenerate IF the simple-module's `go.sum` populates step-5 edges; if simple-module emits zero step-5 edges (no `go.sum`, or the existing tests pass through step 1/2 because cache is mocked), goldens stay byte-identical.
- **FR-008**: The fix MUST NOT introduce new Cargo dependencies. `parse_go_sum` already exists at `mikebom-cli/src/scan_fs/package_db/golang/legacy.rs:353` and is the canonical entry point.

### Key Entities

- **Go-transitive resolution ladder**: the milestone-055 resolution chain at `mikebom-cli/src/scan_fs/package_db/golang/`. Steps 1 (`go mod graph`), 2 (`$GOMODCACHE` walk), 3 (proxy fetch — blocked by `--offline`), 4 (existing direct-deps fallback). Post-091: step 4 becomes step 5 (new go.sum-driven), and the existing direct-deps fallback becomes step 6.
- **`go.sum` entry**: a `<module>@<version> h1:<base64>` line, OR `<module>@<version>/go.mod h1:<base64>`. Both forms encode the same logical (module, version); dedup to a single component + edge.
- **Per-edge provenance annotation**: the new metadata distinguishing step-5 edges from step-1/2/3 edges. Plan-level decision picks the exact native construct per Constitution Principle V.
- **Audit fixture** (test target): `transitive_parity/go/` from `mikebom-test-fixtures` repo (vendored kubernetes-sigs/cri-tools @ v1.32.0) — accessed via `MIKEBOM_FIXTURES_DIR` per milestone 090.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Post-091, mikebom's `transitive_parity_go.rs` regression test reports an edge count ≥130 (up from 31). At least 90% of trivy's 142 edges are also in mikebom's emitted set (PURL-prefix matched).
- **SC-002**: 100% of pre-091 milestone-055 + milestone-013 golang test outcomes pass post-091 with no test deletions, no assertion weakenings, and zero golden regenerations for fixtures whose resolution doesn't reach step 5.
- **SC-003**: `cargo +stable test --workspace` post-091 reports `0 failed`. `./scripts/pre-pr.sh` clean. `cargo +stable clippy --workspace --all-targets -- -D warnings` zero warnings.
- **SC-004**: An operator inspecting any emitted SBOM can determine the provenance of any individual Go DEPENDS_ON edge by reading a single annotation field — no cross-field correlation or out-of-band documentation required.
- **SC-005**: Production-deps trivy CI gate (milestone 089) and the milestone-090 fixture-cache CI step continue to pass; this milestone touches neither.

## Assumptions

- The audit fixture (`transitive_parity/go/`, cri-tools @ v1.32.0) lives in the post-090 `mikebom-test-fixtures` repo; tests resolve it via `MIKEBOM_FIXTURES_DIR`. No fixture changes required.
- mikebom's go reader's existing `parse_go_sum` helper at `legacy.rs:353` correctly handles the canonical line format. If the helper has bugs surfaced during implementation, those are in scope.
- trivy's "flatten go.sum to root edges" approach is the right ceiling for the offline-cache-empty case. If a smarter heuristic emerges (e.g., partial cache + go.sum hybrid), it can be added as a future ladder step; this milestone matches trivy's flat approach as a baseline.
- Constitution Principle X (Transparency) requires per-edge provenance for the lower-fidelity step-5 path. Constitution Principle V (Specification Compliance) requires the chosen mechanism to be a native CDX/SPDX 2.3/SPDX 3 construct where possible, with `mikebom:*` only as a parity-bridging fallback.
- mikebom's existing `--offline` flag semantics + the `MIKEBOM_GOPROXY` precedence chain stay unchanged. Step 5 fires when steps 1–3 don't, regardless of whether the failure was network-blocked, cache-empty, or a mix.

## Dependencies

- Milestone 055 (Go transitive 4-step ladder) — the existing infrastructure this milestone extends.
- Milestone 083 (transitive-parity audit) — the regression-test scaffolding (`transitive_parity_go.rs`) that pins the post-091 edge-count baseline.
- Milestone 090 (fixture-repo split) — the audit fixture lives in the new repo; build.rs's `MIKEBOM_FIXTURES_DIR` resolves to it.
- Constitution Principle V (Specification Compliance) + Principle X (Transparency) — the per-edge provenance annotation mechanism MUST audit native fields first; this is binding.

## Out of Scope

- **Multi-step hybrid resolution** (e.g., partial-cache + go.sum). The fallback fires when ALL of steps 1–3 fail; mixing steps within a single scan is future work.
- **Improving trivy parity beyond the flat go.sum approach**. trivy's go.sum parsing has known limitations (no parent-child topology, weird `replace` semantics); we match trivy without fixing trivy's gaps.
- **Adding a flag to disable step 5** (e.g., `--no-go-sum-fallback`). Step 5 is strictly better than step 6 in the offline-cache-empty case; an opt-out flag adds complexity for no clear benefit. Operators preferring the old behavior can use `--include-dev=false` or scan with `$GOMODCACHE` populated.
- **Improving step-1/2/3 fidelity**. Those paths are unchanged.
- **Bumping the cri-tools fixture version**. Same fixture as milestone 083; reusing it directly.
- **Per-edge provenance for non-Go ecosystems**. Other ecosystems' resolution paths are unchanged; this milestone introduces the annotation only for Go and only for step-5-derived edges.
- **Refactoring the milestone-055 ladder beyond adding step 5**. The existing 4 steps stay shape-identical; step 5 inserts at the right point and step 4 renames to step 6.
