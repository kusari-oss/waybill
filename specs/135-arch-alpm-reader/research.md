# Research — milestone 135 Arch Linux pacman/alpm reader

Resolves the Phase 0 open items from `plan.md`'s Technical Context: the Constitution Principle V audit, the on-disk pacman DB format, the integration site within `read_all`, the file-claim tracker plumbing, and the ladder for distro-namespace selection.

## R1: Constitution Principle V audit — does any standards-native field carry "alpm package identity"?

**Decision**: The purl-spec's `alpm` PURL type IS the standards-native identity. No `mikebom:*` annotation is introduced. The PURL itself carries the package name, version, distro namespace (via the PURL namespace segment), and architecture (via the `arch=` qualifier).

**Rationale**:

The [purl-spec](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst#alpm) defines the `alpm` type explicitly for Arch Linux Pacman packages with the following normative shape:

```
pkg:alpm/<distro-namespace>/<package-name>@<version>?arch=<architecture>
```

- `<distro-namespace>` — the distro identifier (e.g., `arch`, `manjaro`, `steamos`)
- `<package-name>` — the pacman package name (verbatim)
- `<version>` — the pacman version string (typically `<upstream-ver>-<pkgrel>`)
- `arch=<architecture>` — the `%ARCH%` field verbatim (`x86_64`, `aarch64`, `any`, etc.)

CycloneDX 1.6 and SPDX 2.3 / SPDX 3.0.1 all recognize PURL as a first-class component-identity field. Emitting the alpm PURL gets us:

- CDX: `components[].purl` (native)
- SPDX 2.3: `packages[].externalRefs[]` with `referenceCategory: PACKAGE-MANAGER` and `referenceType: purl` (native)
- SPDX 3: `software_Package.software_packageUrl` + `Element.externalIdentifier[]` with `externalIdentifierType: "packageUrl"` (native)

The `distro=` qualifier convention mirrors the existing dpkg/apk/rpm readers' behavior:
- dpkg emits `pkg:deb/debian/curl@7.88.1-10+deb12u4?arch=amd64&distro=debian-12`
- apk emits `pkg:apk/alpine/curl@8.5.0-r0?arch=x86_64&distro=alpine-3.19`
- rpm emits `pkg:rpm/fedora/curl@8.5.0-2.fc40?arch=x86_64&distro=fedora-40`
- alpm will emit `pkg:alpm/arch/curl@8.5.0-1?arch=x86_64` (no `distro=` on rolling Arch) or `pkg:alpm/steamos/curl@8.5.0-1?arch=x86_64&distro=steamos-3.5.7` (on SteamOS where `VERSION_ID` is present)

**Alternatives considered**:

- **Custom `mikebom:pacman-package` annotation.** Rejected: the purl-spec already has a native type; using anything else would be exactly the "reinvent the standard" antipattern Principle V was added to prevent. The milestone-052 `mikebom:dev-dependency` case (later removed in favor of native scope fields) is the canonical motivating example.
- **Generic `pkg:generic/` PURL with a `mikebom:source-type = "pacman"` discriminator.** Rejected: same Principle V failure — would deliberately ignore the existing standards-native identity in favor of a custom carrier.
- **Skip the `distro=` qualifier entirely.** Rejected: the qualifier IS the only signal distinguishing same-package different-distro instances (e.g., `bash` from Manjaro vs `bash` from Arch — both pacman, but different repo provenance for downstream license/CVE attribution). Matches the dpkg/apk/rpm precedent.

**No new C-row in the parity catalog**: the alpm PURL is native across all three formats. No `mikebom:*` annotation introduced means no new `docs/reference/sbom-format-mapping.md` row.

## R2: On-disk pacman DB format

**Decision**: parse `/var/lib/pacman/local/<pkg>-<ver>/desc` and `files` files directly. No SQLite, no daemon, no shell-out.

**Pacman DB layout** (stable since pacman 4.0, ~2012):

```
/var/lib/pacman/
├── local/                                      # INSTALLED packages — in scope
│   ├── glibc-2.40-1/
│   │   ├── desc                                # %KEY%-format stanza with metadata
│   │   ├── files                               # owned-file manifest
│   │   ├── mtree                               # file modes / hashes (signed)
│   │   ├── install                             # optional install hook script
│   │   └── changelog                           # optional human changelog
│   ├── curl-8.5.0-1/
│   │   ├── desc
│   │   └── files
│   ...
└── sync/                                       # AVAILABLE packages — out of scope
    ├── core.db                                 # gzipped tar of remote-repo metadata
    ├── extra.db
    └── ...
```

**`desc` file format** — sequential blocks of:

```
%KEY%
value-line-1
value-line-2
                                                # blank line terminates the block
%NEXT_KEY%
value
...
```

**Keys we care about** (subset of pacman's full key set):

| Key | Cardinality | Use |
|---|---|---|
| `%NAME%` | 1 | PURL `<package-name>` segment |
| `%VERSION%` | 1 | PURL `<version>` segment (typically `<upstream>-<pkgrel>`) |
| `%ARCH%` | 1 | PURL `arch=` qualifier |
| `%DESC%` | 1 | Component description (free text) |
| `%URL%` | 0..1 | Homepage URL (external reference) |
| `%LICENSE%` | 0..N | License expression (per-line; multiple → SPDX `AND`) |
| `%PACKAGER%` | 0..1 | Supplier / packager identity (free text) |
| `%DEPENDS%` | 0..N | Dependency identifiers (per-line; format `<name>` or `<name><op><ver>`) |
| `%OPTDEPENDS%` | 0..N | Optional deps (per-line; format `<name>: <reason>`) |
| `%CONFLICTS%` | 0..N | Conflicting package names |
| `%REPLACES%` | 0..N | Replaced package names |
| `%PROVIDES%` | 0..N | Virtual-package names this satisfies |
| `%REASON%` | 0..1 | `0` = explicit install, `1` = dep-of-explicit (informational) |

**`files` file format** — much simpler:

```
%FILES%
usr/
usr/bin/
usr/bin/curl
usr/lib/
usr/lib/libcurl.so
usr/lib/libcurl.so.4
usr/lib/libcurl.so.4.8.0
usr/share/man/man1/curl.1.gz
```

Lines ending in `/` are directories (not file claims). Real file claims are the non-trailing-slash lines. Paths are rootfs-relative without a leading `/`.

**Alternatives considered**:

- **Shell out to `pacman -Q`.** Rejected: the binary isn't guaranteed to exist on the scanned rootfs (mikebom is host-portable; the scanned target may be a Linux rootfs extracted on macOS). Matches the dpkg/apk/rpm precedent — read on-disk metadata directly.
- **Parse `/var/lib/pacman/sync/*.db` (the sync/cache databases).** Rejected: those describe AVAILABLE packages from the remote repos, not INSTALLED ones. Explicitly out of scope per the spec.
- **Use the `alpm` C library via FFI.** Rejected: violates Principle I (Pure Rust, Zero C). The on-disk format is plain text and trivially parseable in stdlib.

## R3: Integration site within `read_all`

**Decision**: add the call site adjacent to the existing `dpkg::read(...)` / `apk::read(...)` / `rpm::read(...)` invocations in `mikebom-cli/src/scan_fs/package_db/mod.rs:1211–1239`.

**Existing dispatcher shape** (line 1211, simplified):

```rust
match dpkg::read(rootfs, &deb_namespace, distro_version.as_deref()) {
    Ok(entries) => {
        out.extend(entries);
        dpkg::collect_claimed_paths(rootfs, &mut claimed, /* cfg(unix) */ &mut claimed_inodes);
    }
    Err(e) => tracing::debug!(error = %e, "dpkg db read failed (expected if no dpkg)"),
}
match apk::read(rootfs, distro_version.as_deref()) {
    Ok(entries) => {
        out.extend(entries);
        apk::collect_claimed_paths(rootfs, &mut claimed, /* cfg(unix) */ &mut claimed_inodes);
    }
    Err(e) => tracing::debug!(error = %e, "apk db read failed (expected if no apk)"),
}
```

**Insertion**:

```rust
match alpm::read(rootfs, &alpm_namespace, distro_version.as_deref()) {
    Ok(entries) => {
        out.extend(entries);
        alpm::collect_claimed_paths(rootfs, &mut claimed, /* cfg(unix) */ &mut claimed_inodes);
    }
    Err(e) => tracing::debug!(error = %e, "pacman db read failed (expected if no pacman)"),
}
```

Where `alpm_namespace` comes from `/etc/os-release`'s `ID` field (verbatim, lowercased; defaulting to `arch` when absent or empty) — extending the existing `deb_namespace` derivation logic at line 1200–1206 with a parallel `alpm_namespace` resolution.

**Rationale**: The existing dispatcher already iterates every OS package DB regardless of which one is on disk; each reader's `Err` returns are debug-logged (intentional — "no apk DB present on a Debian system" is normal). Adding alpm follows the same pattern with zero new control flow.

## R4: File-claim tracker plumbing (milestone 004)

**Decision**: implement `alpm::collect_claimed_paths(rootfs, &mut claimed, &mut claimed_inodes)` mirroring the existing `dpkg::collect_claimed_paths` shape.

**Existing dpkg shape** (line 175):

```rust
pub fn collect_claimed_paths(
    rootfs: &Path,
    claimed: &mut HashSet<PathBuf>,
    #[cfg(unix)] claimed_inodes: &mut HashSet<(u64, u64)>,
)
```

Reads the `.list` files under `<rootfs>/var/lib/dpkg/info/*.list` and inserts every owned path (resolved against `rootfs`) into the shared claimed-paths set. The binary walker (`mikebom-cli/src/scan_fs/binary/`) consumes this set to skip emission of `pkg:generic/<binary>` components for files owned by an OS package.

**Alpm shape**: walk `<rootfs>/var/lib/pacman/local/<pkg>-<ver>/files`, parse the `%FILES%` block, insert each non-directory path (resolved against `rootfs`) into `claimed`. The `claimed_inodes` side is populated via `stat()` on each resolved path (same pattern as dpkg — only fires on Unix targets).

**Implementation cost**: the `files` parsing is dramatically simpler than the `desc` parsing — no key/value semantics, just lines. Total LOC for `collect_claimed_paths` is ~60 lines including doc comments.

## R5: Distro-namespace selection ladder

**Decision**: extend the existing `deb_namespace` derivation at `mod.rs:1200–1206` with a parallel `alpm_namespace` block. Both consume the same `/etc/os-release` parse result.

**Ladder**:

```
alpm_namespace =
    if id_raw is None or empty:
        "arch"                                  # default — rolling-release Arch
    else:
        id_raw.to_lowercase()                   # verbatim distro ID
```

**Notable IDs we expect to see** (per the existing FR-010 set):

| `/etc/os-release` ID | Resulting namespace | `VERSION_ID` typically present? |
|---|---|---|
| (absent) | `arch` | No |
| `arch` | `arch` | No |
| `manjaro` | `manjaro` | Yes (e.g., `24.0.0`) |
| `endeavouros` | `endeavouros` | No (rolling) |
| `steamos` | `steamos` | Yes (e.g., `3.5.7`) |
| `cachyos` | `cachyos` | Sometimes |
| `arcolinux` | `arcolinux` | Sometimes |
| (anything else) | verbatim `id_raw.to_lowercase()` | Unknown |

The "verbatim pass-through" behavior (FR-010 — no allowlist gate) is what lets future Arch derivatives work without code changes.

**Alternatives considered**:

- **Hardcode an allowlist of "recognized" derivatives.** Rejected: every new derivative would require a code change. Future-proofing via verbatim pass-through is cheap.
- **Always use `arch` as the namespace regardless of `ID`.** Rejected: SteamOS and Manjaro have meaningfully different package sets and CVE feeds; collapsing them under `arch` would degrade downstream vulnerability matching.

## R6: Multi-OS-DB rootfs handling (cross-reader independence)

**Decision**: alpm sits alongside dpkg/apk/rpm/opkg in the dispatcher; all five run on every scan, each independently emitting whatever its DB contains.

**Observation**: a single rootfs can legitimately contain multiple OS package DBs:

- A Debian rootfs with a `chroot` Alpine for cross-build: dpkg DB + apk DB
- A multi-layer container image where one layer is Debian and another is Alpine: both DBs in the merged filesystem (whiteouts permitting)
- A test fixture deliberately combining multiple distros

The existing readers handle this by independently iterating each DB and emitting whatever they find. alpm follows the same pattern — its presence does NOT suppress dpkg / apk / rpm output, and vice versa. The file-claim tracker accumulates claims from all five readers cooperatively.

**Edge case**: on a hybrid rootfs where `/usr/bin/curl` is owned by BOTH a deb package AND an alpm package (impossible in single-distro reality but constructible in a hand-crafted test fixture), both readers register a claim for the same path. The binary walker sees the path as claimed (by either) and skips. Both component-tier entries (`pkg:deb/...` AND `pkg:alpm/...`) survive — which is the correct behavior because both DBs declared ownership.

**Alternatives considered**:

- **Refuse to scan a rootfs with multiple OS DBs.** Rejected: legitimate hybrid scenarios exist; defensive refusal would degrade real workflows.
- **First-DB-wins suppression.** Rejected: would silently drop information from secondary DBs; violates Principle X (Transparency).

## R7: Reader edge-case posture (parse failures, missing fields, etc.)

**Decision**: warn-and-skip on every per-package failure (FR-009), preserving partial output. Fail-the-scan ONLY on conditions that indicate the entire rootfs read is broken (which for the alpm reader means: never — even a totally empty `local/` is just "no packages here").

| Condition | Behavior | Justification |
|---|---|---|
| `/var/lib/pacman/local/` absent | Return `Ok(vec![])` immediately | Same as dpkg-absent on Alpine — clean no-op |
| `local/` exists but empty | Return `Ok(vec![])` immediately | No packages installed; not an error |
| Per-package `desc` file unreadable | `tracing::warn!`, skip the package, continue | FR-009 — partial output is more valuable than no output |
| `desc` file present but missing `%NAME%` | `tracing::warn!`, skip, continue | Can't synthesize a PURL without the name; same posture as dpkg's missing-`Package:` handling |
| `desc` file present, name + version present, malformed `%DEPENDS%` line | `tracing::warn!`, emit the component without that single dep edge, continue | Other deps still useful; one bad line shouldn't drop the whole component |
| `files` manifest missing or unreadable | `tracing::warn!`, emit the component WITHOUT file-claim integration | Component identity still emits; binary walker may produce duplicates as a soft regression |
| Group-package entry encountered | Silently skip (not warn) | Groups are alias-only by design; not a real package |
| Two `local/` entries with same name + version | Treat both as distinct components | Pacman conventionally enforces uniqueness, but archive scans can violate this; mirrors milestone-134's divergent-PURL detection if the dep sets disagree |

## R8: Performance considerations

**Decision**: no performance budget violations expected; per-scan cost is bounded by package count.

- Per-package work: open `desc` file (a few KB max), parse stanza-style (single-pass tokenizer), construct one `PackageDbEntry`. Estimated 200–500 µs per package on a warm cache.
- `files` parse: line-format iteration; ~50 µs per package for typical sizes.
- Total scan cost: ~150 ms on a heavy-desktop Arch install (3000 packages); ~12 ms on a stock container (250 packages).

The no-pacman-DB fast path is one `Path::exists()` call (~1 µs); statistically free.

**Alternatives considered**:

- **Lazy / async per-package reads.** Rejected: total work is small enough that synchronous reads are simpler and faster (no executor overhead).
- **In-memory cache of parsed `desc` across scans.** Rejected: out of scope for a per-scan ephemeral tool.

---

## Summary of Phase 0 resolutions

| Unknown | Decision | Reference |
|---|---|---|
| Principle V audit | Native `pkg:alpm/` PURL exists; no `mikebom:*` annotation needed | R1 |
| On-disk pacman DB format | Plain-text `desc` + `files` per package; parse directly | R2 |
| Integration site | Adjacent to dpkg/apk/rpm in `read_all` dispatcher | R3 |
| File-claim tracker integration | Mirror `dpkg::collect_claimed_paths` shape | R4 |
| Distro namespace ladder | `/etc/os-release` `ID` verbatim; default `arch` | R5 |
| Multi-OS-DB cooperation | All readers run independently; claims accumulate | R6 |
| Per-package error posture | Warn-and-skip; never fail-the-scan | R7 |
| Performance | ~150 ms on heavy install; no budget concerns | R8 |

All Phase 0 unknowns resolved. Ready for Phase 1 (data-model + contracts + quickstart).
