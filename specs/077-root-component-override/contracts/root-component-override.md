# Contract — milestone 077 root component override

The milestone's only contract.

## CLI surface

### Two new flags (on `mikebom sbom scan` AND `mikebom trace run`)

```
--root-name <NAME>
--root-version <VERSION>
```

- Both repeatable: NO. Single-value each.
- Both default-absent.
- Validated at CLI parse via `validate_root_field` (research §4 + VR-077-001).
- Independent — passing one without the other supported.

### Help-text shape (clap-derived)

```
--root-name <NAME>
        Override the auto-derived `metadata.component.name` of the
        emitted SBOM. Useful when scanning an arbitrary directory
        whose basename doesn't reflect the operator-meaningful project
        identity. Accepts any non-empty UTF-8 except whitespace,
        control characters, `?`, and `#`. URL-encoded automatically
        when emitted into the PURL `name` segment.

        When this flag is set on a manifest-driven scan (Cargo, npm,
        pip, gem, Maven, Go), the manifest-derived main-module
        component is dropped entirely from the emitted SBOM
        (clean replacement). To preserve the manifest-derived
        identity as a regular library entry alongside the override,
        track GitHub issue #151.

--root-version <VERSION>
        Override the auto-derived `metadata.component.version`. Same
        validation rules as --root-name. Independent — can be set
        without --root-name and vice versa. When unset, falls through
        to the auto-derived version (typically `0.0.0` for arbitrary
        directories or the manifest-derived version for project scans).
```

## Library surface

### New struct

```rust
// In mikebom-cli/src/generate/mod.rs
#[derive(Debug, Clone, Default)]
pub struct RootComponentOverride {
    pub name: Option<String>,
    pub version: Option<String>,
}

impl RootComponentOverride {
    pub fn is_active(&self) -> bool {
        self.name.is_some() || self.version.is_some()
    }
}
```

### Updated `ScanArtifacts` shape

```rust
pub struct ScanArtifacts<'a> {
    // ... existing fields ...
    pub root_override: RootComponentOverride,
}
```

### New private helpers

```rust
// In mikebom-cli/src/cli/scan_cmd.rs (or shared module)
fn validate_root_field(value: &str, flag_name: &str) -> Result<String, String>;

// In mikebom-cli/src/generate/cyclonedx/metadata.rs (or shared)
fn percent_encode_purl_name(s: &str) -> String;
```

## Per-format wire mapping

When the override is active (`root_override.is_active() == true`), the per-format builders use override values for the root component AND drop manifest-derived main-module components.

### CDX 1.6

`metadata.component`:

| Field | Override-set value | When override unset (today) |
|-------|--------------------|-----------------------------|
| `name` | `<override.name>` (verbatim) | `target_name` (auto-derived) |
| `version` | `<override.version>` (verbatim) | `"0.0.0"` (hardcoded) |
| `bom-ref` | `<name>@<version>` | `<target_name>@0.0.0` |
| `purl` | `pkg:generic/<percent-encoded(name)>@<percent-encoded(version)>` | `pkg:generic/<target_name>@0.0.0` |
| `cpe` | `cpe:2.3:a:mikebom:<cpe_escape(name)>:<cpe_escape(version)>:*:*:*:*:*:*:*` | `cpe:2.3:a:mikebom:<target_name>:0.0.0:*:*:*:*:*:*:*` |
| `type` | unchanged (`application` for source/build, `container` for image) | (same) |

`components[]`: filter OUT entries with `properties[name=mikebom:component-role,value=main-module]` when `root_override.is_active()`.

### SPDX 2.3

Main-module Package:
| Field | Override-set value | When unset |
|-------|--------------------|------------|
| `name` | `<override.name>` | `target_name` |
| `versionInfo` | `<override.version>` | `"0.0.0"` |
| `SPDXID` | hash-derived from new name+version (existing pattern) | (same shape, different hash) |
| `externalRefs[purl]` | `pkg:generic/<percent-encoded(name)>@<percent-encoded(version)>` | (same shape) |

Drop manifest-derived main-module entries from `packages[]` when active.

### SPDX 3

Root Element + main-module Package (per `v3_document.rs:150,376-390,435`):
| Field | Override-set value |
|-------|--------------------|
| `name` | `<override.name>` |
| `software_packageVersion` | `<override.version>` |
| `software_purl` | `pkg:generic/<percent-encoded(name)>@<percent-encoded(version)>` |
| `spdxId` | hash-derived from new name+version |

Drop manifest-derived main-module entries from `elements[]` when active.

## Observable contract from outside the binary

### No-flag case (default behavior, unchanged from alpha.17)

```bash
$ mikebom sbom scan --path /opt/builds/abc123-snapshot --output out.cdx.json
... (no log lines about override)

$ jq '.metadata.component | {name, version, "bom-ref", purl}' out.cdx.json
{
  "name": "abc123-snapshot",
  "version": "0.0.0",
  "bom-ref": "abc123-snapshot@0.0.0",
  "purl": "pkg:generic/abc123-snapshot@0.0.0"
}
```

### Both flags set

```bash
$ mikebom sbom scan --path /opt/builds/abc123-snapshot \
    --root-name widget-svc --root-version 1.2.3 \
    --output out.cdx.json
INFO root component override active: name='widget-svc' (replacing 'abc123-snapshot'), version='1.2.3' (replacing '0.0.0')

$ jq '.metadata.component | {name, version, "bom-ref", purl, cpe}' out.cdx.json
{
  "name": "widget-svc",
  "version": "1.2.3",
  "bom-ref": "widget-svc@1.2.3",
  "purl": "pkg:generic/widget-svc@1.2.3",
  "cpe": "cpe:2.3:a:mikebom:widget-svc:1.2.3:*:*:*:*:*:*:*"
}
```

### Manifest-driven Cargo project with override

```bash
$ mikebom sbom scan --path ./my-cargo-project \
    --root-name widget-svc --root-version 1.2.3 \
    --output out.cdx.json
INFO root component override active: name='widget-svc', version='1.2.3'
INFO override is set; dropping manifest-derived main-module component `pkg:cargo/foo-internal@0.5.1` from emitted SBOM (per milestone 077 clean-replacement clarification; see GitHub issue #151 for the demote-to-library follow-up)

$ jq '.metadata.component.name' out.cdx.json
"widget-svc"

$ jq '.components[] | select(.purl == "pkg:cargo/foo-internal@0.5.1")' out.cdx.json
# (no output — manifest-derived component dropped)
```

### Validation error

```bash
$ mikebom sbom scan --path . --root-name "my widget svc"
error: invalid value 'my widget svc' for '--root-name <NAME>': --root-name contains whitespace at position 2 (character: ' '); whitespace is not allowed
```

### npm-scoped name (allowed per Q1 permissive)

```bash
$ mikebom sbom scan --path . --root-name "@acme/widget-svc" --root-version 1.2.3 --output out.cdx.json

$ jq '.metadata.component.purl' out.cdx.json
"pkg:generic/%40acme%2Fwidget-svc@1.2.3"
```

## Test contract

A new integration-test file `mikebom-cli/tests/identifiers_root_component_override.rs` MUST cover (per US1/US2/US3 acceptance scenarios + the SCs):

| Test | Acceptance scenario | Validates |
|------|--------------------|-----------|
| `root_name_and_version_override_on_arbitrary_dir` | US1 §1, SC-001 | FR-001 + FR-002 + FR-004 (derived fields) |
| `only_root_name_supplied_uses_default_version` | US1 §2, SC-003 | FR-003 |
| `only_root_version_supplied_uses_basename_name` | US1 §3, SC-004 | FR-003 |
| `no_flags_byte_identical_to_alpha17` | US1 §4, SC-002 | FR-009 |
| `override_on_image_tier_scan` | US2 §1 | FR-001 + image-tier coverage |
| `override_on_trace_run_build_tier` | US2 §2 | FR-001 + build-tier coverage |
| `override_drops_manifest_main_module_cargo` | US3 §1, SC-006 | FR-008 + Q2 clean-replacement |
| `no_override_preserves_manifest_main_module` | US3 §2 | FR-009 (no regression) |
| `override_emits_in_all_three_formats` | SC-005 | FR-007 |
| `validation_rejects_empty_name` | SC-007 | FR-006 |
| `validation_rejects_whitespace_name` | SC-008 | FR-006 |
| `validation_rejects_control_char_name` | edge case | FR-006 |
| `validation_rejects_question_mark_name` | edge case | FR-006 |
| `validation_rejects_hash_name` | edge case | FR-006 |
| `npm_scoped_name_url_encoded_in_purl` | edge case + SC-008 second sentence | FR-006 + research §1 |
| `override_deterministic_across_reruns` | SC-009 | FR-010 |
| `override_orthogonal_to_subject_hash` | edge case | FR-011 (orthogonality) |

Plus unit tests on `validate_root_field` (8 tests covering each rejection class) and `percent_encode_purl_name` (5 tests covering ASCII unreserved, ASCII reserved, UTF-8 multi-byte, empty, all-encoded).

Tests guarded with `#[cfg_attr(test, allow(clippy::unwrap_used))]` per CLAUDE.md.

## Performance contract

- Override + main-module-drop adds <1ms per scan: a single field check at the top of the per-format builder + one O(N) filter pass over `components[]` (where N is component count, typically <1000).
- Validator runs once per flag at CLI parse — O(L) in flag value length, negligible.
- `percent_encode_purl_name` runs once per emitted SBOM — O(L) in name length, negligible.

## Determinism contract (per FR-010, SC-009)

- Same `RootComponentOverride` + same scan target → byte-identical emitted SBOMs across re-runs.
- The override is a constant-input transformation; no clock, no UUID, no environment dependency on the override path itself.
- The non-override emission path is unchanged from alpha.17 (per FR-009).

## Logging contract

| Event | Level | Format |
|-------|-------|--------|
| Override active (both fields) | `tracing::info!` | `"root component override active: name='<name>' (replacing '<auto>'), version='<version>' (replacing '<auto>')"` |
| Override active (name only) | `tracing::info!` | `"root component override active: name='<name>' (replacing '<auto>')"` |
| Override active (version only) | `tracing::info!` | `"root component override active: version='<version>' (replacing '<auto>')"` |
| Manifest main-module dropped | `tracing::info!` | `"override is set; dropping manifest-derived main-module component '<purl>' from emitted SBOM (per milestone 077 clean-replacement; see GitHub issue #151)"` |
