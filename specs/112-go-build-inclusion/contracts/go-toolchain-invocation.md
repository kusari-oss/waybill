# Contract: Go Toolchain Invocation (`go mod why`)

**Feature**: 112-go-build-inclusion

## Command

```
go mod why -m -vendor <module-path> [<module-path> ...]
```

- `cwd`: the main module's directory (the directory containing the
  `go.mod` that produced the `mikebom:component-role: main-module`
  entry). Multi-module trees: one invocation series per main module.
- Chunking: at most **20 module paths per invocation**
  (cyclonedx-gomod `FilterModules` parity).
- The `-m` flag queries modules (not packages); `-vendor` excludes
  tests of dependencies (NOT related to `vendor/` directories) — this
  is what makes dependency-declared test requirements come back as
  "does not need".
- The main-module entry itself is never queried.

## Environment

| Mode | Child environment |
|---|---|
| Normal | Inherit parent env unchanged (same posture as the existing `go mod graph` shell-out). |
| `--offline` / `MIKEBOM_OFFLINE=1` | Inherit + `GOPROXY=off`, `GOFLAGS=-mod=mod`, `GOTOOLCHAIN=local`. Toolchain answers from local cache or fails fast; failure degrades (FR-012/FR-007). `GOTOOLCHAIN=local` also blocks go.mod `toolchain`-directive downloads. |

## Reliability preflight (gates every main-module analysis)

`go mod why` does NOT fail when module resolution fails mid-query: it
exits 0 and wrongly reports modules as not needed — including
directly-imported ones. Verified empirically on go 1.26.2: cold module
cache + `GOPROXY=off` AND an unreachable proxy both reproduce silent
false not-needed verdicts, and `vendor/` does not prevent it (the
module graph still requires go.mod data from the cache/proxy).

Therefore, before any `go mod why` chunk runs for a main module:

```
go list all
```

(same cwd, same env pinning, counts against the shared budget)

- Exit 0 → package loading is resolvable; proceed with chunks.
- Non-zero exit or timeout → skip package-level analysis for this main
  module entirely; its modules fall back to FR-001 unknown markers;
  warn log with skip reason `unresolvable-packages`.

`NotNeeded` verdicts MUST never be accepted from a main module whose
preflight did not pass.

## Time budget

- **60 seconds total per scan** across ALL invocations (preflight + all
  chunks, all main modules) — clarification 2026-06-11.
- Test-only override: `MIKEBOM_GO_MOD_WHY_BUDGET_MS` (integer
  milliseconds) replaces the 60s budget when set. Exists solely so the
  budget-exhaustion integration test does not burn 60s of wall clock;
  NOT part of the user-facing docs surface.
- Each chunk runs with timeout `budget − elapsed`, using the existing
  spawn-thread + `mpsc::recv_timeout` pattern from
  `golang/go_mod_graph.rs:81–158`.
- Budget exhaustion: abandon remaining chunks; verdicts already obtained
  are kept; remaining modules → `Unresolved` (eligible for the
  unknown-marker pass); warn log with skip reason `budget-exhausted`.

## Output parsing

Output is a sequence of sections, each headed by `# <module-path>`:

| Section body | Verdict |
|---|---|
| `(main module does not need to vendor module <path>)` — the `-vendor` flag changes the phrasing (without it: `does not need module`); parsers MUST match the `(main module does not need` prefix, covering both forms (verified empirically, go 1.26.2) | `NotNeeded` |
| Import chain (one package per line) containing a node with `.test` suffix | `TestOnly` |
| Non-empty import chain, no `.test` node | `ProdNeeded` |
| Empty body / unparseable section | `Unresolved` (per-module) |

Module ↔ entry matching is by module path against
`PackageDbEntry.name` (golang, source-tier). Sections for unknown
modules and per-module errors yield `Unresolved`, never a scan error.

## Failure classes (all degrade per FR-007 — scan never fails)

| Condition | Behavior | Skip reason (warn log) |
|---|---|---|
| No `go` on PATH | Skip analysis entirely | `no-toolchain` |
| `--no-go-mod-why` / env opt-out | Skip, info-level (operator intent) | `disabled` |
| Spawn failure / non-zero exit on a chunk | Chunk's modules → `Unresolved`; continue remaining chunks within budget | `subprocess-error` (per chunk, warn) |
| Timeout (budget exhausted) | As above, abandon remainder | `budget-exhausted` |
| go.mod requires newer Go (toolchain-directive error under `GOTOOLCHAIN=local`) | Non-zero exit → `subprocess-error` | `subprocess-error` |
| Reliability preflight (`go list all`) fails — cold cache offline, broken proxy, non-buildable tree, not-a-module dir | Skip analysis for this main module; NO verdicts collected (prevents silent false not-needed) | `unresolvable-packages` |

## Observability (FR-013)

One info-level summary line per scan:

```
go-mod-why classification: analyzed=N prod=N test=N not_needed=N unresolved=N unknown_marked=N skipped=<reason|none> elapsed_ms=N
```

Warn-level lines accompany every degrade (skip reason + detail), e.g.:

```
WARN go-mod-why analysis skipped (no-toolchain): 'go' not found on PATH; build-inclusion falls back to unknown markers
```
