# Data Model — milestone 135

The alpm reader does not introduce any new types in `mikebom-common`. Every alpm-derived component flows through the existing `PackageDbEntry` → `ResolvedComponent` pipeline. The only new types are reader-private helper structs that exist inside `alpm.rs` to make parsing testable.

This document describes:

1. The reader-private `PacmanDescStanza` struct (parsing intermediate).
2. The `PackageDbEntry` field mapping (alpm's output of the existing shared type).
3. The file-claim contribution shape (extending the existing shared `HashSet<PathBuf>` claim sets).

## PacmanDescStanza (reader-private)

The parser's per-package intermediate representation. Lives inside `mikebom-cli/src/scan_fs/package_db/alpm.rs` only — NOT exposed across the crate boundary. Mirrors the shape and discipline of `dpkg.rs::DpkgStanza`.

```rust
// Lives in mikebom-cli/src/scan_fs/package_db/alpm.rs only.
struct PacmanDescStanza {
    /// `%NAME%` — required. Identity component of the resulting PURL.
    name: String,
    /// `%VERSION%` — required. Typically `<upstream-ver>-<pkgrel>` form.
    version: String,
    /// `%ARCH%` — required. `x86_64`, `aarch64`, `any`, etc.
    arch: String,
    /// `%DESC%` — optional human-readable description.
    description: Option<String>,
    /// `%URL%` — optional homepage URL. Surfaces as an external reference.
    homepage: Option<String>,
    /// `%LICENSE%` — multi-value. Each entry is one SPDX license expression
    /// fragment; pacman convention is to emit multiple lines for licenses
    /// joined by AND (the SPDX `AND` operator is implied across lines).
    licenses: Vec<String>,
    /// `%PACKAGER%` — optional. Free-text packager identity (typically
    /// `<Name> <<email>>` form). Surfaces as the component's supplier.
    packager: Option<String>,
    /// `%DEPENDS%` — multi-value. Each entry is a pacman dep spec which
    /// MAY include a version constraint (`<name><op><ver>`). Constraints
    /// are recorded but only the name participates in the dep graph.
    depends: Vec<String>,
    /// `%OPTDEPENDS%` — multi-value. Format `<name>: <reason>`. Does NOT
    /// participate in the dep graph; surfaced as evidence-tier metadata.
    optdepends: Vec<String>,
    /// `%CONFLICTS%` — multi-value. Recorded for evidence; does not affect
    /// component emission.
    conflicts: Vec<String>,
    /// `%REPLACES%` — multi-value. Same posture as `conflicts`.
    replaces: Vec<String>,
    /// `%PROVIDES%` — multi-value. Virtual-package names this satisfies.
    /// Recorded for future dep-resolution enhancements (a `Depends:`
    /// referencing a virtual name should map to whichever package provides
    /// it). Not used in v0.1 — direct dep names only.
    provides: Vec<String>,
    /// `%REASON%` — `Some(0)` for explicit installs, `Some(1)` for
    /// dep-of-explicit. Informational only.
    install_reason: Option<u8>,
}
```

### Validation rules

- `name`, `version`, `arch` MUST all be non-empty for the stanza to produce a `PackageDbEntry`. Missing any of these triggers warn-and-skip (FR-009).
- `name` MUST be safe for PURL encoding (purl-spec name segment). Names containing characters outside the unreserved set are percent-encoded at PURL construction time (same as every other reader).
- `version` and `arch` flow verbatim into the PURL.
- Multi-value fields (`depends`, `licenses`, etc.) are parsed line-by-line; empty / whitespace-only lines are skipped.
- Dep specs with version constraints (e.g., `glibc>=2.40`) split on the first comparison operator (`<`, `<=`, `=`, `>=`, `>`); only the name half participates in the dep graph. The full original string is preserved as the dep edge's evidence label (matches the dpkg posture for `Depends: foo (>= 1.0)`).

### State transitions

None. `PacmanDescStanza` is built once per package, consumed once to produce a `PackageDbEntry`, then dropped.

## PackageDbEntry field mapping (alpm reader's output)

Every alpm-derived `PackageDbEntry` populates the existing shared type's fields as follows:

| Field | Alpm source | Notes |
|---|---|---|
| `purl` | `pkg:alpm/<namespace>/<name>@<version>?arch=<arch>[&distro=<ns>-<verid>]` | Native PURL per purl-spec `alpm` type (research §R1) |
| `name` | `%NAME%` | Verbatim |
| `version` | `%VERSION%` | Verbatim |
| `arch` | `%ARCH%` | Verbatim |
| `source_path` | `<rootfs>/var/lib/pacman/local/<name>-<ver>` | Absolute path to the package's metadata dir |
| `depends` | Names extracted from `%DEPENDS%` (constraints stripped) | One entry per dep spec, deduplicated |
| `maintainer` | `%PACKAGER%` | Verbatim free text |
| `licenses` | `%LICENSE%` lines, each canonicalized via `SpdxExpression::try_canonical` | Multiple lines → joined with `AND` per pacman convention |
| `lifecycle_scope` | `None` | pacman has no scope distinction; same posture as dpkg/apk/rpm |
| `requirement_range` | `None` | pacman versions are pinned at install time; no range |
| `source_type` | `Some("alpm")` | Identifier for downstream consumers |
| `buildinfo_status` | `None` | N/A |
| `evidence_kind` | `Some("alpm-local-db")` | New canonical-enum value per data-model.md §C4 |
| `binary_class` | `None` | OS packages aren't ELF/Mach-O/PE classified |
| `binary_stripped` | `None` | Same |
| `linkage_kind` | `None` | Same |
| `detected_go` | `None` | Same |
| `confidence` | `None` | Direct DB read; no heuristic confidence |
| `binary_packed` | `None` | Same |
| `raw_version` | `None` | pacman doesn't have a separate raw-version field (rpm does) |
| `parent_purl` | `None` | OS-package components are top-level |
| `npm_role` | `None` | N/A |
| `co_owned_by` | `None` | Initial; cross-reader dual-ownership detection is a follow-up |
| `hashes` | `Vec::new()` | pacman's `mtree` file carries per-file hashes but not a package-level hash; deferred |
| `sbom_tier` | `Some("deployed")` | Same posture as dpkg/apk/rpm — these are installed-tier components |
| `shade_relocation` | `None` | N/A |
| `extra_annotations` | `BTreeMap::new()` | No `mikebom:*` annotations introduced per Principle V (R1) |
| `binary_role` | `None` | N/A |
| *(homepage / URL — NOT emitted)* | `%URL%` parsed into `PacmanDescStanza.homepage` | Per FR-012 deferral (analysis finding U1), the parsed value is held in-memory but not written to any `PackageDbEntry` field this milestone. Existing OS readers (dpkg/apk/rpm/opkg) do not emit URL/Homepage today; introducing it here would require cross-reader work tracked as a follow-up. |

### Validation rules (PackageDbEntry-level)

The existing `PackageDbEntry::validate` checks apply unchanged. Alpm-specific:

- `purl.as_str().starts_with("pkg:alpm/")` for every emitted entry.
- `arch` is present (non-empty) — pacman ALWAYS declares `%ARCH%`, even for noarch packages (`any`).
- If `licenses` is non-empty, every entry MUST be SPDX-canonicalizable (or fall back to a `LicenseRef-` per the existing milestone-012 license-handling discipline).

## File-claim contribution

Cross-reader claim sets are owned by `read_all` and passed to each reader's `collect_claimed_paths` function. The alpm reader extends these without changing their shape:

```rust
// Shared types — already in mikebom-cli/src/scan_fs/package_db/mod.rs.
type ClaimedPaths = HashSet<PathBuf>;
#[cfg(unix)]
type ClaimedInodes = HashSet<(u64, u64)>;  // (dev_id, inode_number)

// New function in alpm.rs (mirrors dpkg::collect_claimed_paths).
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut ClaimedPaths,
    #[cfg(unix)] claimed_inodes: &mut ClaimedInodes,
)
```

**Behavior**:

1. Walk `<rootfs>/var/lib/pacman/local/<*>/files`.
2. For each `files` manifest, parse the `%FILES%` block.
3. For each non-directory line (no trailing `/`):
   - Resolve to the absolute path `<rootfs>/<relative-path>`.
   - Insert into `claimed`.
   - On Unix, `stat()` the resolved path; on success, insert `(dev_id, inode)` into `claimed_inodes`.
4. Per-package read failures (missing `files` file, unreadable line) are warn-and-skipped per FR-009.

**Cooperative invariant**: claim insertion is set-union — multiple readers claiming the same path is idempotent. The binary walker downstream checks `claimed.contains(&path)` regardless of which reader inserted; identity of the claiming reader is not preserved at the claim-set level (the corresponding `PackageDbEntry.source_type` field is the authoritative identity).

## Integration with existing types

- `Purl` — `mikebom_common::types::purl::Purl::new` is the only entry point; constructs validated `pkg:alpm/...` strings. No alpm-specific constructor helper needed beyond a small reader-local convenience function `build_alpm_purl(namespace, name, version, arch, distro_qualifier) -> Result<Purl>`.
- `SpdxExpression::try_canonical` — used to normalize multi-line `%LICENSE%` values into a single SPDX expression.
- `ScanDiagnostics` — alpm doesn't introduce any new diagnostic fields; per-package warn-and-skip is logged via `tracing::warn!` and doesn't surface in the document-level diagnostic bag.
- `ResolvedComponent` — the existing conversion in `mikebom-cli/src/scan_fs/mod.rs:637` (where `PackageDbEntry.extra_annotations.clone()` flows into `ResolvedComponent.extra_annotations`) Just Works for alpm — there's nothing alpm-specific to thread through.

## Out-of-scope data shapes

- **AUR provenance discrimination** (R7 / spec assumption): the pacman DB does not natively carry "this came from AUR vs the official repo" — adding it would require parsing `/etc/pacman.conf` and `pacman.log`. Deferred.
- **Per-file `mtree` hash extraction**: pacman's `mtree` files carry mode/owner/size + SHA-256 per file. Surfacing those in evidence is a quality-of-output improvement deferred to a follow-up (parallels milestone-038's deb deep-hash work).
- **Group-package aggregation**: groups are alias-only; emitting components for groups would create phantom entries. Out of scope per FR-002 / Edge Cases.
- **`co_owned_by` cross-reader dual-ownership detection**: when a path is claimed by BOTH alpm AND another reader on a hybrid rootfs, the current model emits both components and lets the binary walker skip the file via path-claim union. A follow-up could add cross-reader dual-ownership annotations like the milestone-090 Maven `mikebom:co-owned-by` pattern.
