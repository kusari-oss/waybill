# Research — milestone 077 root component override

Six implementation-level decisions to pin before Phase 1 design.

## §1 — PURL emitter URL-encoding behavior

**Decision**: Add a new private helper `fn percent_encode_purl_name(s: &str) -> String` in `mikebom-cli/src/generate/cyclonedx/metadata.rs` (and re-exported into the spdx modules) that performs RFC 3986 percent-encoding for the PURL `name` segment. Use this helper ONLY when constructing the override-path PURL. Leave the existing `url_friendly()` helper at `spdx/v3_document.rs:410` untouched for the non-override path.

**Rationale**: The existing `url_friendly` performs lossy substitution (replaces non-`[A-Za-z0-9._-]` characters with `-`). For an operator-supplied `--root-name @acme/widget-svc`, that would produce PURL `pkg:generic/-acme-widget-svc@1.2.3` — different from the operator's intent. The Q1 clarification explicitly said "URL-encode the rest at PURL emission time," meaning RFC 3986 percent-encoding (`%40acme%2Fwidget-svc` for the `@` and `/` characters), not substitution.

Why not refactor `url_friendly` to also use percent-encoding? Risk of regressing existing fixture goldens. Today's fixtures (cargo, deb, npm, etc.) have PURL-safe basenames whose `url_friendly` result equals their input; switching to percent-encoding would produce identical output for those cases, so the regression risk is theoretically zero — but verifying that empirically across all existing fixtures + format combinations is more scope than this milestone needs. Safer: introduce the new encoder for the override path only; let a future cleanup milestone consolidate.

The new helper's encoding rule per RFC 3986 §2.3 (Unreserved Characters): preserve `[A-Za-z0-9._~-]`; percent-encode everything else (including UTF-8 bytes for non-ASCII characters).

**Alternatives considered**:
- Refactor `url_friendly` to do percent-encoding — Rejected for risk-of-regression reasons above.
- Keep `url_friendly` substitution for the override path too — Rejected: operator supplies `@acme/widget-svc`, expects to see something close to that in the PURL. Lossy substitution surprises operators.
- Reject PURL-unsafe characters at parse time — Rejected: contradicts the Q1 permissive clarification.

## §2 — `RootComponentOverride` shape

**Decision**: Define a small struct in `mikebom-cli/src/generate/mod.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct RootComponentOverride {
    pub name: Option<String>,
    pub version: Option<String>,
}
```

Add a new field to `ScanArtifacts`:

```rust
pub struct ScanArtifacts<'a> {
    // ... existing fields ...
    /// Milestone 077: operator-supplied overrides for the root
    /// component's name + version. When `name` or `version` is
    /// Some(_), the override replaces the corresponding auto-derived
    /// value in `metadata.component` (CDX) / main-module Package
    /// (SPDX 2.3) / root element (SPDX 3). When None, the existing
    /// auto-derivation flow runs unchanged.
    pub root_override: RootComponentOverride,
}
```

Default to `RootComponentOverride::default()` (both fields `None`) for back-compat — existing struct-literal call sites that use struct-update syntax (`..Default::default()`) continue to compile.

**Rationale**: Struct-with-named-fields beats `(Option<String>, Option<String>)` tuple for clarity at call sites and future-proofing (if `--root-vendor` or other root-component override flags are added later, the struct gains fields rather than the tuple growing). `Default` derive lets existing call sites construct `ScanArtifacts` without explicit `root_override: RootComponentOverride::default()` if they use update syntax.

**Alternatives considered**:
- Tuple field — Rejected: less self-documenting at call sites.
- Two separate `Option<String>` fields directly on `ScanArtifacts` — Rejected: doesn't group related state; future expansion churns more call sites.
- Keep on `ScanArgs` (CLI-layer struct) instead of `ScanArtifacts` (emit-layer struct) — Rejected: breaks the existing pattern where ScanArtifacts holds ALL emit-layer state. Other identifier flags (075's `--keep-credentials-in-identifiers`, 076's `--component-id`) followed this same pattern.

## §3 — Manifest-driven main-module detection + drop site

**Decision**: At the CDX builder's `build()` site (`cyclonedx/builder.rs:163-228`), filter `components` to drop main-module entries when `root_override.name.is_some()` OR `root_override.version.is_some()`. Filter is keyed on the existing `mikebom:component-role = main-module` property (per the comment at `builder.rs:286` already documenting how main-modules are identified). Mirror in SPDX 2.3 (`spdx/document.rs`) and SPDX 3 (`spdx/v3_document.rs`) flows.

**Rationale**: Per the Q2 clarification (clean replacement), when override is set the manifest-derived main-module disappears entirely from the emitted SBOM. The filter happens BEFORE `build_components` runs so the main-module never reaches the per-component emission code path.

The filter MUST apply uniformly when EITHER override field is set (not just when both are set). Operator passing `--root-name widget-svc` with no `--root-version` gets `widget-svc@<auto-derived-version>` as the root; the manifest's `foo-internal` is still dropped because the operator opted into "the root is widget-svc, not foo-internal" — version override is independent.

When both override fields are `None`, no filter applies; existing milestone-073/074/075/076 behavior is preserved exactly.

Multi-main-module case (cargo workspace) at `builder.rs:267-272`: when `main_module_count > 1`, no main-module is promoted to `metadata.component` today; the synthesized scan-target acts as root. With override set, the override identity becomes the root and ALL main-modules in the workspace are dropped. This is consistent — explicit operator input replaces the entire main-module set, regardless of whether it's 1 or N main-modules.

**Alternatives considered**:
- Filter inside `build_components` rather than before — Rejected: harder to reason about; mixes filter logic with emission logic.
- Skip the filter when `root_override` only sets version (just name overridden, version not) — Rejected: name-only override still implies "the root is widget-svc, not foo-internal"; partial filter would be inconsistent.
- Add a `--keep-manifest-main-module` flag to opt out of the drop — Rejected: out of scope for MVP. The future GitHub issue #151 tracks the demote-to-library exploration.

## §4 — CLI flag validation regex

**Decision**: Validator function:

```rust
fn validate_root_field(value: &str, flag_name: &str) -> Result<String, String> {
    if value.is_empty() {
        return Err(format!("{flag_name} must not be empty"));
    }
    for (i, c) in value.chars().enumerate() {
        if c.is_whitespace() {
            return Err(format!(
                "{flag_name} contains whitespace at position {i} \
                 (character: {:?}); whitespace is not allowed",
                c
            ));
        }
        if c.is_control() {
            return Err(format!(
                "{flag_name} contains a control character at position {i} \
                 (codepoint: U+{:04X}); control characters are not allowed",
                c as u32
            ));
        }
        if c == '?' || c == '#' {
            return Err(format!(
                "{flag_name} contains URL-syntax-breaking character '{c}' \
                 at position {i}; '?' and '#' are not allowed"
            ));
        }
    }
    Ok(value.to_string())
}
```

Used as a clap `value_parser` for both `--root-name` and `--root-version`. Returns `Result<String, String>` so clap's error formatter surfaces the message to the operator at parse time.

**Rationale**: Implements the Q1 clarification exactly — permissive accept (any non-empty UTF-8) with rejection only for `\s`, control chars, `?`, and `#`. The error messages are deliberately verbose so operators understand WHICH character violated WHICH rule (operators with weird-but-legal names like `@acme/widget-svc` need to know it's the `@` or `/` that's fine, vs `?`/`#` which aren't).

The function signature returning `Result<String, String>` matches the milestone-076 `parse_component_id_flag` pattern (returns clap-shaped errors).

**Alternatives considered**:
- Stricter validation (reject anything outside `[a-zA-Z0-9._-]`) — Rejected per Q1 clarification.
- `regex::Regex` for the rule — Rejected: overhead for what's a per-character classification.
- `clap`'s built-in `value_parser` for non-empty strings — Rejected: doesn't cover the per-character classification needed.

## §5 — CPE construction with override values

**Decision**: When the override is set, construct the CPE from the override values: `format!("cpe:2.3:a:mikebom:{}:{}:*:*:*:*:*:*:*", cpe_escape(name), cpe_escape(version))`. Reuse the existing `cpe_escape` helper at `cpe.rs:145`. Vendor portion stays hardcoded `mikebom` per assumption.

**Rationale**: CPE formatted-string-binding rules (NIST IR 7695 §6.2) require specific escaping of WFN component values — `cpe_escape` already implements this. Operator-supplied override values flow through the same escaping; no per-CPE-version logic needed at the override layer.

The vendor stays `mikebom` for consistency with the existing CPE emission. A future milestone can introduce `--root-vendor` if multi-vendor CPE namespacing is requested. Note: the spec's example output already shows vendor=mikebom, so this is the documented MVP behavior.

**Alternatives considered**:
- Allow operator to override vendor too — Out of scope per spec assumptions; future milestone.
- Drop CPE entirely on override — Rejected: CPE consumers (vulnerability scanners hard-keyed on CPE) would lose the override-identity match.
- Use `cpe:2.3:a:<NAME>::*:*:*:*:*:*:*` with no vendor — Rejected: violates CPE 2.3 spec; vendor is required.

## §6 — `mikebom sbom generate` subcommand reach

**Decision**: Scope the new flags to `mikebom sbom scan` (both `--path` and `--image` variants) and `mikebom trace run`. Do NOT add to `mikebom sbom generate` (the post-trace re-emit-from-attestation flow).

**Rationale**: The `generate` subcommand reads an existing attestation and re-emits its embedded SBOM. The root component identity comes from the embedded SBOM, not from operator input — overriding it would silently mutate a previously-generated artifact's identity, which is surprising. Operators who want to override during `generate` can do so by re-scanning with `scan` (which honors the new flags), or by post-processing with `mikebom sbom enrich` (which is the documented JSON-Patch path for ad-hoc edits to existing SBOMs).

If operator demand for `generate`-time override emerges later, a follow-up milestone can add the flags there with explicit semantics about how they interact with the embedded SBOM. For MVP, `scan` + `run` is sufficient — those are the SBOM-construction call sites where override has unambiguous semantics.

**Alternatives considered**:
- Add to `generate` too — Rejected: ambiguous semantics on a re-emit flow.
- Add to ALL SBOM-emitting subcommands uniformly — Rejected: parity ≠ correctness; `enrich` and `verify-binding` and similar don't construct root components, so adding the flags there is meaningless.
