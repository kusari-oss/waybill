# Contract: `--no-binary-scan=<MODE>` CLI flag surface

## Interface

### CLI flag

```
--no-binary-scan=<MODE>
```

- **Subcommand**: `waybill sbom scan` (per Q2 clarification — not `waybill trace`).
- **Value**: REQUIRED enum. v1 accepts `go`. Case-insensitive at parse time; canonicalized to lowercase in emitted SBOM annotations.
- **Absent from CLI**: no suppression (default; byte-identical to pre-feature behavior per FR-003).
- **Bare `--no-binary-scan` (no `=<MODE>`)**: `clap` emits a parse error naming the flag needs a value. Exit code 2.
- **Unrecognized value (e.g., `--no-binary-scan=xyz`)**: `clap`'s `value_enum`-derived error names the recognized values. Exit code 2. FR-009 + SC-007.

### Environment variable

```
WAYBILL_NO_BINARY_SCAN=<MODE>
```

- Same accepted-mode set as the CLI flag.
- Read ONLY when the CLI arg is absent (CLI wins per R2).
- Empty string treated as absent.
- Unrecognized value: fail fast with an operator-visible error (same message shape as the CLI-flag path).

### `--help` output contract (FR-006)

```
--no-binary-scan <MODE>  Skip specified binary-scanning reader(s). Trades
                         binary-content-based module attribution for scan
                         speed. Possible values: go
                         [env: WAYBILL_NO_BINARY_SCAN=]
```

## Behavioral contract

### C1: Registration gate

When the effective mode (CLI ∪ env-var per V4) is `Some(BinaryScanMode::Go)`, the caller of `run_shared_walker_pilot` MUST NOT register `go_binary::registration()` in the builder chain. All other reader registrations proceed unchanged.

### C2: Emission gate

When the effective mode is `Some(_)`, the SBOM emitter MUST include a document-scope `waybill:binary-scan-suppressed=<mode>` annotation in EVERY output format (CDX / SPDX 2.3 / SPDX 3). Value equals the active mode's canonical string (`BinaryScanMode::as_annotation_value()`).

### C3: FR-009 diagnostic

At scan start, when the effective mode is `Some(_)`, emit ONE INFO-level `tracing` log line:

```
INFO waybill::cli::scan_cmd: --no-binary-scan={mode} — skipping <human-list-of-affected-readers>
```

For v1 mode `go`: "skipping go_binary reader (statically-linked Go BuildInfo probing)".

### C4: Byte-identity default path

When the effective mode is `None`, the emitter's document-scope annotation set MUST NOT include `waybill:binary-scan-suppressed` (not even with an empty value). FR-003.

### C5: Parity extractor

A new entry MUST be added to `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` for the `waybill:binary-scan-suppressed` C-row, keyed to the C-row identifier chosen at implementation time. Extractor kind: `SymmetricEqual`. Value shape: string.

### C6: Env-var / CLI precedence

Given `cli = --no-binary-scan=<x>`, `env = WAYBILL_NO_BINARY_SCAN=<y>`:

| CLI | Env | Effective mode |
|---|---|---|
| absent | absent | `None` |
| absent | valid `<y>` | `Some(<y>)` |
| absent | invalid `<y>` | ERROR: exit 2, name recognized modes |
| valid `<x>` | absent | `Some(<x>)` |
| valid `<x>` | valid `<y>` | `Some(<x>)` (CLI wins per R2) |
| valid `<x>` | invalid `<y>` | ERROR (env-var invalid — fail fast per Constitution III) |
| invalid `<x>` | any | ERROR: exit 2, name recognized modes |

## Test-authoring rules

### T1: Env-var test isolation (from V6)

Any integration test that mutates `WAYBILL_NO_BINARY_SCAN` MUST acquire the `crate::testing::EnvGuard` for the duration of the mutation. Per project memory `reference_podman_test_flake` + `feedback_release_bump_regen_goldens`.

```rust
#[test]
fn test_env_var_precedence() {
    let _guard = crate::testing::EnvGuard::acquire();
    std::env::set_var("WAYBILL_NO_BINARY_SCAN", "go");
    // ... run test ...
    std::env::remove_var("WAYBILL_NO_BINARY_SCAN");
}
```

### T2: Fixture path convention (from R5)

Tests requiring a Go binary fixture MUST access it via the m090 `fixture_path()` helper:

```rust
let bin_path = fixture_path("no_binary_scan/gobin_with_buildinfo");
```

The fixture is checked in to `kusari-oss/waybill-test-fixtures` at a pinned SHA (managed via the sibling repo's tag, `Cargo.toml [package.metadata.fixtures]` pin).

### T3: Two-fixture parity for SC-006

```rust
#[test]
fn suppression_annotation_present_iff_flag() {
    let sbom_with = run_scan_with_flag("go");
    let sbom_without = run_scan_without_flag();
    let anno_with = extract_annotation(&sbom_with, "waybill:binary-scan-suppressed");
    let anno_without = extract_annotation(&sbom_without, "waybill:binary-scan-suppressed");
    assert_eq!(anno_with.as_deref(), Some("go"));
    assert_eq!(anno_without, None);
}
```

## Non-contracts

- **`--verify` mode interaction**: verify runs against an existing SBOM; scan-time flags don't affect it. Explicitly out of scope per spec §Out of Scope.
- **`waybill trace` subcommand**: per Q2, not addressed by v1.
- **Silently changing default to enabled**: not a contract; behavior change would require its own spec.
- **Repeatable flag** (`--no-binary-scan=go --no-binary-scan=elf`): rejected per R1; use future `all` mode instead.
