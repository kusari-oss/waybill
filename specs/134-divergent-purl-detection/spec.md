# Feature Specification: Divergent-PURL collision detection in main-module dedup

**Feature Branch**: `134-divergent-purl-detection`
**Created**: 2026-06-21
**Status**: Draft
**Input**: User description: "Divergent-PURL collision detection in main-module dedup. When mikebom's main-module dedup finds two-or-more manifest files claiming the same `pkg:<ecosystem>/<name>@<version>` identity but with different declared direct-dep sets OR different deep-hashes, emit a structured machine-readable signal in the SBOM. Today milestone 064 detects same-PURL dedup but only logs `tracing::warn!`; this milestone upgrades that to a machine-readable SBOM signal. Scope: cargo first; detection logic ecosystem-agnostic. Default: soft annotation. Closes #125."

## Background

Milestone 064 introduced cargo main-module emission with same-PURL deduplication: when mikebom's filesystem walker finds two-or-more `Cargo.toml` files claiming the same `pkg:cargo/<name>@<version>` identity, the dedup logic picks the first discovered one and emits a single component. The realistic cases the milestone-064 spec considered (vendored copies under `vendor/`, mirrors under `examples/`, extractions under `target/package/`) have **identical** declared dep sets, so first-wins is harmless and a `tracing::warn!` provides visibility for the human operator.

The milestone-064 spec deliberately deferred the more interesting case: same-PURL collision where the **content actually diverges** — different declared `[dependencies]`, or different file-tree deep hashes. That divergent case is what this milestone addresses.

Divergent-PURL is a **potential supply-chain signal**:

- **Accidental**: a developer copy-pasted a crate skeleton without changing the name/version, then edited the local copy.
- **Adversarial**: typosquat-by-shadowing — a malicious actor lands a vendored copy that retains the legitimate `pkg:cargo/foo@1.2.3` identity but contains different code (which produces different deep-hashes per milestone 038).

Neither Trivy nor Syft detects this case. Surfacing it in the SBOM is a mikebom differentiator.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator detects an accidental shadow copy (Priority: P1)

A Rust developer's workspace contains `crates/foo/Cargo.toml` declaring `name = "foo", version = "1.2.3"` and a `vendor/foo/Cargo.toml` that ALSO declares `name = "foo", version = "1.2.3"`. Sometime in the past, the vendored copy was edited locally — it now declares an additional `[dependencies]` entry that the upstream `crates/foo` does not.

The operator runs `mikebom sbom scan --path .` and receives an SBOM. The emitted root component for the workspace still carries a single `pkg:cargo/foo@1.2.3` identity (per milestone 064's dedup), BUT the component now also carries an annotation indicating that mikebom detected a same-PURL collision with divergent declared dep sets, and lists the discovered manifest paths.

**Why this priority**: The headline detection use case. Without this, divergent-PURL is invisible in the emitted SBOM — only present in `tracing::warn!` log output that downstream automation can't easily consume.

**Independent Test**: Construct a synthetic fixture with two `Cargo.toml` files declaring the same PURL but with different `[dependencies]` blocks. Scan with `mikebom sbom scan`. Parse the emitted CDX JSON. Assert that the `pkg:cargo/<name>@<version>` component carries the divergence annotation AND that the annotation lists both manifest paths AND identifies the divergence reason as `deps-differ`.

**Acceptance Scenarios**:

1. **Given** a workspace with two `Cargo.toml` files claiming `pkg:cargo/foo@1.2.3` and divergent `[dependencies]` blocks, **When** the operator runs `mikebom sbom scan --path .`, **Then** the emitted SBOM contains a `pkg:cargo/foo@1.2.3` component with a divergence annotation listing both discovered paths and reason `deps-differ`.
2. **Given** a workspace with two `Cargo.toml` files claiming `pkg:cargo/foo@1.2.3` with IDENTICAL declared dep sets, **When** the operator runs `mikebom sbom scan --path .`, **Then** the emitted SBOM does NOT contain a divergence annotation (no false positive on harmless dedup; current milestone-064 first-wins behavior preserved bit-for-bit).
3. **Given** the same divergent workspace as scenario 1, **When** the operator runs `mikebom sbom scan --path .`, **Then** the existing milestone-064 `tracing::warn!` STILL fires alongside the new annotation (so existing log-watching automation keeps working).

---

### User Story 2 — Operator detects an adversarial shadow copy via deep-hash (Priority: P2)

A workspace contains `crates/foo/Cargo.toml` and `vendor/foo/Cargo.toml` that declare the same PURL AND identical `[dependencies]`, but `vendor/foo/src/lib.rs` contains modified code (a typosquat-by-shadowing attack). The declared-dep comparison alone would NOT catch this — the dep sets are identical by design — but the per-component deep hashes from milestone 038 diverge.

The operator runs `mikebom sbom scan --path . --deep-hash` and receives an SBOM. The `pkg:cargo/foo@1.2.3` component carries the divergence annotation with reason `hashes-differ`.

**Why this priority**: P2 because (a) it requires `--deep-hash` mode (the default off-state of milestone 038), and (b) the dep-set comparison from US1 catches the more common case. Deep-hash divergence is the explicit supply-chain-attack catch.

**Independent Test**: Construct a synthetic fixture with two `Cargo.toml` files claiming the same PURL and identical `[dependencies]` blocks, but with different `src/lib.rs` contents. Run `mikebom sbom scan --path . --deep-hash`. Assert the annotation appears with reason `hashes-differ`.

**Acceptance Scenarios**:

1. **Given** a workspace with two `Cargo.toml` files claiming the same PURL, identical declared deps, divergent source-file contents, **When** the operator runs `mikebom sbom scan --path . --deep-hash`, **Then** the emitted SBOM contains the divergence annotation with reason `hashes-differ`.
2. **Given** the same workspace as scenario 1, **When** the operator runs `mikebom sbom scan --path .` (no `--deep-hash`), **Then** the annotation is NOT emitted (deep-hash comparison only fires when the per-file SHA work is being done anyway; doesn't add cost to the default path).

---

### User Story 3 — Operator gets a scan-wide collisions summary (Priority: P3)

A workspace contains multiple divergent-PURL collisions across different crates. The operator wants a single document-scope view of every collision detected in the scan rather than scanning every component annotation looking for the property.

**Why this priority**: P3 — useful operational convenience, but the per-component annotations from US1/US2 already carry the necessary data. This is an aggregation view.

**Independent Test**: Construct a fixture with three divergent same-PURL collisions across three different crates. Scan. Assert a document-scope `mikebom:purl-collisions-detected` annotation lists all three collisions in one place.

**Acceptance Scenarios**:

1. **Given** a workspace with three independent divergent-PURL collisions, **When** the operator runs `mikebom sbom scan --path .`, **Then** the emitted SBOM contains a document-scope annotation listing all three collisions (one entry per collision with paths + reason).

---

### Edge Cases

- **Workspace member sharing root's name+version**: A workspace's root `Cargo.toml` declares `name = "foo", version = "1.0.0"` AND a member crate at `crates/bar/Cargo.toml` declares `name = "foo", version = "1.0.0"`. This is degenerate-but-valid Rust. Detection MUST treat this as a collision (both produce the same PURL) and apply the divergent-check logic.
- **Path-dep with same name+version as a registry crate**: A path dep declared with `[dependencies.foo] = { path = "vendor/foo" }` where the path-dep's `Cargo.toml` carries `name = "foo", version = "1.2.3"` matching a registry crate also pulled in. The cargo resolver distinguishes these in the lock file, but at the manifest-walk stage mikebom sees two `Cargo.toml`s with identical name+version. Detection treats this as a collision per the dedup rule from milestone 064; the divergence check fires if dep sets or hashes differ.
- **Three or more colliding manifests**: Detection MUST list ALL discovered paths in the annotation, not just a representative pair. Divergence is computed across the full set (any pair diverging triggers the annotation).
- **Pre-existing milestone-064 warn-emission**: The `tracing::warn!` from milestone-064 dedup MUST continue to fire on every same-PURL collision (whether divergent or not), so existing log-watching automation is not broken.
- **No-collision scans**: Scans where no two manifests resolve to the same PURL MUST NOT carry any divergence annotation (no SBOM bloat, no false-positive signal).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect when two-or-more `Cargo.toml` manifest files within a single scan resolve to the same `pkg:cargo/<name>@<version>` PURL.
- **FR-002**: System MUST compare the declared direct dependency sets across colliding manifests, where the dep set is the union of `[dependencies]`, `[dev-dependencies]`, and `[build-dependencies]` table keys.
- **FR-003**: When the declared direct dep sets differ across colliding manifests, system MUST emit BOTH (a) a per-component property on the deduped `pkg:<ecosystem>/<name>@<version>` component identifying the collision and its divergence reason, AND (b) a document-scope summary annotation listing every collision detected in the scan. The two surfaces are strictly additive: per-component is the primary detection signal (consumers asking "is THIS component divergent?"); document-scope is the aggregation view (consumers enumerating every collision in one query). Both surfaces follow the milestone-061 `mikebom:graph-completeness` annotation pattern (structured value carrying detection context).
- **FR-004**: The emitted signal MUST identify every manifest path that participated in the collision and the divergence reason (`deps-differ` or `hashes-differ`).
- **FR-005**: When `--deep-hash` mode is enabled and per-file SHA computation is already happening, system MUST additionally compare the deep hashes across colliding manifests and emit `hashes-differ` if they diverge.
- **FR-006**: When colliding manifests have identical declared dep sets AND (if `--deep-hash`) identical deep hashes, system MUST NOT emit any divergence signal — current milestone-064 first-wins behavior preserved bit-for-bit.
- **FR-007**: Default scan behavior MUST NOT fail or exit non-zero on a divergent-PURL detection — the signal is informational only. A hard-fail mode is explicitly out of scope for this milestone (follow-up CLI flag in a future milestone).
- **FR-008**: System MUST continue to emit the existing milestone-064 `tracing::warn!` on every same-PURL collision (whether divergent or not), preserving the log-watching contract.
- **FR-009**: When no colliding manifests are found in a scan, the emitted SBOM MUST NOT carry any divergence-related annotation or property — no bloat, no spurious signal.
- **FR-010**: Detection logic MUST be structured to apply ecosystem-agnostically at the data-model layer (operating on the deduped `(PURL, paths[], deep_hashes[])` tuple). Only the cargo wiring is in scope for this milestone, but the detection function MUST NOT contain cargo-specific logic that would block reuse in follow-up milestones for npm / maven / pip / gem / go-binary.
- **FR-011**: The chosen signal shape MUST be subjected to the Constitution Principle V audit (standards-native fields take precedence over `mikebom:*` annotations). If a CycloneDX or SPDX native field expresses "same identity, divergent content" semantics, that native field MUST be emitted instead of (or in addition to) the mikebom annotation.

### Key Entities

- **Manifest collision**: A set of two-or-more manifest file paths whose contents resolve to the same `pkg:<ecosystem>/<name>@<version>` PURL identity. Attributes: PURL string, paths list, per-path declared-dep-set (sorted), per-path deep-hash (optional, only when `--deep-hash` is set).
- **Divergence record**: Result of evaluating a collision. Attributes: PURL string, paths list, divergence reason enum (`deps-differ`, `hashes-differ`, `both`), and a structured payload listing which dep names or which hash values diverge (to aid human triage). Emitted only when divergence is actually detected.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A synthetic fixture with two `Cargo.toml` files claiming `pkg:cargo/foo@1.2.3` where one has an additional `[dependencies] bar = "1.0"` entry MUST produce an SBOM containing a divergence signal with reason `deps-differ` and the two manifest paths.
- **SC-002**: A synthetic fixture with two `Cargo.toml` files claiming the same PURL and IDENTICAL declared dep sets MUST produce an SBOM containing NO divergence signal. The emitted SBOM MUST be byte-identical (modulo timestamps + serial numbers) to the SBOM produced by the same fixture before this milestone shipped.
- **SC-003**: A synthetic fixture with two `Cargo.toml` files claiming the same PURL, identical declared deps, and divergent `src/lib.rs` contents MUST produce an SBOM containing a divergence signal with reason `hashes-differ` ONLY when `mikebom sbom scan --deep-hash` is passed.
- **SC-004**: Across all CI-tracked realistic-project fixtures (knative-func, express, flask, ripgrep, jackson-databind, sinatra, debian-rootfs, alpine-rootfs, rockylinux-rootfs — see `.github/workflows/realistic-projects.yml`), the divergence-detection path MUST NOT increase scan wall-clock by more than 2% (the no-collision path is essentially free — a single hash-set lookup per emitted root component).
- **SC-005**: An external SBOM consumer reading the emitted CDX JSON MUST be able to enumerate every divergent-PURL collision in the scan via a single structured query against the SBOM document — no log-parsing required. (Operationalizes the "machine-readable signal" promise.)

## Assumptions

- **Cargo-only this milestone**: Detection logic is structured to apply to any ecosystem with main-module dedup (per the issue's cross-cutting note), but only the cargo wiring is implemented. Follow-up milestones extend to npm, maven, pip, gem, go-binary.
- **Soft-only this milestone**: Default behavior is a soft annotation (always emit on divergence; never fail). A hard-error mode (fail-the-scan on divergence) is a deferred follow-up behind a CLI flag.
- **No new scan-wide work in the no-collision path**: Detection piggybacks on the existing milestone-064 dedup hash-set; the divergence check fires only at the point where dedup already activates. Zero overhead for the common case (no collisions in the scan).
- **Existing milestone-061 transparency-annotation pattern is the template**: The signal shape MUST follow the same documented pattern as `mikebom:graph-completeness` (per-component property, with a structured value carrying the detection context).
- **Trivy / Syft don't address this case**: Empirical claim from the issue body, treated as a project-positioning input for the spec but not gated against in CI.

## Out of Scope

- Hard-fail mode (`mikebom sbom scan --fail-on-divergent-purl` or similar). Deferred to a follow-up milestone.
- Extension to non-cargo ecosystems (npm / maven / pip / gem / go-binary). Detection logic structured for reuse; wiring is per-ecosystem follow-up.
- Detection of divergent content WITHOUT a same-PURL collision (e.g., two crates with different PURLs but accidentally identical names). Out of scope — the entire premise of this feature is that PURL identity equality is the trigger.
- Dependency-graph-deep divergence comparison (e.g., comparing transitive dep trees, not just direct deps). The issue is explicit that the direct-dep set comparison is the in-scope signal; transitive divergence is a separate, much costlier check.
- Cross-scan persistence of detected collisions for trend tracking. Each scan is independent; the annotation reflects only what mikebom detected in that scan.
