# Quickstart — milestone 098 maintainer recipes

Five recipes for landing compiler/linker version extraction.

## Recipe 1 — Implement the ELF `.comment` parser (FR-001 / US1)

Open `mikebom-cli/src/scan_fs/binary/elf.rs`. Add `parse_comment_section` per `data-model.md §elf.rs — extension` and `research.md §2`. Append unit tests covering happy path + multi-toolchain dedup + size caps + non-UTF-8 lossy decode + empty section.

Then in `mikebom-cli/src/scan_fs/binary/scan.rs`, inside `scan_binary` after the existing build-id / runpath / debuglink extraction (around line 158):

```rust
let comment_stamps: Vec<String> = if class == "elf" {
    file.section_by_name_bytes(b".comment")
        .and_then(|s| s.data().ok())
        .map(elf::parse_comment_section)
        .unwrap_or_default()
} else {
    Vec::new()
};
```

Add `comment_stamps` to the `BinaryScan` initializer block (around line 223).

Add a `pub comment_stamps: Vec<String>` field to the `BinaryScan` struct in `entry.rs` (after the existing ELF fields).

Add the `fake_binary_scan` test-helper at `entry.rs:898` to include `comment_stamps: Vec::new()`.

Extend `build_elf_identity_annotations` in `entry.rs`:

```rust
if !scan.comment_stamps.is_empty() {
    bag.insert(
        "mikebom:elf-compiler-stamps".to_string(),
        serde_json::json!(scan.comment_stamps),
    );
}
```

Compile-check: `cargo +stable check -p mikebom`.

## Recipe 2 — Implement the Mach-O `LC_BUILD_VERSION` full parser (FR-002 / US2)

In `macho.rs`, add the `MachoBuildVersion` struct + `parse_build_version_full` + `tool_name` helper per `data-model.md §macho.rs — extension` and `research.md §3`.

Append unit tests: happy path (LC_BUILD_VERSION with platform=macos, minos=14.0, sdk=14.4, 2 tools), unknown-platform passthrough, malformed-ntools defensive parse, missing-LC_BUILD_VERSION returns None.

Add `pub macho_build_version: Option<macho::MachoBuildVersion>` field to `BinaryScan` (and to the `fake_binary_scan` test-helper).

In `scan.rs::scan_binary` (Mach-O path) and `scan.rs::scan_fat_macho` (first slice), set the field:

```rust
let macho_build_version = if class == "macho" {
    macho::parse_build_version_full(bytes)
} else {
    None
};
```

Extend `build_macho_identity_annotations` in `entry.rs` per `data-model.md §entry.rs`:

```rust
if let Some(bv) = &scan.macho_build_version {
    bag.insert(
        "mikebom:macho-build-version".to_string(),
        serde_json::json!({
            "platform": bv.platform,
            "min_os": bv.min_os,
            "sdk": bv.sdk,
        }),
    );
    if !bv.tools.is_empty() {
        let tools_json: Vec<serde_json::Value> = bv.tools.iter()
            .map(|(tool, version)| serde_json::json!({"tool": tool, "version": version}))
            .collect();
        bag.insert(
            "mikebom:macho-build-tools".to_string(),
            serde_json::Value::Array(tools_json),
        );
    }
}
```

## Recipe 3 — Implement the PE linker-version always-emit (FR-003 / US3)

In `pe.rs`, add `parse_linker_version` per `data-model.md §pe.rs — extension`. Decide at implementation time whether to:
- **(a)** extend the `PeIdentity` tuple to 4-elements, OR
- **(b)** refactor to a named `PeIdentity` struct with 4 fields.

Both are fine; (b) is slightly cleaner but increases churn. (a) is the minimal-diff path.

Append unit tests: happy-path linker-version extraction (14.36), zeroed-linker always-emit (0.0).

Add `pub pe_linker_version: Option<String>` field to `BinaryScan` (and to the `fake_binary_scan` test-helper).

In `scan.rs::scan_binary` (PE path), set the field:

```rust
let pe_linker_version = if class == "pe" {
    Some(super::pe::parse_linker_version(bytes))
} else {
    None
};
```

Extend `build_pe_identity_annotations` in `entry.rs`:

```rust
if let Some(lv) = &scan.pe_linker_version {
    bag.insert(
        "mikebom:pe-linker-version".to_string(),
        serde_json::Value::String(lv.clone()),
    );
}
```

## Recipe 4 — Add 4 parity-catalog rows (FR-010 / Constitution V)

Find the highest existing C-row in `mikebom-cli/src/parity/extractors/mod.rs`:

```bash
grep -nE 'row_id: "C[0-9]+"' mikebom-cli/src/parity/extractors/mod.rs | tail -3
```

Append 4 new rows after the highest (likely C15 → next is C16). Per `data-model.md §Parity-catalog rows`:

```rust
ParityExtractor { row_id: "C16", label: "mikebom:elf-compiler-stamps",  cdx: c16_cdx, spdx23: c16_spdx23, spdx3: c16_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
ParityExtractor { row_id: "C17", label: "mikebom:macho-build-version",  cdx: c17_cdx, spdx23: c17_spdx23, spdx3: c17_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
ParityExtractor { row_id: "C18", label: "mikebom:macho-build-tools",    cdx: c18_cdx, spdx23: c18_spdx23, spdx3: c18_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: true },
ParityExtractor { row_id: "C19", label: "mikebom:pe-linker-version",    cdx: c19_cdx, spdx23: c19_spdx23, spdx3: c19_spdx3, directional: Directionality::SymmetricEqual, order_sensitive: false },
```

Add 4 macro invocations per format file (`cdx.rs` / `spdx2.rs` / `spdx3.rs`):

```rust
cdx_anno!(c16_cdx, "mikebom:elf-compiler-stamps", component);   // cdx.rs
cdx_anno!(c17_cdx, "mikebom:macho-build-version", component);
cdx_anno!(c18_cdx, "mikebom:macho-build-tools", component);
cdx_anno!(c19_cdx, "mikebom:pe-linker-version", component);
```

(Same shape with `spdx23_anno!` / `spdx3_anno!` for the other two files.)

Add 4 rows to `docs/reference/sbom-format-mapping.md` mirroring C10/C11/C15's format.

## Recipe 5 — Run pre-PR gate + verify diff scope

```bash
MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1 ./scripts/pre-pr.sh
# Expected: `>>> all pre-PR checks passed.`

# Diff scope (Contract 9):
git diff --name-only main | sort
# Expected:
#   CLAUDE.md                                  (auto-updated)
#   docs/reference/sbom-format-mapping.md      (4 new rows)
#   mikebom-cli/src/parity/extractors/{cdx,mod,spdx2,spdx3}.rs
#   mikebom-cli/src/scan_fs/binary/{elf,entry,macho,pe,scan}.rs
#   mikebom-cli/tests/binary_build_provenance.rs   (NEW, optional)
#   specs/098-compiler-version-extract/...

# No Cargo.* changes (FR-005):
git diff --name-only main | grep -E '^Cargo\.(lock|toml)$' && echo "DEP CHURN" || echo "clean"
# Expected: clean

# Goldens regen scope (FR-009 / SC-007):
git diff --stat mikebom-cli/tests/fixtures/golden/ | tail -1
# Expected: empty or minimal additive changes
```

## When in doubt

- **A new Mach-O platform value emerges and the `platform_name` allowlist is incomplete**: don't gate emission on the allowlist; emit `"unknown-<id>"` per FR-002 + Edge Case "unknown platform value". Add the new platform to the allowlist in a follow-up PR.
- **A `.comment` entry contains a NUL byte mid-string** (theoretical): the splitter at `parse_comment_section` treats every NUL as a terminator. Mid-NUL entries get split into two pieces. Defensive — matches typical linker behavior; document if signal emerges.
- **PE optional-header parsing fails** (corrupt binary): the existing scan-time `PeFile32`/`PeFile64::parse` failure path drops the binary before reaching `parse_pe_identity`. No new code path.
- **Mach-O `LC_BUILD_VERSION.cmdsize` is smaller than the 24-byte fixed header** (severely corrupt): `parse_build_version_full` returns `None` rather than reading off the end. Defensive — bounded by `for_each_load_command`'s existing cmdsize-validation.
- **Backward-compat panic on `mikebom:macho-min-os` and `mikebom:macho-build-version` both present**: not a panic — both emit; consumers key on whichever property they prefer. The flat `min_os` (milestone 024) and the structured `build-version.min_os` (this milestone) carry the same data, redundantly, by design for back-compat.
- **A future maintainer adds a new ELF identity property and the parity-catalog row count goes past C19**: that's fine — the catalog has no hard upper bound. Use the next available C-row.
