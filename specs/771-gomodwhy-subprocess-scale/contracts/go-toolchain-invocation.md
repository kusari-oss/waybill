# Contract — Go toolchain invocation surface

**Feature**: 771-gomodwhy-subprocess-scale
**Status**: Complete
**Date**: 2026-09-04
**Supersedes**: `specs/112-go-build-inclusion/contracts/go-toolchain-invocation.md` (partial — this milestone changes _how_ the invocation is orchestrated, not _what_ is invoked)

The only external interface this milestone touches is the `go` binary. This contract pins the exact invocation shape post-milestone so future maintainers (and the m669 benchmark harness) can verify byte-identity vs pre-milestone at the argv/env level for equivalent workloads.

---

## Contract 1 — `go list all` preflight

**Purpose**: Detect module-resolution failures before spending budget on `go mod why -m` (which silently emits false-not-needed verdicts on unresolvable inputs — verified empirically on Go 1.26.2 per the m112 comment at `mod_why.rs:12-17`).

### Pre-milestone (per-workspace, N times per scan for N workspaces)

```sh
cd <workspace_dir>
GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local go list all
```

### Post-milestone US3 (per go.work scope, 1 time per scope)

```sh
cd <go.work parent dir>            # NOT any member subdir — per spec Clarification 2026-09-04 Q1
GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local go list all
```

### Post-milestone loose main-modules (unchanged)

```sh
cd <workspace_dir>                 # not a go.work member; own preflight
GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local go list all
```

### Env-var pinning (unchanged from m112 / m231)

Applied by `apply_offline_env` at `mod_why.rs:218` whenever `--offline` / `WAYBILL_OFFLINE` is set:
- `GOPROXY=off` — no network reads
- `GOFLAGS=-mod=mod` — read go.mod-declared deps (not `-mod=vendor` — that path is exercised by `-vendor` in Contract 2)
- `GOTOOLCHAIN=local` — no toolchain autodetection subprocess

When workspace mode is Active/Explicit AND `--offline` is set, `apply_offline_env` skips `-mod=mod` (workspace mode has its own module resolution semantics that `-mod=mod` conflicts with). Unchanged from m231.

**Contract observable to operators**: identical argv + env for equivalent workloads. Only the working directory changes for shared-preflight members (per spec Clarification 2026-09-04 Q1).

---

## Contract 2 — `go mod why -m -vendor <paths...>` classification

**Purpose**: Classify each queried module as `ProdNeeded` / `TestOnly` / `NotNeeded` / `Unresolved` for a specific main-module's build graph.

### Pre-milestone (chunked at CHUNK_SIZE=20)

```sh
cd <workspace_dir>
GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local \
  go mod why -m -vendor <path_1> <path_2> ... <path_20>
# Repeat ceil(N/20) times for N modules per workspace.
```

### Post-milestone US1 + R2 (chunked at CHUNK_SIZE=500 with argv guard)

```sh
cd <workspace_dir>
GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local \
  go mod why -m -vendor <path_1> <path_2> ... <path_500>
# Repeat ceil(N/500) times for N modules per workspace.
# If projected argv > 96 KiB, chunk is bisected and both halves invoked
# sequentially. Bisection recurses until argv fits.
```

**Contract observable to operators**: identical argv shape (`go mod why -m -vendor <paths...>`) and identical env vars. Only the batch cardinality per invocation changes.

**Output-parsing contract**: unchanged. `parse_go_mod_why` at `mod_why.rs:440` continues to handle multi-section output (already covered by the `multi_section_output` test at `mod_why.rs:548+`).

---

## Contract 3 — Working-directory-of-invocation semantics

Both Contract 1 and Contract 2 rely on the working directory for module-graph resolution. Pinning the exact semantic here so a reviewer can grep for compliance:

| Invocation | Pre-milestone cwd | Post-milestone cwd | Trigger |
|---|---|---|---|
| `go list all` (loose main-module, no `go.work`) | main-module dir | main-module dir | unchanged |
| `go list all` (main-module INSIDE a `go.work` scope) | main-module dir | **`go.work` parent dir** | US3 (shared preflight) |
| `go mod why -m -vendor` (any main-module) | main-module dir | main-module dir | unchanged |

The rationale for keeping `go mod why -m -vendor` per-member (not per-scope) is that its verdicts _are_ per-member: `go mod why -m foo` from member A returns why A imports foo, which is semantically distinct from why member B imports foo. Only the preflight (which asks a resolvability question, not a why-question) can be shared.

---

## Contract 4 — Subprocess-concurrency shape

**Pre-milestone**: All `go` invocations are serial. At most one `go` process in flight at any time.

**Post-milestone US2**: Up to `min(N_workspaces, std::thread::available_parallelism())` `go` processes in flight concurrently, one per worker thread. Each worker holds a `run_bounded` call which is synchronous (spawn `go` + wait on `mpsc::recv_timeout` for output or timeout). No worker spawns a second `go` process while its first is running.

**Contract observable to operators**:
- Concurrent `go` invocations are permitted. The Go toolchain's module cache is designed for concurrent readers (spec Assumption 6).
- On a 2-CPU CI runner, at most 2 `go` processes are in flight. On an 8-core dev machine, up to 8. On a 1-CPU embedded builder, US2 is effectively serial (worker count = 1).
- Log lines from concurrent workspaces MAY interleave; each carries the workspace's absolute path in the structured `main_module` field so operators can grep-reconstruct per-workspace timelines (FR-005).

---

## Contract 5 — Failure-mode taxonomy (unchanged from m112)

Every failure mode remains named by an existing `SkipReason` or `Invocation` variant. Post-milestone changes route more paths through the same variants; no new variants are introduced.

| Condition | Variant | Post-milestone routing |
|---|---|---|
| `go` binary not on `$PATH` | Existing toolchain-missing skip | Classifier bails at entry; no workers spawned. |
| `go list all` non-zero exit / spawn fail / timeout (loose main-module) | `SkipReason::UnresolvablePackages` | Per-workspace fallback path (unchanged from m112). |
| `go list all` non-zero exit / spawn fail / timeout (`go.work` scope shared preflight) | `SkipReason::UnresolvablePackages` | **Every member of the scope** marked with this variant (FR-007). |
| `go mod why -m` non-zero exit / spawn fail | `SpawnFailed` → chunk's modules → `GoModWhyVerdict::Unresolved` | Per-chunk fallback, unchanged. |
| `go mod why -m` timeout (chunk budget exhausted) | `SkipReason::BudgetExhausted` | Reported by the worker whose chunk timed out. Sibling workers who complete under-budget still emit verdicts. |
| Shared 60s wall-clock budget exhausted | `SkipReason::BudgetExhausted` | Any worker that finds `budget.remaining() == None` exits its loop; unprocessed workspaces receive `mark_unresolved`. |

**Contract observable to operators**: identical WARN/INFO log messages except for FR-005 (per-workspace `main_module` field on concurrent log lines is now the norm).

---

## Contract 6 — Backwards compatibility with m112 / m231 tests

Every existing `mod_why.rs::tests::*` unit test must continue to pass unchanged:
- `parses_prod_needed_chain`, `parses_test_only_chain`, `parses_not_needed_plain_phrasing`, `parses_not_needed_vendor_phrasing`, `empty_section_is_unresolved`, `unknown_parenthesized_diagnostic_is_unresolved`, `multi_section_output`, `garbage_before_first_header_ignored`, `empty_output_yields_no_verdicts`, `bare_hash_header_is_skipped` — cover `parse_go_mod_why` output-parsing. Untouched.

Every existing scan-level integration test that exercises the golang classifier must produce byte-identical CDX/SPDX output modulo version-string cascades (SC-003):
- `waybill-cli/tests/scan_go*.rs` — verifies FR-012.
- `waybill-cli/tests/golang_transitive*.rs` — verifies verdict-classification stability.

New tests in `waybill-cli/tests/mod_why_scaling.rs` cover the acceptance scenarios enumerated in spec.md US1/US2/US3.
