# Feature Specification: Go transitive-edge coverage investigation + gap surface

**Feature Branch**: `160-go-transitive-coverage`
**Created**: 2026-07-04
**Status**: Draft
**Input**: User description: "494" (implement fix for [issue #494](https://github.com/kusari-oss/mikebom/issues/494))

## Clarifications

### Session 2026-07-04

- Q: How is `partial` vs `unknown` decided for `mikebom:go-transitive-coverage`? → A: Reason-code-driven (mirrors milestone-158 Q1 caution-first). `unknown` iff a documented "we can't measure" reason applies (offline mode, `off` in GOPROXY chain, `go mod graph` subprocess degraded). `partial` iff we ran the pass and ≥1 module ended up `unresolved`. `complete` iff every module resolved via steps 1–4 of the ladder. Deterministic; not fragile to network flake counts.
- Q: Should `mikebom:go-transitive-source` be universal-per-component or signal-only? → A: Universal — every Go component carries the annotation naming which ladder step resolved it (`go-mod-graph` / `module-cache` / `proxy-fetch` / `go-sum-fallback` / `unresolved`). Matches milestone-158 C104 + milestone-159 alias-annotation universal-presence pattern. Consumers gate on annotation value, not presence/absence (avoids the "no annotation = success" silent-lie failure mode). Approx +300 annotations on test-podman; acceptable per Constitution Principle X (Transparency).
- Q: What ground-truth defines SC-001's ≥90% edge-match measurement? → A: `go mod graph` — same generator as the milestone-157 audit script that established the 52.2% pre-160 baseline. Union of ALL module-`requires`-module edges across the module closure. Cross-platform-inclusive (matches mikebom's build-tag-agnostic scan posture per milestone 112's out-of-scope note). Deterministic single-command invocation. Rejected `go list -m all` / `go mod why -m all` because build-tag filtering would artificially narrow the denominator and diverge from mikebom's own scan semantics.
- Q: Should the `mikebom:go-transitive-coverage-reason` code vocabulary be closed or open? → A: Closed but extensible — the FR-005 5-code vocab (`offline`, `goproxy-off`, `go-mod-graph-degraded`, `fetch-failures`, `unknown-modules`) is the stable consumer surface for milestone 160; new codes require a future milestone bump. Exactly mirrors milestone-158's SC-005 governance of the `mikebom:graph-completeness-reason` 8-code list. Balances consumer stability (jq recipes with fixed switch statements stay correct) with deliberate future-proofing.

## Motivation

Discovered during the milestone-157 Round-2 audit against `kusari-sandbox/test-podman`: even in ONLINE mode (with `GOPROXY` reachable), mikebom's Go dep-graph covers only ~50% of the edges reported by `go mod graph`.

Empirical measurement 2026-07-03 on `test-podman` (300+ Go modules in closure):

| shape | count | % |
|---|---|---|
| EXACT-MATCH (mikebom == `go mod graph`) | 95 | 52.2% |
| EMITTED-SUPERSET (mikebom emits MORE) | 0 | 0% |
| EMITTED-SUBSET (mikebom MISSING edges) | 38 | 20.9% |
| DIVERGE (both extra AND missing) | 0 | 0% |
| Not emitted at all | 49 | 26.9% |
| **Total mismatch** | **87 / 182** | **47.8%** |

Concrete missing-edge example — `github.com/containernetworking/plugins@v1.9.1`'s own `go.mod` declares 20+ direct requires; mikebom emits 38 edges for this module but misses 5, including two cross-platform ones (`alexflint/go-filemutex`, `buger/jsonparser`) that can't be explained by a Linux/Windows-only build-tag filter:

```text
require (
    github.com/Microsoft/hcsshim v0.13.0             # <-- MISSING (Windows-only, arguably fine)
    github.com/alexflint/go-filemutex v1.3.0         # <-- MISSING (cross-platform!)
    github.com/buger/jsonparser v1.1.1               # <-- MISSING (cross-platform!)
    github.com/coreos/go-iptables v0.8.0             # <-- MISSING (Linux-only, arguably fine)
    github.com/containerd/cgroups/v3 v3.0.3          # <-- MISSING (indirect on Windows)
    github.com/containernetworking/cni v1.3.0        # emitted ✓
    github.com/coreos/go-systemd/v22 v22.7.0         # emitted ✓
    github.com/godbus/dbus/v5 v5.2.2                 # emitted ✓
    ...
)
```

Offline mode (`--offline` / `GOPROXY=off`) is MUCH worse — test-podman drops to 7.29% EXACT-MATCH because milestone-055's proxy-fetch-based transitive builder can't populate at all.

Consumer impact: **vulnerability scanners silently miss 47.8% of the true dep-graph** in the online case, and 92.7% in the offline case. A `cargo audit`-style tool that BFS-traverses the emitted graph would never find CVEs in the un-emitted modules.

## User Scenarios & Testing

### User Story 1 - SBOM consumer gets `go mod graph`-parity coverage on online scans (Priority: P1)

An SBOM consumer (Kusari Inspector, a vulnerability scanner, an SBOM comparator) loads mikebom's Go SBOM for a repo scanned in online mode and finds that ≥90% of module-edges reported by `go mod graph` on the same repo are present. Cross-platform direct requires (like `alexflint/go-filemutex`) MUST be emitted; platform-specific requires (like `Microsoft/hcsshim` on Linux) MAY be filtered based on host build-tag context but the filtering MUST be traceable via annotation.

**Why this priority**: This is the observed bug's user-visible symptom. Without this fix, mikebom's Go coverage is an SBOM completeness failure (Constitution Principle VIII) — vulnerability scans silently miss half the closure, defeating the core value proposition of an SBOM for supply-chain integrity.

**Independent Test**: Scan `kusari-sandbox/test-podman` in online mode (`GOPROXY=https://proxy.golang.org`). For each module in the emitted CDX, look up its declared direct requires from `go mod graph`. Assert:

- ≥90% of go-mod-graph edges have a corresponding entry in mikebom's `dependencies[].dependsOn`.
- The 5 specific missing edges from the milestone-157 audit (`containernetworking/plugins@v1.9.1` → alexflint/go-filemutex, buger/jsonparser, hcsshim, coreos/go-iptables, containerd/cgroups/v3) are either present OR annotated with a documented reason code (`build-tag-filtered: <host-goos-goarch>` OR `deps.dev-missing` OR `proxy-fetch-degraded`).

**Acceptance Scenarios**:

1. **Given** `test-podman` scanned in online mode with `GOPROXY=https://proxy.golang.org`, **When** mikebom emits the CDX, **Then** the emitted-edge count MUST be ≥90% of what `go mod graph` reports (measured as `|mikebom_edges ∩ go_mod_graph_edges| / |go_mod_graph_edges|`).

2. **Given** the same scan, **When** enumerating `github.com/containernetworking/plugins@v1.9.1`'s dependsOn, **Then** it MUST include the 2 cross-platform direct requires from the milestone-157 audit (`alexflint/go-filemutex@v1.3.0`, `buger/jsonparser@v1.1.1`).

3. **Given** a module whose direct requires include a platform-specific dep (e.g. `Microsoft/hcsshim` on Linux hosts), **When** mikebom filters it out based on GOOS/GOARCH build-tag context, **Then** the emitted component MUST carry a `mikebom:go-build-tag-filtered = "<goos-goarch>"` annotation naming the host OS/arch that caused the filter, so consumers can audit whether the filter is appropriate for their target platform.

4. **Given** a module for which the Go module proxy returned a non-200 response for the `.mod` fetch, **When** mikebom emits the component, **Then** it MUST carry a `mikebom:go-transitive-fetch-status = "<status-code-or-error-class>"` annotation so consumers can distinguish "no deps" from "we couldn't fetch the .mod file."

---

### User Story 2 - SBOM consumer sees a document-scope go-transitive-coverage signal (Priority: P2)

A compliance auditor loads a mikebom Go SBOM and wants to know the overall completeness of the transitive-edge resolution without running their own `go mod graph`. mikebom emits a document-scope `mikebom:go-transitive-coverage` annotation with values `complete` | `partial` | `unknown` (mirroring the milestone-158 graph-completeness vocabulary shape) and a companion `mikebom:go-transitive-coverage-reason` when the value is not `complete`.

**Why this priority**: Constitution Principle X (Transparency). Consumers should be able to programmatically detect coverage gaps and adjust their trust accordingly. Also enables CI gating on Go-specific coverage.

**Independent Test**: For every emitted SBOM containing at least one Go component, assert:

- `mikebom:go-transitive-coverage` annotation is present exactly once at document scope.
- The value is one of the three literal strings.
- If value is `partial` or `unknown`, `mikebom:go-transitive-coverage-reason` is present with a documented reason code.

**Acceptance Scenarios**:

1. **Given** an online Go scan where every module's transitive edges resolved successfully (all `.mod` fetches returned 200), **When** mikebom emits the SBOM, **Then** `mikebom:go-transitive-coverage = "complete"` MUST be present.

2. **Given** an online Go scan where ≥5% of modules had fetch failures (403/404/timeout), **When** mikebom emits, **Then** `mikebom:go-transitive-coverage = "partial"` MUST be present AND the reason MUST name `proxy-fetch-degraded: <N> of <M> modules unresolved`.

3. **Given** an offline Go scan (`GOPROXY=off`), **When** mikebom emits, **Then** `mikebom:go-transitive-coverage = "unknown"` MUST be present with reason `offline-mode: transitive edges from proxy fetches unavailable`.

4. **Given** a scan with mixed Go workspaces (some modules with complete edges, some without), **When** mikebom emits, **Then** the annotation reflects the WORST case across all Go modules — a single `partial` module drags the document-scope value to `partial`.

---

### User Story 3 - Non-Go scans byte-identical to pre-160 (Priority: P3)

Users scanning repos with NO Go components see byte-identical SBOM output vs. pre-160 milestones.

**Why this priority**: Regression guard. The new annotations MUST be dormant when no Go module is present. SC-002 dual-side byte-identity precedent (milestones 157/158/159).

**Independent Test**: Regenerate all 11 milestone-090 non-Go goldens with the milestone-160 code. Diff against pre-160. Zero diff bytes on non-Go goldens; the milestone-090 `golang` fixture MAY change (new annotations expected there).

**Acceptance Scenarios**:

1. **Given** the milestone-090 npm fixture (no Go components), **When** mikebom scans, **Then** the emitted CDX diff vs. pre-160 is exactly ZERO bytes.

2. **Given** the milestone-090 golang fixture, **When** mikebom scans, **Then** the emitted CDX MUST contain the new `mikebom:go-transitive-coverage` document-scope annotation AND the milestone-090 pre-160 empirical shape MAY change if the coverage-fix code changes edge counts.

### Edge Cases

- **Repo with go.mod but no dependencies**: the empty-closure case. mikebom emits `mikebom:go-transitive-coverage = "complete"` because there are no edges to miss.

- **Repo with GOPRIVATE-matching modules**: private modules explicitly opt out of proxy resolution per Go semantics. mikebom MUST NOT count these as "fetch failures" — they're intentional non-fetches. Track separately via `mikebom:go-goprivate-count = N` document annotation for auditability.

- **Repo where GOPROXY chain is `off,direct` (offline OR non-standard mirror)**: mikebom MUST detect this via existing goprivate config parsing and emit `mikebom:go-transitive-coverage = "unknown"` with reason `goproxy-off-in-chain`.

- **Module in `go mod graph` output but not in the closure of mikebom's main-module scan**: this is a legitimate discrepancy — `go mod graph` reports ALL modules in the closure regardless of build inclusion, while mikebom filters by the milestone-055 `mikebom:build-inclusion` semantics. Not counted as a coverage failure; verified via cross-referencing `mikebom:build-inclusion` on each module.

- **Concurrent fetch failure (network flake vs. permanent 404)**: single-fetch retries are already in the milestone-055 code; if a module fails after retries, it's counted as unresolved regardless of underlying cause. The FR-004 annotation captures the reason code seen by the fetch layer.

- **`go mod graph` binary not installed on the auditor's machine**: mikebom's own SC-001 verification uses the `go` binary as source-of-truth. If auditors want to verify SC-001 themselves, they need `go` installed — spec assumption.

- **Build-tag filtering across GOOS/GOARCH boundaries**: mikebom currently doesn't apply build-tag filtering at all (per empirical observation — no cross-platform direct requires are being intentionally filtered). This spec doesn't add filtering; the annotation described in US1 acceptance scenario 3 is a FUTURE hook if filtering is added.

## Requirements

### Functional Requirements

- **FR-001**: mikebom's Go transitive-edge builder (`mikebom-cli/src/scan_fs/package_db/golang/graph_resolver.rs`) MUST be instrumented to record, per-module, which resolution ladder step succeeded (`go-mod-graph`, `module-cache`, `proxy-fetch`, `go-sum-fallback`, `unresolved`). The existing `LadderCounters` at `graph_resolver.rs:229` is the starting point; per-module granularity is new.

- **FR-002**: For every emitted Go module component, mikebom MUST attach a component-scope annotation `mikebom:go-transitive-source` with value from the enum `{"go-mod-graph", "module-cache", "proxy-fetch", "go-sum-fallback", "unresolved"}` naming which ladder step produced the module's transitive-edge list.

- **FR-003**: When a module's transitive-edge resolution fell through to `unresolved`, mikebom MUST additionally attach `mikebom:go-transitive-unresolved-reason` naming the reason class (`proxy-fetch-timeout`, `proxy-fetch-not-found`, `proxy-fetch-forbidden`, `proxy-off-in-chain`, `goprivate-matched`, `module-cache-miss`, `unknown-error`).

- **FR-004**: mikebom MUST emit a document-scope annotation `mikebom:go-transitive-coverage` with value `{"complete", "partial", "unknown"}` per US2 semantics. Emitted only when at least one Go module component appears in the SBOM.

- **FR-005**: When `mikebom:go-transitive-coverage` != `complete`, mikebom MUST emit `mikebom:go-transitive-coverage-reason` with a joined reason string (mirrors milestone-158 grammar): `<code>: <detail>[; <code>: <detail>]*`. Documented codes: `proxy-fetch-degraded`, `offline-mode`, `goproxy-off-in-chain`, `go-mod-graph-degraded`, `module-cache-empty-and-no-proxy`.

- **FR-006**: mikebom MUST fix the fetch-degradation root causes discoverable during T014-T016 empirical investigation. Concrete required fixes (verified 2026-07-04 during specification via `containernetworking/plugins@v1.9.1` failure inspection):

  - **FR-006a**: When the milestone-055 proxy-fetch succeeds for module M but returns edges that the parser drops, the parser MUST be corrected. Root cause candidates identified: (a) `// indirect` deps being dropped when they should be preserved; (b) version-conflict-triggered drops (mikebom picks one version but discards the other's declared deps); (c) build-tag filtering that was accidentally too aggressive. T014-T016 empirical will confirm which applies.

  - **FR-006b**: When the milestone-055 proxy-fetch itself fails (403/404/timeout) for module M, mikebom MUST attempt the milestone-091 `go.sum`-based fallback for that module BEFORE marking it `unresolved`. Verify at empirical time that the fallback is being exercised on test-podman's 5 missing modules; if not, wire it correctly.

  - **FR-006c**: When mikebom's own scan flow uses `--offline` mode, the transitive-edge resolver MUST detect this AT-CONFIG-TIME (before starting fetches) and skip the proxy-fetch step entirely rather than failing per-fetch. The current behavior emits N × warn logs where N is the closure size; a single info log at ladder-start suffices per Q1 caution-first.

- **FR-007**: mikebom MUST maintain the milestone-055 concurrency semantics unchanged — 16-way parallel fetches per FR-008a of milestone 055. The FR-006c early-skip in offline mode preserves this on the ONLINE code path.

- **FR-008**: Standards-native precedence per Constitution Principle V. If either CDX 1.6 or SPDX 3.0.1 introduces an official "SBOM-completeness-per-ecosystem" property, mikebom MUST prefer that property. As of 2026-07-04, no such standard property exists; the `mikebom:go-transitive-*` prefixes are used.

- **FR-009**: `mikebom:go-transitive-source` and `mikebom:go-transitive-unresolved-reason` MUST be registered as new per-component parity-catalog rows (C108 + C109) with `Directionality::SymmetricEqual` — matching the milestone-127/134/158/159 pattern. `mikebom:go-transitive-coverage` and `mikebom:go-transitive-coverage-reason` MUST be registered as new document-scope parity rows (C110 + C111) — matching milestone-158's C104/C105 shape.

- **FR-010**: When per-module resolution ladder metrics are recorded, mikebom MUST emit an info-level tracing log at scan-emission time summarizing the counts: `"go transitive edges resolution summary"` with fields `total_modules`, `go_mod_graph_count`, `cache_count`, `proxy_count`, `gosum_count`, `unresolved_count`. Grep-friendly for CI-log analysis per the milestone-157/158/159 observability convention.

### Key Entities

- **Ladder step**: Enum `{GoModGraph, ModuleCache, ProxyFetch, GoSumFallback, Unresolved}` per the existing `graph_resolver.rs:229` LadderCounters shape. Each step reflects a different fallback in the milestone-055/091 resolution flow.

- **Unresolved reason class**: Enum `{ProxyFetchTimeout, ProxyFetchNotFound, ProxyFetchForbidden, ProxyOffInChain, GoPrivateMatched, ModuleCacheMiss, UnknownError}`. Used to populate the FR-003 component annotation.

- **`mikebom:go-transitive-source` (per-component)**: Component-scope annotation carrying the ladder step that produced this component's transitive-edge list. Bare-string value = enum name in kebab-case (e.g. `go-mod-graph`, `proxy-fetch`).

- **`mikebom:go-transitive-unresolved-reason` (per-component, conditional)**: Component-scope annotation present iff source == `unresolved`. Names the reason class in kebab-case.

- **`mikebom:go-transitive-coverage` (document-scope)**: Document-scope annotation with value `complete` | `partial` | `unknown`. Reflects overall Go-transitive coverage across the whole scan.

- **`mikebom:go-transitive-coverage-reason` (document-scope, conditional)**: Document-scope annotation present iff coverage != `complete`. Structured `<code>: <detail>[; ...]` joined per milestone-158 grammar.

## Success Criteria

### Measurable Outcomes

- **SC-001 (test-podman edge coverage, online mode)**: After milestone 160 ships, running `mikebom sbom scan --path test-podman --format cyclonedx-json` (online mode, `GOPROXY=https://proxy.golang.org`) and comparing against `go mod graph`-derived ground truth MUST show ≥90% edge-match (measured as `|mikebom_edges ∩ go_mod_graph_edges| / |go_mod_graph_edges|`). Pre-160 empirical baseline: **52.2%**. Target: ≥90%. This SC is empirically-locked to the concrete testbed named in issue #494.

- **SC-002 (test-podman specific missing-edge fix)**: The 5 concrete missing edges from the milestone-157 audit on `github.com/containernetworking/plugins@v1.9.1` MUST be present in the emitted SBOM: `alexflint/go-filemutex@v1.3.0`, `buger/jsonparser@v1.1.1`, `hcsshim@v0.13.0`, `coreos/go-iptables@v0.8.0`, `containerd/cgroups/v3@v3.0.3`. If any are LEGITIMATELY filtered by build-tag context (e.g. hcsshim on Linux), the filtered component MUST carry `mikebom:go-build-tag-filtered` annotation naming the host GOOS/GOARCH.

- **SC-003 (dual-side byte-identity guard, mirrors milestones 158/159)**: For every milestone-090 non-Go golden fixture (10 of 11 ecosystems: apk, bazel, cargo, cmake, deb, gem, maven, npm, pip, rpm), the emitted CDX / SPDX 2.3 / SPDX 3 SBOMs MUST be byte-identical to pre-160. The `golang` fixture is exempt — it will change to add `mikebom:go-transitive-*` annotations. Zero diff bytes on the 10 non-Go ecosystems × 3 formats = 30 goldens.

- **SC-004 (per-component annotation universal presence)**: 100% of emitted Go components (`purl.starts_with("pkg:golang/")`) MUST carry a `mikebom:go-transitive-source` annotation with a valid enum value. Zero components without.

- **SC-005 (document-scope annotation universal presence)**: 100% of emitted SBOMs containing at least one Go component MUST carry the `mikebom:go-transitive-coverage` document-scope annotation with one of the 3 valid values.

- **SC-006 (offline mode signals unknown)**: Running `mikebom --offline sbom scan --path test-podman` MUST emit `mikebom:go-transitive-coverage = "unknown"` with reason `offline-mode: transitive edges from proxy fetches unavailable`.

- **SC-007 (pre-PR gate)**: `cargo +stable clippy --workspace --all-targets -- -D warnings` and `cargo +stable test --workspace` MUST both pass with zero errors before the PR is opened. The mandatory `./scripts/pre-pr.sh` gate must be green.

- **SC-008 (unit test coverage)**: The new annotation-emission code paths + FR-006 fetch-degradation fixes MUST have at least 10 unit tests covering: (a) online scan with all fetches succeeding → `complete`; (b) online scan with 5% fetch failures → `partial` with `proxy-fetch-degraded`; (c) offline scan → `unknown` with `offline-mode`; (d) GOPROXY chain with `off` in the middle → `unknown` with `goproxy-off-in-chain`; (e) FR-006a parser-drop fix for the containernetworking/plugins@v1.9.1 shape; (f) FR-006b go.sum fallback exercised when proxy fetches fail; (g) FR-006c early-skip in offline mode; (h) per-component `mikebom:go-transitive-source` on a `go-mod-graph`-resolved module; (i) per-component `mikebom:go-transitive-source` on a `proxy-fetch`-resolved module; (j) per-component `mikebom:go-transitive-source = "unresolved"` on a fetch-failed module.

- **SC-009 (integration test)**: A new integration test at `mikebom-cli/tests/go_transitive_coverage.rs` MUST synthesize a Go workspace with 3+ modules (some fetch-succeed, some fetch-fail via mock proxy), scan it via the release binary, and assert per-component + document-scope annotations are present with the correct values.

- **SC-010 (CHANGELOG entry)**: `CHANGELOG.md` MUST document the coverage-fix + FR-002/003/004/005 annotation vocabulary + the SC-001 empirical numbers + a consumer jq recipe for gating on `mikebom:go-transitive-coverage`.

- **SC-011 (parity catalog registration)**: The 4 new annotations (C108/C109 per-component + C110/C111 document-scope) MUST have parity-catalog entries with `Directionality::SymmetricEqual`. Milestone-071 parity check MUST pass symmetrically across CDX / SPDX 2.3 / SPDX 3.

- **SC-012 (issue #494 closure)**: Issue #494 MUST reference this milestone (`closes #494` in the impl commit message) and the milestone MUST demonstrably resolve the reported symptom (52.2% → ≥90% edge coverage on test-podman).

## Assumptions

- **Ground truth = `go mod graph`**: The `go mod graph` command output on the target repo is the authoritative source for what edges "should" be in the SBOM. SC-001 measures against this. Consumers running SC-001 verification themselves need the `go` binary installed.

- **Online mode is the primary target**: The vast majority of Go SBOM users run mikebom in online mode where `GOPROXY` reaches a working mirror. SC-001's ≥90% target applies to online mode. Offline mode gets a truthful `unknown` signal per SC-006 but is NOT the primary fix target.

- **`test-podman` is the empirical benchmark**: SC-001/SC-002 numbers are pinned to this repo. The 5 specific missing edges from milestone-157's audit are the load-bearing verification.

- **Build-tag filtering is out of scope**: Milestone 160 does NOT introduce build-tag-based filtering. If mikebom's fetch-degradation fixes surface direct requires that ARE platform-specific (like `hcsshim` on Linux hosts), they're emitted anyway. A future milestone MAY add filtering + the `mikebom:go-build-tag-filtered` annotation described in US1 acceptance scenario 3.

- **Fetch retries stay at milestone-055 defaults**: 3-retry cap, exponential backoff. No change.

- **Concurrency stays at 16-way per milestone-055 FR-008a**: No change.

- **No new Cargo dependencies**: Following the milestone-157/158/159 precedent, this work uses existing crates only (`reqwest` blocking, `tracing`, `serde`, `anyhow`, `thiserror`).

- **SC-001 target is empirically-adjustable**: If T014-T016 investigation reveals the FR-006 root causes are more complex than anticipated, SC-001 may be revised inline per the milestone-156/157/158/159 empirical-revision pattern. The floor is ≥52.2% + material improvement; the aspirational is ≥90%.

- **milestone-090 golang fixture will change**: The pre-160 golang golden emits 12 components; post-160 it will have the same 12 components + new `mikebom:go-transitive-source` component annotations + `mikebom:go-transitive-coverage` document annotation. Golden regeneration is expected.

## Out of Scope

- **The Go workspace-mode false-edge fix (issue #495)** — separate milestone. That's about EXTRA edges from workspace-root leaking into leaf modules; this milestone is about MISSING edges.

- **The Ruby built-in gem edge fix (issue #496)** — separate milestone.

- **The npm phantom empty-version edges fix (issue #498)** — separate milestone.

- **Build-tag-aware filtering** — introducing GOOS/GOARCH-based selective emission is a bigger design conversation. Milestone 160 emits everything the parser sees; a future milestone MAY add filtering + `mikebom:go-build-tag-filtered` annotation.

- **Alternative proxy backends** (Athens, JFrog, local mirrors) — the existing `GOPROXY` chain semantics handle these; no new detection logic in this milestone.

- **`vendor/` directory scanning as a transitive-edge source** — some Go repos vendor their dependencies. This is a DIFFERENT resolution mechanism (module cache is not the same as vendored source) and out of scope. If empirical investigation reveals vendored repos are affected, that's a follow-on milestone.

- **Cross-scan aggregation** of transitive-coverage annotations across multiple Go repos — mikebom-160 emits per-scan; no across-scan rollup.
