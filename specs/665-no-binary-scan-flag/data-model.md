# Data Model: `--no-binary-scan=<MODE>`

## Entities

### `BinaryScanMode` enum

Rust enum representing the requested suppression scope. v1 recognizes one variant; the type is designed for future extension.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum BinaryScanMode {
    /// Skip the `go_binary` reader — no BuildInfo content probing.
    /// Emitted PURL suppression for statically-linked Go binaries;
    /// components claimed via OS-package readers (dpkg / apk / rpm /
    /// pip RECORD) remain emitted from those sources.
    #[clap(name = "go")]
    Go,
    // Future variants:
    //   /// Skip go_binary + m096 ELF section + m099 symbols + m104
    //   /// binary-role classification. Broadest suppression.
    //   #[clap(name = "all")] All,
    //   /// Skip m096 ELF `.dep-v0` section reader only.
    //   #[clap(name = "elf")] Elf,
    //   /// Skip m099 symbol fingerprinting only.
    //   #[clap(name = "symbols")] Symbols,
}

impl BinaryScanMode {
    /// The canonical string used in `waybill:binary-scan-suppressed`
    /// annotation values. Matches the `#[clap(name = ...)]` attribute
    /// so operators see the same value in `--help` output, error
    /// messages, and emitted SBOMs.
    pub fn as_annotation_value(&self) -> &'static str {
        match self {
            Self::Go => "go",
        }
    }
}
```

**Attributes**:
- Serializable via the `#[clap(name = ...)]` attribute — the string value is what appears in `--help`, in error messages for FR-009's unrecognized-mode path, and in the emitted SBOM annotation (FR-004). Keeping these three surfaces aligned to a single source of truth prevents drift.
- `Copy` + `Clone` because the enum is threaded through function signatures where `&BinaryScanMode` would be equally valid but ergonomically noisier for a one-variant-currently type.

**Location**: `waybill-cli/src/cli/scan_cmd.rs` (colocated with the CLI-arg-struct that owns the field).

**Threading**: the flag's parsed value (`Option<BinaryScanMode>`) threads through:

```
ScanArgs (clap-derive struct)
  ↓
scan_fs::mod::scan_path(..., no_binary_scan: Option<BinaryScanMode>, ...)
  ↓
scan_fs::package_db::mod::read_all(..., no_binary_scan: Option<BinaryScanMode>, ...)
  ↓
scan_fs::package_db::mod::run_shared_walker_pilot(..., no_binary_scan: Option<BinaryScanMode>, ...)
```

Inside `run_shared_walker_pilot`, one branch gates registration:

```rust
if !matches!(no_binary_scan, Some(BinaryScanMode::Go)) {
    if let Some(r) = register("go_binary", go_binary::registration()) {
        builder = builder.register(r);
    }
}
```

For the emitter side, `no_binary_scan` threads separately to the document-scope-annotation assembly point via `ScanResult` or an equivalent handoff channel.

---

### `waybill:binary-scan-suppressed` annotation (m071 parity C-row)

Per-scan document-scope annotation. Emitted iff `--no-binary-scan=<mode>` was set (via CLI or env var).

| Field | Type | Value |
|---|---|---|
| **Name** | string | `waybill:binary-scan-suppressed` |
| **Value** | string | The mode name (v1: `"go"`; future: `"all"`, `"elf"`, `"symbols"`, ...). Matches `BinaryScanMode::as_annotation_value()`. |
| **Scope** | document-scope (not per-component) | Analogous to C112 `waybill:workspace-mode`, C120 `waybill:workspaces-detected`, C136 `waybill:go-toolchain-detected`. |
| **Directionality** (m071) | `SymmetricEqual` | Present with identical value in CDX / SPDX 2.3 / SPDX 3 iff the flag was set. |
| **Presence rule** | Optional | Absent when the flag is unset — byte-identity preservation for the default path (FR-003). |

**Emission points per format**:

- **CDX 1.6**: `metadata.properties[]` entry `{name: "waybill:binary-scan-suppressed", value: "go"}`. Emit in `waybill-cli/src/generate/cyclonedx/document.rs` alongside existing `waybill:workspace-mode` etc.
- **SPDX 2.3**: `annotations[]` on `SPDXRef-DOCUMENT` with `annotator: "Tool: waybill-<version>"`, `annotationType: "OTHER"`, `comment: "waybill:binary-scan-suppressed=go"`. Emit in `waybill-cli/src/generate/spdx/document.rs`.
- **SPDX 3.0.1**: `Annotation` element with `subject: <SpdxDocument IRI>`, `statement: "waybill:binary-scan-suppressed=go"`. Emit in `waybill-cli/src/generate/spdx3/annotations.rs`.

**Parity extractor**: new entry in `waybill-cli/src/parity/extractors/mod.rs::EXTRACTORS` mapping this C-row to a `SymmetricEqual` extractor that reads the annotation from each format's emitted document. Enforces per project memory `feedback_sbom_format_mapping_extractor_gate`: every row in `docs/reference/sbom-format-mapping.md` must have a matching extractor.

## State Transitions

### Feature state

Not applicable — the flag has two states (set / unset) and one axis of variation (the mode enum). No state machine.

### Component set state (per scan)

```
Pre-m664:     [components emitted by all N readers]
Post-m664:    [components emitted by all N readers, single-pass walker]
Post-m665 default (flag unset):
              [byte-identical to post-m664; FR-003]
Post-m665 with --no-binary-scan=go:
              [post-m664 set] − [components emitted by go_binary reader only]
              + [waybill:binary-scan-suppressed=go annotation on document]
```

The suppression is a strict subset operation on the emitted component set (never adds components; only removes them). No m191 reconciler dependencies broken because the go_binary reader emits only its own components — never modifies emissions by other readers in place.

## Validation Rules

- **V1 — Mode enum exhaustiveness**: every variant in `BinaryScanMode` MUST have a matching arm in `as_annotation_value()`. Enforced at compile time via Rust's exhaustive-match check.
- **V2 — Annotation-mode consistency**: the annotation value emitted per FR-004 MUST equal `BinaryScanMode::as_annotation_value()` for the active mode. Enforced via a shared-constant call at the emission site (no ad-hoc string literals).
- **V3 — Parity-catalog gate**: adding this C-row to `docs/reference/sbom-format-mapping.md` MUST land together with the extractor entry in `parity/extractors/mod.rs::EXTRACTORS`. Enforced by the existing `every_catalog_row_has_an_extractor` + `holistic_parity` tests.
- **V4 — Env-var / CLI precedence**: CLI flag wins over env var (per R2). Threaded in the CLI arg-parsing entry point (`scan_cmd.rs`) via:
  ```rust
  let mode = args.no_binary_scan
      .or_else(|| std::env::var("WAYBILL_NO_BINARY_SCAN")
          .ok()
          .and_then(|s| BinaryScanMode::from_str(&s, /* ignore_case = */ true).ok()));
  ```
- **V5 — FR-003 byte-identity**: workspace test suite MUST remain at 5183+N passed / 0 failed after this feature ships, where N is the count of new tests added by this feature. Enforced by CI (walker-audit gate is unchanged; this is a normal `cargo test` verification).
- **V6 — EnvGuard test isolation**: any integration test that mutates `WAYBILL_NO_BINARY_SCAN` env var MUST acquire the `EnvGuard` first per project memory `reference_podman_test_flake`. Documented as a test-authoring rule in `contracts/cli-flag.md`.
