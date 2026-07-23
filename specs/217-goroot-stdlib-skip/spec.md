# Feature Specification: Skip GOROOT stdlib module as a Go main-module candidate

**Feature Branch**: `217-goroot-stdlib-skip`
**Created**: 2026-07-22
**Status**: Draft
**Input**: User description: fix [waybill#631](https://github.com/kusari-oss/waybill/issues/631). MVP scope: (1) module-path filter for `"std"` + `"cmd"` in the Go walker; (2) regression fixture that stubs a mini-GOROOT layout and asserts no main-module is emitted; (3) optional transparency annotation.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — CI scan of a Go-toolchain-carrying image produces a clean log + correct SBOM (Priority: P1)

An operator runs `waybill sbom scan --image <ref>` against a container image that ships a Go toolchain at `/usr/local/go/` (or another install location) — typical for images that bundle Go for runtime use (govulncheck, `go build` at runtime, dev images, `golang:*`-derived images). Waybill MUST NOT treat the toolchain's stdlib source tree (`$GOROOT/src/`) or command tree (`$GOROOT/src/cmd/`) as a user-project main-module.

**Why this priority**: The current behavior (waybill#631) breaks every CI job that scans a Go-toolchain image:
1. `go list all` preflight fails with ~180 "use of internal package … not allowed" errors when run inside `$GOROOT/src/`
2. Waybill's WARN log dumps every stderr line, GitHub Actions' Go problem-matcher regex converts each to `##[error]` annotations, polluting the workflow UI with hundreds of misleading badges per build
3. The emitted SBOM contains a `pkg:golang/std@...`-shaped pseudo-main-module component with `waybill:build-inclusion = "unknown"` markers — false-positive noise on every downstream vuln scan / consumer that keys on the SBOM's main-module identity

**Independent Test**: Author a minimal fixture that mirrors the GOROOT/src layout (`fixture/usr/local/go/src/go.mod` declaring `module std`; optionally a companion `cmd/go.mod` declaring `module cmd`). Run `waybill sbom scan --path <fixture>`. Assert: (a) exit code 0, (b) no `##[error]`- or `Error:`-shaped lines in stderr, (c) emitted SBOM does NOT contain a component with PURL `pkg:golang/std@...` or PURL `pkg:golang/cmd@...` or any main-module component whose evidence points at `$GOROOT/src/`.

**Acceptance Scenarios**:

1. **Given** a rootfs containing `<path>/usr/local/go/src/go.mod` with `module std`, **When** waybill scans that rootfs, **Then** no main-module component is emitted for the stdlib and no `go list all` preflight is attempted inside GOROOT.
2. **Given** a rootfs additionally containing `<path>/usr/local/go/src/cmd/go.mod` with `module cmd`, **When** waybill scans that rootfs, **Then** no main-module component is emitted for the toolchain-command tree either.
3. **Given** a real user Go project alongside the toolchain (e.g., `<path>/app/go.mod` with `module example.com/app` at the same rootfs), **When** waybill scans, **Then** the user project's main-module IS emitted, only the toolchain-internal modules are skipped.
4. **Given** any Go image whose toolchain triggers the pre-feature bug, **When** the same image is scanned post-feature, **Then** the CI workflow log contains zero `Error:\s+.*\.go:\d+:\d+:\s+use of internal package` lines caused by waybill.

---

### User Story 2 — Transparency: operators can see waybill detected a Go toolchain (Priority: P2)

When waybill's walker skips a `go.mod` because it declares a toolchain-internal module (`std` or `cmd`), an operator inspecting the emitted SBOM SHOULD be able to see that waybill observed the toolchain — via a document-level annotation naming the observation. This lets consumers distinguish "the scanned image has no Go toolchain" from "the scanned image has a Go toolchain that waybill correctly skipped for main-module purposes".

**Why this priority**: Useful for auditing + downstream consumer decisions (e.g., a Grype-style scanner may want to know a Go toolchain is present so it can cross-reference against Go stdlib CVEs). Not blocking — the P1 fix alone eliminates the reported bug. Follows Constitution Principle X (Transparency).

**Independent Test**: Run waybill against a rootfs containing a Go toolchain. Assert the emitted SBOM's document-level properties/annotations include a `waybill:go-toolchain-detected` signal naming the toolchain's install path.

**Acceptance Scenarios**:

1. **Given** a rootfs containing `<path>/usr/local/go/src/go.mod` with `module std`, **When** waybill scans, **Then** the emitted SBOM's document-scope annotations include a `waybill:go-toolchain-detected` entry naming the detected toolchain-root path (e.g., `<path>/usr/local/go`) OR the observed `module std` go.mod path.
2. **Given** a rootfs with NO Go toolchain, **When** waybill scans, **Then** the `waybill:go-toolchain-detected` annotation is absent (silence = "not observed", matching the waybill precedent for observation-based annotations).

---

### Edge Cases

- **Multiple toolchains in the same rootfs** (multi-stage Docker build accidentally copied the builder's GOROOT + kept the runtime GOROOT): each toolchain-internal go.mod is skipped independently. The optional transparency annotation lists each detected toolchain root once (dedup + sort).
- **Non-standard install location** (Go installed at `/opt/go`, `/home/user/go`, `~/sdk/go1.22.5/`, etc.): detection MUST NOT depend on the install path. The `module std` / `module cmd` module-path signal is the primary trigger.
- **User has a package named `std` or `cmd` in their own workspace** (extremely rare — those names are widely known to be reserved in the Go ecosystem, but not enforced by cargo/go): out of scope. If a user genuinely publishes their own `module std`, they will be treated as a toolchain-internal module by waybill and skipped. Documented as an assumption.
- **Test-data go.mods inside GOROOT/src** (e.g., `$GOROOT/src/debug/buildinfo/testdata/go117/go.mod`): pre-existing `should_skip_descent` at `legacy.rs:2515+` skips `testdata/` directories, so no additional handling required. Verify by test.
- **Go source-checkout for Go compiler development itself** (someone building the Go toolchain from source): the same `module std` filter would skip it, which is correct — a Go compiler source checkout isn't a "user project" for supply-chain SBOM purposes.
- **Older Go versions (< 1.18) whose GOROOT/src had no go.mod**: pre-1.18 stdlib source was walked as loose `.go` files without any `go.mod`. The bug doesn't trigger for those toolchains because the walker never picks them up as main-module candidates. No behavior change needed for pre-1.18 support.
- **`$GOROOT/pkg/mod` and `$GOROOT/pkg/tool`**: these directories don't carry `go.mod` files that would trigger the walker; the pre-existing `.../go/pkg/mod/...` skip at `legacy.rs:2532+` already handles the module cache case for user-installed builds.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Waybill MUST skip any `go.mod` file whose `module` line declares exactly the string `"std"` (case-sensitive) when enumerating Go project-root candidates. No main-module component MUST be emitted for such a directory, and no downstream `go list` / `go mod why` preflight MUST run against it.
- **FR-002**: Waybill MUST also skip `go.mod` files whose `module` line declares exactly `"cmd"` (case-sensitive) — matching the toolchain-commands module at `$GOROOT/src/cmd/go.mod`.
- **FR-003**: The skip decision MUST happen at the walker level (before any downstream `go list` preflight or PackageDbEntry construction), so no stderr output from a toolchain-triggered `go list` failure can leak into logs.
- **FR-004**: The skip MUST NOT affect user projects with legitimate Go module paths. Every `go.mod` declaring `module <any-other-path>` (e.g., `example.com/app`, `github.com/user/repo`, `internal.local/service`) MUST continue to be picked up as a candidate project root, byte-identical to pre-feature behavior.
- **FR-005**: The skip MUST NOT depend on the filesystem install path (must NOT hardcode `/usr/local/go`, `/opt/go`, etc.). Detection MUST be based solely on the `module` declaration inside the go.mod file. Rationale: install locations vary widely across distros, container images, and user layouts; the module-path signal is universal.
- **FR-006**: When a toolchain-internal `go.mod` is skipped, waybill SHOULD emit a document-level annotation `waybill:go-toolchain-detected` with a value naming the detected toolchain's root path (parent of `src/`) OR the observed go.mod path. Multiple detected toolchains in one scan yield a single annotation whose value is a JSON-encoded array of paths, sorted lex and deduplicated (matches the existing `waybill:workspaces-detected` pattern for observation aggregation).
- **FR-007**: The change MUST NOT affect any pre-feature SBOM output on any repo that doesn't contain a `module std` or `module cmd` go.mod. Pre-feature `{cdx,spdx,spdx3}_regression` byte-identity tests MUST continue to pass.
- **FR-008**: When the P2 transparency annotation is present, the CDX / SPDX 2.3 / SPDX 3 emit paths MUST route it to the format-native document-level property/annotation landing slot the parity framework already uses for `waybill:workspaces-detected` (C121 catalog row). A new parity-catalog entry MUST be added per Constitution Principle V documentation requirement.

### Key Entities

- **GOROOT-internal go.mod**: a `go.mod` file whose `module` line declares exactly `"std"` or `"cmd"`. Ships with every Go 1.18+ install at `$GOROOT/src/go.mod` and `$GOROOT/src/cmd/go.mod`. Represents the Go toolchain's own module boundary, NOT a user project.
- **User project go.mod**: any `go.mod` whose `module` line declares a non-toolchain-internal path (`example.com/app`, `github.com/user/repo`, etc.). Represents a scanning-target and continues to participate in main-module emission unchanged.
- **Detected Go toolchain**: a filesystem observation — the parent directory of a `src/go.mod` declaring `module std`. Typically `$GOROOT`; example paths: `/usr/local/go`, `/opt/go`, `/opt/homebrew/opt/go/libexec`. The optional transparency annotation surfaces this observation without affecting emission decisions elsewhere.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the waybill#631 reproducer (any container image with a Go toolchain at `/usr/local/go/`), the emitted SBOM contains ZERO components whose PURL matches `pkg:golang/std@*` or `pkg:golang/cmd@*` and ZERO main-module components whose evidence path starts with the GOROOT install root.
- **SC-002**: On the same reproducer, the stderr output of the scan contains ZERO lines matching the pattern `Error:\s+.*\.go:\d+:\d+:\s+use of internal package .* not allowed`. Consequence: GitHub Actions' Go problem-matcher stops emitting `##[error]` annotations from waybill scans of Go-toolchain-carrying images.
- **SC-003**: A newly-authored fixture at `waybill-cli/tests/fixtures/goroot_stub/` mimicking a minimal GOROOT layout (`src/go.mod` with `module std`, `src/cmd/go.mod` with `module cmd`) produces an SBOM with zero pkg:golang/std / pkg:golang/cmd components AND (when a companion user project is present at the same rootfs) EXACTLY one user main-module component identifying the user project.
- **SC-004**: 100% of the pre-existing `cdx_regression`, `spdx_regression`, `spdx3_regression` byte-identity tests continue to pass — no drift on any fixture that doesn't contain a `module std` or `module cmd` go.mod.
- **SC-005**: For scans that DO detect a Go toolchain (the P2 story), the emitted SBOM's document-level annotations include a `waybill:go-toolchain-detected` entry naming the detected root path(s). For scans without a toolchain, the annotation is absent (silence = "not observed").

## Assumptions

- **Ecosystem convention**: `module std` and `module cmd` are effectively reserved for the Go toolchain itself. No mainstream user-published Go module uses these paths as their module identity. The tiny risk (user genuinely publishes their own `module std`) is accepted; documented in Edge Cases.
- **Go version coverage**: The bug triggers only for Go 1.18+ toolchains (which introduced `$GOROOT/src/go.mod`). Pre-1.18 toolchains don't ship a go.mod under GOROOT/src, so the walker never picked them up. No back-compat concern.
- **Detection at walker time is cheaper than emission-time filtering**: parsing the go.mod's `module` line requires the same file-read as building the main-module entry, but doing it at the walker cuts off the entire downstream pipeline (no `go list` preflight, no `go mod why` analysis, no stderr dump, no PackageDbEntry construction). The reader is already going to read the file — the check is a single string comparison after the existing parse.
- **Transparency annotation value shape**: matches the existing `waybill:workspaces-detected` convention — a JSON-encoded array of paths for aggregation across multiple detections in one scan (uncommon but possible). Single-toolchain scans emit a 1-element array (parity with the C121 pattern).
- **CI problem-matcher compatibility**: GitHub Actions' Go problem-matcher regex is not documented as stable; the SC-002 measure focuses on eliminating waybill-produced `Error:\s+.*\.go:\d+:\d+:` lines rather than promising any specific `##[error]` count. Consequence: whether GitHub aggregates the badges differently across runners is out of scope.
- **No Go toolchain synthesis**: This feature does NOT emit a synthetic `Go toolchain <version>` component for the detected toolchain. That's a nice-to-have follow-up (would require reading `$GOROOT/VERSION` and constructing a coherent PURL) but expands scope beyond a bug fix. Tracked as a potential future feature.

## Dependencies

- Milestone 053 (Go workspace-version resolver) — merged. Provides `run_git_describe_with_timeout` primitive (used by m216 too). No direct dependency, but the same file (`legacy.rs`) is touched.
- Milestone 176 (`waybill:workspaces-detected` C121) — merged. Provides the value-shape precedent (JSON-encoded sorted-deduplicated path array) that the P2 annotation follows.
- Constitution Principle V (standards-native precedence) — the new annotation MUST be documented in `docs/reference/sbom-format-mapping.md` with a parity-bridging justification (no format-native "Go toolchain observation" field exists).
- No new external crates. Feature lives entirely in `waybill-cli/src/scan_fs/package_db/golang/legacy.rs`.

## Out of Scope

- **Emitting a synthetic `Go toolchain <version>` component**: waybill#631 mentions this as a nice-to-have ("at most inventoried as a single 'Go toolchain X.Y.Z' component"). Deferred — it's a separate feature (PURL construction, VERSION file parsing, downstream consumer semantics) worth its own spec.
- **Skipping other language toolchains** (Ruby/Python/Node/etc. install trees at `/usr/local/<lang>/`): out of scope. Each language has its own module conventions; this fix is Go-specific.
- **Filtering `go list`/`go mod why` stderr more aggressively**: the P1 fix eliminates the stderr flood by not running these commands at all against GOROOT. General-purpose stderr suppression for other failure paths is a separate concern.
- **CI-side problem-matcher configuration**: this feature makes waybill produce clean stderr; whether users also want to disable GitHub Actions' Go problem-matcher via `::remove-matcher` is up to them.
- **Cross-ecosystem edge resolution for m216 pkg:generic/ main-modules**: tracked separately at [waybill#633](https://github.com/kusari-oss/waybill/issues/633).
