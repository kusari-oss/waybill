# Research: `--no-binary-scan=<MODE>` — Phase 0 outputs

## R1: `clap` value-enum shape

**Decision**: `#[arg(long, value_enum, value_name = "MODE")] no_binary_scan: Option<BinaryScanMode>`, where `BinaryScanMode` derives `ValueEnum`. Absent → `None` (default; nothing suppressed). Present with value → `Some(mode)`.

**Rationale**:
- Matches existing waybill flag idiom (e.g., `--split=<MODE>` from m219, `--scan-mode=<MODE>` from CLI parsing).
- `ValueEnum` derive emits per-variant help text and automatic parse-error messages naming valid values — satisfies FR-009 (unrecognized mode fails fast with recognized-mode list) for free.
- `Option<>` cleanly distinguishes "flag absent" from "flag present with valid value"; internal branching is explicit.
- Bare `--no-binary-scan` (no value) is rejected by clap automatically → FR-001's "REQUIRED value" requirement met without custom handling.

**Alternatives considered**:
- `Vec<BinaryScanMode>` (repeatable). Rejected — encourages `--no-binary-scan=go --no-binary-scan=elf` which duplicates future `--no-binary-scan=all` semantic. Also adds set-ordering ambiguity.
- `String` with hand-rolled parse. Rejected — reinvents clap's `value_enum` + weakens help output.
- `BinaryScanMode::None` sentinel + `#[arg(long, default_value = "none")]`. Rejected — makes "flag absent" and "flag=none" indistinguishable in downstream code; complicates FR-004 (annotation emission gate) logic.

## R2: env-var precedence

**Decision**: **CLI flag wins over env-var** when both are set with different values. Waybill's existing convention (per `WAYBILL_INCLUDE_VENDORED` inspection): CLI flag reads `std::env::var(...)` as a fallback ONLY when the corresponding CLI arg is `None`. Same pattern here.

**Rationale**:
- Matches every existing waybill CLI-flag / env-var pair (`WAYBILL_INCLUDE_VENDORED`, `WAYBILL_CMAKE_THIRD_PARTY_RECURSIVE`, `WAYBILL_MAX_RPM_BYTES`, `WAYBILL_RPM_DISTRO`, etc.).
- CLI-explicit-wins is the least-surprising CI ergonomic — CI can set the env var as a default across all scans; per-scan `--no-binary-scan=<other>` overrides it.
- No new precedence rule to document — operators already know this pattern from other flags.

**Alternatives considered**:
- Env-var wins. Rejected — invert of every existing waybill flag; would surprise operators.
- Error if both set. Rejected — too aggressive; CI would need to unset the env var per scan.

**Implementation note**: since v1 only recognizes `go`, the precedence question is moot within v1 (both `--no-binary-scan=go` and `WAYBILL_NO_BINARY_SCAN=go` produce the same behavior). Precedent-setting only.

## R3: m071 parity-catalog row

**Decision**: **Add one new C-row** (next in sequence; check `docs/reference/sbom-format-mapping.md` at implementation time) with:
- **Annotation name**: `waybill:binary-scan-suppressed`.
- **Value shape**: string (the mode; v1 only `"go"`).
- **Directionality**: `SymmetricEqual` (present in all three formats iff the flag was set; absent otherwise).
- **Scope**: document-scope (not per-component).

**Rationale**:
- Per project memory `feedback_sbom_format_mapping_extractor_gate`: every C-row in `docs/reference/sbom-format-mapping.md` MUST have a matching entry in `parity/extractors/mod.rs::EXTRACTORS`. Adding the row without the extractor makes the `every_catalog_row_has_an_extractor` + `holistic_parity` tests fail. Task list must include both.
- Document-scope is correct because the suppression is a scan-wide property, not a per-component signal. Matches C112 (`waybill:workspace-mode`), C120 (`waybill:workspaces-detected`), C136 (`waybill:go-toolchain-detected`) precedents.
- Value is a string (not boolean) because future modes will need to distinguish `"go"` from `"all"` from `"elf"` — future-proofs the extractor.

**Alternatives considered**:
- Reuse an existing C-row (e.g., extend C136's payload). Rejected — semantic muddying; each C-row should have exactly one meaning per m071 parity design.
- Emit as a per-component `pkg:golang/*` annotation instead of document-scope. Rejected — the components aren't emitted (that's the whole point of the flag), so per-component isn't a viable anchor.
- Boolean value (true/false). Rejected — future modes need distinguishable values.

## R4: SBOM emission surface per format

**Decision**:
- **CDX 1.6**: `metadata.properties[]` array entry with `name: "waybill:binary-scan-suppressed"`, `value: "<mode>"`. Emit in `waybill-cli/src/generate/cyclonedx/document.rs` where document-scope properties are assembled (near existing `waybill:workspace-mode` emit site).
- **SPDX 2.3**: `creationInfo.creators` or `annotations[]` on the `SPDXRef-DOCUMENT`. Waybill's existing document-scope annotation channel writes to `annotations[]` per m071 — reuse.
- **SPDX 3.0.1**: `annotation` element with `subject: SpdxDocument`, `statement: "waybill:binary-scan-suppressed=<mode>"`. Emit in `waybill-cli/src/generate/spdx3/annotations.rs`.

**Rationale**: matches existing waybill document-scope annotation emission points for parity-catalog compliance. Zero new emission paths.

**Alternatives considered**:
- CDX-only, skip SPDX. Rejected — Constitution V requires cross-format parity per the m071 catalog.
- CDX `metadata.tools[].note`. Rejected — waybill's convention is `metadata.properties[]` for machine-readable waybill annotations; `note` is human-readable freeform.

## R5: Sibling-repo fixture path

**Decision**: fixture at `no_binary_scan/gobin_with_buildinfo` (short path — one dir with the pre-built binary + a `README.md` documenting the Go version + source snippet used to build). Accessed via existing m090 `fixture_path("no_binary_scan/gobin_with_buildinfo")` helper. SHA pin managed in the fixture repo's tag; waybill's `Cargo.toml` `[package.metadata.fixtures]` block pins the tag (same pattern as m090).

**Rationale**:
- m090's `fixture_path()` handles the fetch, cache, and pin verification. Zero new plumbing.
- Small binary (~1 MB target size) minimizes fixture-repo bloat.
- Cross-platform tests can either check in per-target binaries (`_darwin_arm64`, `_linux_x86_64`) OR test only on one platform per CI lane. Decision: single ELF binary; only Linux CI lane runs SC-005. macOS lane runs SC-006 (annotation parity — no binary required).

**Alternatives considered**:
- Multi-platform binary (ELF + Mach-O + PE). Rejected for v1 — one binary per platform triples fixture size; single-platform SC-005 verification is enough for FR-002.
- Generate via `go build` in a build.rs. Rejected — introduces Go toolchain requirement per project memory Principle I intent.

## R6: FR-003 byte-identity preservation methodology

**Decision**: existing `cargo +stable test --workspace` guarantees byte-identity on the default (flag-absent) path. NO new goldens need to change. Verification:

1. Before writing production code: baseline workspace test count is **5183 passed / 0 failed** (m664 merge point).
2. Ship the feature (production code + tests).
3. Run `cargo +stable test --workspace --no-fail-fast` with no `WAYBILL_UPDATE_*` env vars.
4. If count remains 5183+N (where N = new tests added by this feature), FR-003 is satisfied.
5. If any pre-existing test regresses on the default path, the feature is broken — revert or fix.

**Rationale**: this is the same SC-004 gate used by m664 (per project memory `feedback_prepr_gate_full_output`). No new methodology needed.

**Alternatives considered**:
- Two-run diff harness (one with flag, one without, expect annotation-only diff). Rejected as CI-time-mandatory — instead this is a per-feature validation in the SC-006 test scaffolding, not a workspace-wide gate.

## Constitution re-check post-research

All Phase 0 decisions preserve every principle:
- I. Pure Rust, Zero C: confirmed zero new deps.
- IV. Type-Driven Correctness: `ValueEnum`-derived enum is the surface.
- V. Specification Compliance: new C-row + extractor plumbing per m071 gate.
- VII. Test Isolation: SC-006 uses `EnvGuard`.
- VIII. Completeness: annotation emission per FR-004.
- X. Transparency: INFO log at scan-start.

Ready to proceed to Phase 1 design.
