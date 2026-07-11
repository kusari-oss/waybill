# Contract: `parse_ipk_filename` Decision Matrix (m185 US1)

**Feature**: [../spec.md](../spec.md) · **Plan**: [../plan.md](../plan.md) · **Data model**: [../data-model.md](../data-model.md)

## Scope

Canonical input × output table for `mikebom-cli/src/scan_fs/package_db/ipk_file.rs::parse_ipk_filename` under m185. This is the single-source-of-truth for what each filename input produces post-fix. Task implementers MUST reference this table when writing unit tests (see data-model.md §4).

## Semantic

**Post-m185 rule**: parse using `stem.rsplitn(3, '_')` iterator ordering — extract arch (first item), version (second item), name (third item, containing any additional underscores in the leading text).

The rsplitn iterator produces at most 3 slices. For an N-underscore stem where N ≥ 2, the iteration order returns:
1. First `next()` → the rightmost `_`-delimited segment (arch)
2. Second `next()` → the segment between the rightmost and second-rightmost `_` (version)
3. Third `next()` → the ENTIRE remaining leading text (name), which may itself contain `_` characters

**Empty-field guard**: any of `name`, `version`, or `arch` being empty after extraction → return None (fail-safe, preserves pre-m185 malformed-input behavior).

**`.ipk` extension guard**: `strip_suffix(".ipk")` must succeed — otherwise return None. Same as pre-m185.

## Full input × output table

| # | Input filename                                                                                                          | Iterator sequence (arch, version, name)                                                                                              | Emitted `Option<(name, version, arch)>`                                                                             | Pre-m185 | Change? |
|---|------------------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------|---------------------------------------------------------------------------------------------------------------------|----------|---------|
| 1 | `test-pkg_1.0-r0_all.ipk`                                                                                              | (`"all"`, `"1.0-r0"`, `"test-pkg"`)                                                                                                  | `Some(("test-pkg", "1.0-r0", "all"))`                                                                              | Same    | NONE    |
| 2 | `packagegroup-core-boot_1.0-r0_all.ipk`                                                                                | (`"all"`, `"1.0-r0"`, `"packagegroup-core-boot"`)                                                                                    | `Some(("packagegroup-core-boot", "1.0-r0", "all"))`                                                                | Same    | NONE    |
| 3 | `test-pkg_1.0+git0+abc_def-r0_all.ipk`                                                                                 | (`"all"`, `"1.0+git0+abc_def-r0"`, `"test-pkg"`)                                                                                     | `Some(("test-pkg", "1.0+git0+abc_def-r0", "all"))` ✱                                                                | `None`  | ✱ CHANGED |
| 4 | `kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard_6.6.127+git0+45f69741c7_70af2998be-r0_qemux86_64.ipk`             | (`"qemux86_64"`, `"6.6.127+git0+45f69741c7_70af2998be-r0"`, `"kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard"`)              | See below (real Yocto kernel-module shape)                                                                          | `None`  | ✱ CHANGED |
| 5 | `no-underscores.ipk`                                                                                                    | (`"no-underscores"`) — iterator produces 1 slice                                                                                     | `None` (version.next() returns None → early return via `?` on version binding)                                       | Same    | NONE    |
| 6 | `single_underscore.ipk`                                                                                                 | (`"underscore"`, `"single"`) — iterator produces 2 slices                                                                            | `None` (name.next() returns None → early return via `?` on name binding)                                             | Same    | NONE    |
| 7 | `some-file.txt` (no `.ipk` suffix)                                                                                     | (not reached — strip_suffix returns None)                                                                                            | `None`                                                                                                              | Same    | NONE    |
| 8 | `_1.0-r0_all.ipk` (empty leading name)                                                                                 | (`"all"`, `"1.0-r0"`, `""`)                                                                                                          | `None` (name.is_empty() guard)                                                                                       | Same    | NONE    |
| 9 | `test-pkg__all.ipk` (empty version between two underscores)                                                              | (`"all"`, `""`, `"test-pkg"`)                                                                                                        | `None` (version.is_empty() guard)                                                                                    | Same    | NONE    |
| 10 | `test-pkg_1.0-r0_.ipk` (empty arch after trailing underscore)                                                          | (`""`, `"1.0-r0"`, `"test-pkg"`)                                                                                                     | `None` (arch.is_empty() guard)                                                                                       | Same    | NONE    |

**Row 4 emitted `Option`** (kernel-module shape, formatted for clarity):

```
Some((
    "kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard",
    "6.6.127+git0+45f69741c7_70af2998be-r0",
    "qemux86_64"
))
```

**Legend**:
- ✱ = new m185 behavior. Two rows CHANGED (rows 3 and 4 — both multi-underscore versions).
- All other rows preserve pre-m185 byte-identity per FR-004 / SC-001-adjacent regression pins.

## Purl construction (downstream)

`filename_fallback_entry` at `ipk_file.rs:557` consumes the `parse_ipk_filename` output and calls `build_opkg_purl(&name, &version, &arch, distro_tag)`. The PURL construction is UNCHANGED by m185 — it URL-encodes special characters (`+` → `%2B`, `_` in version → preserved as-is per RFC 3986 / purl-spec) via the existing `encode_purl_segment` helper. Row 3's emitted PURL is:

```
pkg:opkg/test-pkg@1.0%2Bgit0%2Babc_def-r0?arch=all
```

Row 4's emitted PURL (assuming `distro_tag = None`):

```
pkg:opkg/kernel-module-nf-conntrack-tftp-6.6.127-yocto-standard@6.6.127%2Bgit0%2B45f69741c7_70af2998be-r0?arch=qemux86_64
```

Both PURLs are valid per purl-spec's `opkg` type definition. Pre-m185 both rows emitted `null` PURL because the parser returned None and the fallback path emitted a degenerate component (per issue #538 evidence).

## Invariants

1. **Canonical 2-underscore shape**: byte-identical output pre/post m185. Rows 1–2 above pin this via unit tests.
2. **Empty-field fail-safe**: any of name/version/arch being empty after `rsplitn(3, '_')` extraction returns None. Rows 8–10 pin this.
3. **`.ipk` suffix requirement**: stems without the `.ipk` suffix return None. Row 7 pins this.
4. **Fewer than 3 underscores → None**: rsplitn produces fewer than 3 slices when the stem has fewer than 2 underscores. Rows 5–6 pin this.
5. **PURL construction stays stable**: m185 does not modify `build_opkg_purl`. The URL-encoding rules are unchanged; only the name/version/arch triple fed into it changes.
