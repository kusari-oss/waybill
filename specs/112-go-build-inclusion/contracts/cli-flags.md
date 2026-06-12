# CLI Contract: `--no-go-mod-why`

**Feature**: 112-go-build-inclusion

## Flag definition

```
--no-go-mod-why
    Disable Go package-level build-graph classification. By default,
    when a `go` toolchain is found on PATH during a Go source scan,
    mikebom runs `go mod why -m -vendor` (60s total budget) to mark
    modules the production build does not need as scope-excluded and
    test-only modules with test lifecycle scope. With this flag (or
    when no toolchain is available), unconfirmed fallback-discovered
    modules carry `mikebom:build-inclusion: unknown` instead.

    Also settable via MIKEBOM_NO_GO_MOD_WHY=1.
```

- Boolean flag, no value. Repeating it is harmless.
- Env var `MIKEBOM_NO_GO_MOD_WHY` (any non-empty value ≠ `0`) is
  equivalent; flag OR env var disables (same dual pattern as
  `MIKEBOM_OFFLINE`).

## Default behavior matrix

| Toolchain on PATH | `--no-go-mod-why` | `--offline` | Behavior |
|---|---|---|---|
| yes | no | no | Part C runs (inherit env), Part B marks leftovers |
| yes | no | yes | Part C runs with `GOPROXY=off GOFLAGS=-mod=mod GOTOOLCHAIN=local`; failure → degrade |
| yes | yes | — | Part C skipped (info log, reason `disabled`); Part B only |
| no | — | — | Part C skipped (warn log, reason `no-toolchain`); Part B only |

## Interaction with other flags

| Flag | Interaction |
|---|---|
| `--include-dev` / `--exclude-scope` | Affects toolchain TEST-tagged modules exactly like all other test-scoped deps (pre-existing semantics). Has NO effect on `not-needed` / `unknown` components — those always remain in output. |
| `--offline` | Pins child env per the invocation contract; never enables network. |
| `--image` | No effect — Part B/C apply to source-tier Go scans only; image/binary paths unchanged (BuildInfo remains authoritative, FR-010). |

## Exit-status contract

Package-level analysis can NEVER change the scan exit status (FR-007 /
SC-003): toolchain absence, subprocess failure, and budget exhaustion
all degrade with logs and exit 0 (assuming the scan itself succeeds).

## Log contract (FR-013)

- Info summary (always emitted on Go source scans):
  `go-mod-why classification: analyzed=N prod=N test=N not_needed=N unresolved=N unknown_marked=N skipped=<reason|none> elapsed_ms=N`
- Warn on every degrade with skip reason
  (`no-toolchain` | `subprocess-error` | `budget-exhausted` |
  `unresolvable-packages`);
  info on operator-intent skip (`disabled`).

## Versioning + back-compat

- Purely additive; no existing flag's semantics change.
- With `--no-go-mod-why` AND no fallback-discovered components, output
  is byte-identical to pre-feature output (SC-004 envelope).
