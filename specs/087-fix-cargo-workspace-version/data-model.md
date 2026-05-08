# Data Model — milestone 087 Cargo workspace-member version-disambiguation

The milestone introduces ZERO new production Rust types. The "data model" here is the conceptual encoding-shape change in `PackageDbEntry.depends` for cargo entries + the `name_to_purl` lookup-table dual-key insert.

## Existing types — unchanged signatures, changed encoding

### `PackageDbEntry.depends: Vec<String>` (existing)

Pre-087 (cargo entries): each string is the bare crate name, version stripped: `["clap_builder", "clap_derive", ...]`.

Post-087 (cargo entries): each string preserves Cargo.lock's exact form (`(source)` suffix stripped):

| Cargo.lock dep entry | Pre-087 `depends[]` value | Post-087 `depends[]` value |
|---|---|---|
| `"clap_builder"` (single-version case) | `"clap_builder"` | `"clap_builder"` (unchanged) |
| `"clap_builder 4.5.21"` (multi-version case) | `"clap_builder"` (version dropped ❌) | `"clap_builder 4.5.21"` ✅ |
| `"clap_builder 4.5.21 (registry+...)"` | `"clap_builder"` | `"clap_builder 4.5.21"` (source suffix stripped) |

Other ecosystems' `depends[]` shape is unchanged. The cargo reader's encoding choice doesn't propagate to deb/rpm/npm/etc.

### `name_to_purl: HashMap<(String, String), String>` (existing at `scan_fs/mod.rs:371`)

Pre-087: keyed by `(ecosystem, normalized_name)`; for cargo, name-only — multi-version same-name entries collide.

Post-087: cargo entries get a SECOND key `(ecosystem, "name version")` per milestone-085's maven `groupId:artifactId` precedent. Both keys point at the same PURL value:

| Entry | Pre-087 keys | Post-087 keys |
|---|---|---|
| `clap_builder@4.5.21` | `(cargo, "clap_builder")` | `(cargo, "clap_builder")` + `(cargo, "clap_builder 4.5.21")` |
| `clap_builder@4.5.9` | `(cargo, "clap_builder")` (collides!) | `(cargo, "clap_builder")` (collides — still last-write-wins) + `(cargo, "clap_builder 4.5.9")` (unique) |

The single-key still last-writes-wins (no behavior change for that path); the disambiguated key resolves correctly. Cargo's own `dependencies = [...]` writer chooses which form the dep-string takes — single-version case uses name-only (fine), multi-version case uses name+version (now disambiguated).

## Validation rules

- **VR-087-001**: For every cargo entry processed by `package_to_entry`, the resulting `depends` Vec MUST encode each `dependencies = [...]` list element verbatim with only the `(source)` suffix stripped.
- **VR-087-002**: For every cargo entry processed by the `name_to_purl` insert loop in `scan_fs/mod.rs`, the lookup table MUST contain BOTH `(cargo, name)` AND `(cargo, "name version")` keys. The single-key remains for `dependencies = ["name"]` lookups; the dual-key resolves `dependencies = ["name version"]` lookups correctly.
- **VR-087-003**: The cargo edge-emission loop at `scan_fs/mod.rs:548-564` MUST resolve each dep-name to the correct `(name, version)` PURL when version-disambiguation applies. Verified by the new regression test exercising the clap-rs/clap @ v4.5.21 fixture.
- **VR-087-004**: When Cargo.lock has only one `[[package]]` block for a given crate name, the dep-string is `"name"` (no version) and the lookup hits the existing single-key. Post-087 behavior unchanged for this case.
- **VR-087-005**: Existing milestone-064 cargo main-module emission is unaffected. Component identity is determined by the `[[package]] name + version` block, not by the `dependencies = [...]` encoding.
- **VR-087-006**: Existing milestone-085 maven `groupId:artifactId` lookup is unaffected. Cargo's name-version disambiguation is independent of maven's groupId-based one.
- **VR-087-007**: Existing milestone-052 cargo dev-dep classification (`scope: excluded`) is unaffected. The version-disambiguation runs orthogonally to scope classification (which uses target-side `lifecycle_scope`, not edge-source name).
- **VR-087-008**: `normalize_dep_name(cargo, "clap_builder 4.5.21")` produces `"clap_builder 4.5.21"` (lowercase no-op for digits + dots + already-lowercase ASCII). The dep-string from `cargo.rs` and the lookup-key from `mod.rs` end up at byte-identical forms after normalization. Tested via a new unit test.
- **VR-087-009**: Cargo CDX 1.6 + SPDX 2.3 + SPDX 3 goldens regenerate with diffs containing only the dep-edge `version` strings — no `metadata.component`, `components[].name`, `components[].version`, `components[].purl`, or other field-level changes.
- **VR-087-010**: Other ecosystems' goldens (apk/deb/gem/golang/maven/npm/pip/rpm × CDX/SPDX 2.3/SPDX 3 = 24 goldens) stay byte-identical pre/post 087. Verified by `cdx_regression`, `spdx_regression`, `spdx3_regression` runs without their `MIKEBOM_UPDATE_*_GOLDENS` env vars.

## Backward compatibility

- **Operator-perceived**: post-087 cargo SBOMs against multi-version-same-name workspaces have correctly-versioned dep edges. Pre-087 wrong-version edges go away. Vulnerability scanners stop reporting phantom CVEs from the wrong-version transitives. Reverse-impact analysis ("who depends on `clap_builder@4.5.21`?") starts returning correct answers.
- **Internal**: `PackageDbEntry.depends: Vec<String>` shape is unchanged. The encoding inside the strings changes for cargo only. Other ecosystem readers don't need to update.
- **Goldens**: 3 cargo goldens (CDX, SPDX 2.3, SPDX 3) regenerate. Other 24 stay byte-identical.
- **No new Cargo dependencies**.
- **No CI workflow changes** beyond what milestone 083 already added (trivy + syft for the audit suite, which this milestone reuses unchanged).
- **Closure invariant from milestone 084**: continues to hold post-087. The fix doesn't introduce or remove orphan refs; it only changes which PURL is the target of certain edges.
