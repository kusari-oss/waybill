# Research — milestone 136 Homebrew (brew + Linuxbrew) reader

Resolves the Phase 0 open items from `plan.md`'s Technical Context: the Constitution Principle V audit for the unblessed `pkg:brew/` PURL type, the on-disk `INSTALL_RECEIPT.json` schema, the Cask metadata format, the prefix-detection ladder, the file-claim deferral rationale, and the multi-OS-DB coexistence posture.

## R1: Constitution Principle V audit — `pkg:brew/` PURL type is unblessed

**Decision**: Emit `pkg:brew/<formula>@<version>` per industry convention. File a sibling purl-spec extension proposal as a follow-up. Document the informal status in spec Assumptions + this research entry; no `mikebom:*` annotation introduced.

**Rationale**:

The [purl-spec PURL-TYPES.rst](https://github.com/package-url/purl-spec/blob/main/PURL-TYPES.rst) currently defines explicit types for `alpm`, `apk`, `cargo`, `composer`, `conda`, `deb`, `gem`, `generic`, `github`, `golang`, `maven`, `npm`, `nuget`, `pypi`, `rpm`, `swid`, and others — but NOT for Homebrew/`brew`. Other SBOM tools (cyclonedx-bom-gen, syft) have adopted `pkg:brew/<formula>@<version>` as a de-facto convention; mikebom follows the same shape.

Principle V's "standards-native fields take precedence" clause asks whether a standards-native carrier exists. For component identity, the PURL field IS the native carrier — every format (CDX 1.6 `components[].purl`, SPDX 2.3 `externalRefs[purl]`, SPDX 3 `software_packageUrl`) consumes it as first-class. The question is only the type-name token: `pkg:brew/` is not yet purl-spec-blessed.

Two options for handling this:

1. **Emit `pkg:brew/...` as an unblessed convention** (chosen). Aligns with the de-facto industry shape. Downstream consumers that don't recognize `brew` as a known type still see a syntactically valid PURL and can string-match on the prefix.
2. **Emit `pkg:generic/?repository_url=https://brew.sh&name=<formula>@<version>`** (rejected). Loses the ecosystem signal; consumers can't filter Homebrew components without parsing the qualifier.

The Yocto reader (milestone 128) hit the same situation with `pkg:yocto/` and chose the same convention. Future-state: a purl-spec PR introducing the `brew` type formally would let mikebom keep its emitted output unchanged.

**No `mikebom:*` annotation introduced.** The component identity rides the native PURL field across all three formats; no parity-catalog C-row needed.

**Audit narrative for any future docs reference**: "mikebom emits `pkg:brew/<name>@<version>[?tap=<owner>/<tap>][&type=cask]` per the de-facto convention used by other SBOM tooling. The `brew` PURL type is not yet formally registered in the purl-spec PURL-TYPES.rst; a follow-up issue should propose its addition. Emitting under an informal type-name is preferable to inventing a `mikebom:*` annotation because PURL IS the standards-native identity carrier — the unblessed type-name affects only consumer-side filter sophistication, not wire-format validity."

**Follow-up issue (to file post-merge)**: "Propose `brew` (or `homebrew`) PURL type to package-url/purl-spec."

## R2: `INSTALL_RECEIPT.json` schema (modern 2024–2026 Homebrew)

**Decision**: Parse a small subset of the receipt — identity (from directory name + receipt cross-check), `source.tap`, `runtime_dependencies`. Treat every field as `Option<T>` except where the path through the JSON requires presence; warn-and-skip on parse failures.

**Schema** (per the [Homebrew tab.rb source](https://github.com/Homebrew/brew/blob/master/Library/Homebrew/tab/tab.rb), HEAD as of 2026):

Top-level keys:

```text
homebrew_version, time, source, runtime_dependencies, built_as_bottle,
poured_from_bottle, loaded_from_api, installed_on_request, changed_files,
source_modified_time, stdlib, compiler, aliases, arch, built_on, used_options,
unused_options
```

Only two fields are unconditionally present in modern receipts:

- `homebrew_version: String` — always emitted.
- `time: i64` — Unix epoch install timestamp. Always emitted.

**Fields mikebom consumes**:

```text
source.tap: Option<String>         // "homebrew/core" for core formulae; "<owner>/<tap>"
                                   // for third-party taps; null for raw-path installs.
runtime_dependencies: Vec<RuntimeDep>
  RuntimeDep:
    full_name: String              // PURL name source. May be tap-prefixed
                                   // ("hashicorp/tap/terraform" form for non-core taps).
    version: Option<String>        // Upstream version.
    pkg_version: Option<String>    // "<version>_<revision>" — round-trips the Cellar
                                   // directory name. Use when present, fall back to
                                   // `version` otherwise.
    revision: Option<u32>          // Homebrew rebuild counter.
    declared_directly: Option<bool>// True for deps in the formula's own depends_on block;
                                   // false for transitives. Surfaced as evidence (optional).
```

**Fields mikebom IGNORES** (parsed-but-discarded for forward compatibility): all other top-level keys.

**Identity sourcing**: formula name comes from the directory walk (`Cellar/<formula>/<version>/`), not the receipt. The receipt has no top-level `name` field — that information lives in the path. This matches Homebrew's own behavior: `brew list` uses the directory name as the identifier.

**License**: NOT in the receipt. License lives in the formula `.rb` source (Ruby DSL `license "..."` call) or in the JSON API at `formulae.brew.sh/api/formula/<name>.json`. Out of scope for milestone 136 — `licenses` field on `PackageDbEntry` stays empty for brew components. Mirrors the milestone-135 FR-012 deferral (URL/homepage out of scope on alpm too).

**Pre-2014 fossil records**: the receipt format has existed since at least 2011 (per the [legacy Homebrew issue #8616](https://github.com/Homebrew/legacy-homebrew/issues/8616)). Modern fields like `runtime_dependencies` were added in 2016/2017 ([Homebrew/brew#930](https://github.com/Homebrew/brew/issues/930)). The reader treats every field as optional, so very old receipts produce a valid-but-thin component (just identity from the directory + no dep graph).

**Alternatives considered**:

- **Shell out to `brew list` / `brew deps`.** Rejected: the `brew` binary isn't guaranteed to exist on the scanned rootfs; mikebom may be scanning a macOS-target rootfs from a Linux host. Matches the dpkg/apk/rpm/alpm posture.
- **Parse the formula `.rb` Ruby DSL.** Rejected: Ruby parsing is a non-trivial dependency and would violate the Principle I spirit (no embedded scripting). Receipt-only metadata is sufficient for the v1 slice.
- **Hit the `formulae.brew.sh` JSON API for license enrichment.** Rejected: network calls violate the FR-010 offline-by-default posture. License surfacing is a separate enrichment concern (parallels deps.dev for Maven enrichment).

## R3: Cask metadata format

**Decision**: Parse `<prefix>/Caskroom/<cask>/<version>/.metadata/<version>/<timestamp>/Casks/<token>.json` files (modern Homebrew 4.0+ API-backed cask installs). Warn-and-skip `.rb`-only casks (pre-4.0 or Ruby-only formulae) per Principle I (no Ruby parser).

**Layout** (per [Cask::Metadata Ruby API docs](https://rubydoc.brew.sh/Cask/Metadata.html)):

```text
<prefix>/Caskroom/
  └── <cask-token>/
      └── <version>/
          ├── .metadata/
          │   └── <version>/
          │       └── <timestamp>/         // %Y%m%d%H%M%S.%L
          │           └── Casks/
          │               ├── <token>.json   ← parse this when present
          │               └── <token>.rb     ← warn-and-skip when .json absent
          └── <installed payload — App bundles, binaries, etc.>
```

**`<token>.json` shape** (per Homebrew's `Cask::DSL` JSON serializer, format introduced in Homebrew 4.0 ~2023):

```text
token: String                       // matches the cask directory name
version: String                     // installed version
name: Vec<String>                   // human-readable name(s); may have multiple aliases
desc: Option<String>                // description
homepage: Option<String>            // upstream URL
url: Option<String>                 // download URL
sha256: Option<String>              // download checksum (NOT the installed-bytes hash)
depends_on: Option<DependsOn>       // can declare formula: deps but conventionally doesn't
artifacts: Vec<Artifact>            // .app, .pkg, binary, etc.
```

**Identity sourcing for casks**: `token` comes from the directory name (matching Homebrew's own convention); `version` comes from the directory name (or the JSON, if present — both agree by construction).

**Cask PURL shape**: `pkg:brew/<token>@<version>?type=cask`. The `type=cask` qualifier discriminates casks from formulae so downstream consumers can filter (a `pkg:brew/firefox@121.0` could ambiguously be a formula or a cask without it).

**Casks with `.rb`-only metadata** (older installs or non-API-backed casks): warn-and-skip with a single `tracing::warn!` naming the cask directory. The user-facing diagnostic: `"brew: cask <token> at <path> has only Ruby-DSL metadata (no Casks/<token>.json); skipping — Ruby parsing is out of scope per Constitution Principle I"`. Transparency-preserving per Principle X — operators see EXACTLY what was skipped and why.

**Cask deps**: the `depends_on.formula` array (when present) MAY participate in the dep graph; in practice casks rarely declare formula deps. Out of scope for v1 — casks emit without dep edges per FR-005.

**Alternatives considered**:

- **Embed a Ruby parser to handle `.rb`-only casks.** Rejected: violates Constitution Principle I (Pure Rust, Zero C — extends to "no embedded interpreters" by spirit). Adding a Ruby crate (e.g., `artichoke` or `rufu`) is a substantial dep with unclear maintenance posture.
- **Heuristic-extract identity from `.rb` content via regex.** Rejected: fragile, brittle against DSL evolution. Operators with pre-4.0 cask installs are advised to `brew reinstall` to get JSON-backed metadata.

## R4: Prefix-detection ladder

**Decision**: Check existence of `<rootfs>/<prefix>/Cellar/` for each of the three documented prefixes. Cellar-subdir presence is the discrimination signal; `<prefix>/` alone is not (especially for `/usr/local` which is a generic Linux sysadmin path).

**Ladder**:

```rust
const HOMEBREW_PREFIXES: &[&str] = &[
    "opt/homebrew",                  // Apple Silicon macOS (default since macOS 11 / 2020)
    "usr/local",                     // Intel macOS (default for pre-2020 installs)
    "home/linuxbrew/.linuxbrew",     // Linuxbrew (Linux developer machines + CI)
];
```

For each prefix:

1. Check `<rootfs>/<prefix>/Cellar/.is_dir()`. If false, skip — this prefix has no Homebrew install.
2. If true, walk `<rootfs>/<prefix>/Cellar/<formula>/<version>/INSTALL_RECEIPT.json`.
3. Independently check `<rootfs>/<prefix>/Caskroom/.is_dir()`; if true, walk for casks.

All three prefixes are processed independently — a single rootfs can contain Homebrew at multiple prefixes (pathological but constructible — e.g., an Apple Silicon Mac with a leftover Rosetta-era `/usr/local/Cellar/`). Each is processed without suppressing the others. PURL-identical formulae across prefixes collapse via the standard `seen_purls` dedup at `package_db/mod.rs:~1042`.

**Why not `HOMEBREW_PREFIX` env-var detection?** Custom prefix installs are rare. The env var is a runtime tool flag, not a persistent install signal — there's no way to know if the scanned rootfs was installed with a custom prefix from filesystem evidence alone. Documented as a deferred follow-up in spec Out-of-Scope.

**Why include `/usr/local`?** Even though it's a generic Linux sysadmin path, on Intel macOS it's THE Homebrew prefix. The `Cellar/` subdir check is what makes the detection unambiguous — `/usr/local/bin/` alone says nothing about Homebrew; `/usr/local/Cellar/` is unambiguous.

## R5: Why file-claim integration is deferred

**Decision**: Do NOT implement `collect_claimed_paths` for brew in milestone 136. Document the known soft regression (binary walker may emit `pkg:generic/<binary>` alongside `pkg:brew/<formula>`) in spec Out-of-Scope. Follow-up issue tracks the deferred work.

**Rationale**: Homebrew's install topology is fundamentally different from dpkg/apk/rpm/alpm.

- **dpkg** owns paths under `/usr/bin/`, `/usr/lib/`, etc. directly. The `.list` file maps `<package> → <owned-paths>` linearly.
- **alpm** same as dpkg — paths in `files` manifest are the canonical owned paths.
- **rpm** carries `BASENAMES` + `DIRNAMES` arrays — same flat-list model.
- **opkg** same as dpkg.
- **Homebrew** installs into the Cellar (`<prefix>/Cellar/<formula>/<version>/bin/<bin>`) then SYMLINKS into the prefix's exposed paths (`<prefix>/opt/<formula> → ../Cellar/<formula>/<version>`, `<prefix>/bin/<bin> → ../Cellar/<formula>/<version>/bin/<bin>`, etc.). The "user-visible" path is a symlink chain.

To make the binary walker's file-claim work correctly for Homebrew, the claim tracker would need to:

1. Walk the Cellar contents per formula (paths inside `<prefix>/Cellar/<formula>/<version>/bin/`, `lib/`, etc.).
2. Resolve the corresponding symlinks at `<prefix>/bin/`, `<prefix>/opt/`, etc., and claim those paths too.
3. Handle Homebrew's `keg_only` flag (formulae that AREN'T symlinked — those have no `<prefix>/bin/` exposure).
4. Handle Cellar-internal symlinks (some formulae symlink between their own versions).

That's a meaningfully larger surface than the alpm reader's flat `%FILES%` parse. Doing it right warrants its own spec + ~200-400 LOC of symlink resolution logic. Deferring keeps milestone 136 tight; the soft regression (binary walker emits duplicates) is acceptable for v1 since:

- macOS scans aren't the primary target for binary-walker emission today (mikebom's binary readers are Linux-focused with `cfg(unix)` gating on some integrations)
- The duplicate `pkg:generic/curl` vs `pkg:brew/curl@8.5.0` is consumer-side filterable
- Operators most concerned about this can filter on the `mikebom:source-type = "brew"` evidence property

**Alternatives considered**:

- **Implement a naive Cellar-only walk.** Rejected: would miss the `<prefix>/bin/` exposure that the binary walker actually scans, producing zero useful dedup.
- **Shell out to `brew --prefix --installed` for path resolution.** Rejected: violates the no-live-brew-invocation posture (FR-010 spirit + dpkg/apk/rpm/alpm precedent).

## R6: Multi-OS-DB rootfs handling (cross-reader independence)

**Decision**: brew sits alongside dpkg/apk/rpm/opkg/alpm in the dispatcher; all six run on every scan, each independently emitting whatever its DB/install contains.

**Observation**: a Linuxbrew rootfs is also a Debian/Ubuntu rootfs (Linuxbrew runs on top of a real Linux distro). On such a rootfs:

- dpkg finds the distro's `glibc`, `bash`, etc., and emits `pkg:deb/debian/glibc@<ver>?distro=debian-12`.
- brew finds the Linuxbrew-installed formulae and emits `pkg:brew/<formula>@<ver>`.
- Both surface — the operator wanted both views (Homebrew installs supplement, not replace, the underlying distro packages).

Same pattern as milestone-135's hybrid alpm + dpkg + apk scan in research §R6. No reader suppresses any other; the file-claim tracker (when populated by dpkg/alpm/apk) cooperatively skips binary-walker duplicates for paths owned by ANY reader. The brew reader's lack of file-claim integration (per R5) means brew-owned binaries WILL produce `pkg:generic/*` duplicates on Linuxbrew rootfs — known soft regression.

**Edge case**: a single binary at `/usr/local/bin/curl` could in principle be owned by both Homebrew (via Cellar+symlink) AND by the distro's curl package (if that exists at the same path). On real systems this collision doesn't happen because Linuxbrew installs into `/home/linuxbrew/.linuxbrew/`, not `/usr/local/`. On Intel macOS where Homebrew IS in `/usr/local/`, there's no dpkg/apk/rpm to collide with.

## R7: Per-formula and per-cask error posture

**Decision**: warn-and-skip on every per-formula or per-cask failure (FR-007), preserving partial output. Never fail-the-scan.

| Condition | Behavior | Justification |
|---|---|---|
| None of the 3 prefix `Cellar/` dirs exist | Return `Ok(vec![])` immediately | Clean no-op per FR-006 — matches dpkg-absent-on-Alpine posture |
| Cellar exists, no formula subdirs | Return `Ok(vec![])` immediately | No installed formulae; not an error |
| `INSTALL_RECEIPT.json` unreadable | `tracing::warn!`, skip the formula, continue | FR-007 — partial output > no output |
| Receipt JSON parse-error | `tracing::warn!`, skip, continue | Same posture as dpkg's malformed-stanza handling |
| Receipt JSON present but `runtime_dependencies` absent | Emit component with empty `depends`, continue | Older receipts pre-2017 lack this field; partial info still useful |
| Receipt JSON's `source.tap` absent or null | Treat as default `homebrew/core` (omit `tap=` qualifier) | Matches Homebrew's own default-tap fallback behavior |
| Cellar dir present but no `INSTALL_RECEIPT.json` | `tracing::warn!`, skip (very old installs only — rare in 2026) | Cannot synthesize component identity without it |
| Cask `.metadata/` present but no `.json` or `.rb` | Skip silently (not an installed cask) | Empty `.metadata/` is the Homebrew sentinel for "cask was uninstalled but dir not cleaned" |
| Cask `.json` absent but `.rb` present | `tracing::warn!`, skip with diagnostic naming the cask | R5 — no Ruby parser per Principle I |
| Cask `.json` parse-error | `tracing::warn!`, skip, continue | Same posture as formula |

## R8: Performance considerations

**Decision**: no performance budget violations expected; per-scan cost is bounded by formula + cask count.

- Per-formula: open `INSTALL_RECEIPT.json` (typically 1–5 KB), `serde_json::from_slice` → typed receipt struct, build `PackageDbEntry`. Estimated 150–300 µs per formula on warm cache.
- Per-cask: nested directory walk (`.metadata/<version>/<timestamp>/Casks/<token>.json`) — single `readdir` per level + JSON parse. ~500 µs per cask.
- Total scan cost: ~75 ms on a heavy developer install (300 formulae + 50 casks). Stock developer install (~50 formulae, no casks): ~10 ms.
- The no-Homebrew-detected fast path: three `Path::exists()` calls (~1 µs each); statistically free.

**Alternatives considered**:

- **Lazy / async per-formula reads.** Rejected: total work is small enough that synchronous reads are simpler and faster (no executor overhead).
- **In-memory cache of parsed receipts across scans.** Rejected: per-scan ephemeral tool; out of scope.

---

## Summary of Phase 0 resolutions

| Unknown | Decision | Reference |
|---|---|---|
| Principle V audit | `pkg:brew/` unblessed in purl-spec; emit per de-facto convention; file follow-up spec extension | R1 |
| INSTALL_RECEIPT.json schema | Parse subset (identity from path + `source.tap` + `runtime_dependencies`); treat fields as Option | R2 |
| Cask metadata format | Parse `Casks/<token>.json` (Homebrew 4.0+); warn-and-skip `.rb`-only casks | R3 |
| Prefix detection | Three documented prefixes, gated on `Cellar/` subdir existence | R4 |
| File-claim deferral | Out of scope per spec; symlink-resolution warrants separate spec | R5 |
| Multi-OS-DB cooperation | All six readers run independently; alpm-precedent applies | R6 |
| Per-formula error posture | Warn-and-skip; never fail-the-scan | R7 |
| Performance | ~75 ms on heavy install; no budget concerns | R8 |

All Phase 0 unknowns resolved. Ready for Phase 1 (data-model + contracts + quickstart).
