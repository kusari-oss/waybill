# Research — milestone 100 Windows-host build + run support

Phase 0 investigation. Six decision points, all resolved via audit-of-existing-code + reference-doc citations.

## §1 — `#[cfg(unix)]` gate audit (FR-001, FR-007)

**Decision**: no new gates required; the 94 existing `#[cfg(unix)]` markers are already correctly placed.

**Audit results** (categorized):

| Category | Count | Files | Windows behavior |
|----------|-------|-------|-------------------|
| `claimed_inodes: &HashSet<(u64, u64)>` plumbing — argument type | ~40 | `scan_fs/mod.rs`, `package_db/{dpkg, apk, rpm, go_binary, maven}.rs`, `binary/mod.rs` | Compiles clean; the argument simply doesn't exist on Windows so the function signatures omit it. |
| `claimed_inodes` insertion / lookup — usage in body | ~40 | Same set + helpers in `package_db/mod.rs::insert_claim_with_canonical` | Layers 1 (raw path) + 2 (canonical path) still run; layer 3 (inode match) is unreachable. Equivalent correctness for Windows (no hardlink / symlink-inode complexity in typical Windows scans). |
| `std::os::unix::fs::MetadataExt` (dev/ino access) | 5 | `binary/mod.rs:86`, `package_db/mod.rs:366` | All inside `#[cfg(unix)]` blocks; compiles clean on Windows. |
| `std::os::unix::fs::PermissionsExt` (Unix file modes) | 3 | `oci_pull/auth.rs:485`, `docker_image.rs:{314,345}` | All inside `#[cfg(unix)]` blocks; fallbacks use `Permissions::set_readonly()` (cross-platform) elsewhere. |
| Test-only `#[cfg(unix)]` tests | ~10 | Various test modules | Compile-skipped on Windows; the tests asserting Unix-specific symlink / mode behavior don't run there (which is correct). |

**Result**: zero new gates required. The audit recommends one *cosmetic improvement* — the `package_db/{dpkg,rpm,apk}.rs` `read()` entry points should explicitly return `Ok(Vec::new())` on Windows hosts BEFORE attempting any `/var/lib/dpkg`-style path probes (saves wasted syscalls + makes the "no-op on Windows" behavior intentional rather than accidental). Implementer's call; not blocking.

**Alternative considered**: convert `#[cfg(unix)]` to `#[cfg(target_os = "linux")]` for the Linux-specific readers. Rejected — macOS hosts can legitimately have Homebrew-installed dpkg or rpm directories (rare but real); keeping `unix` is the more permissive + correct gate.

## §2 — Path normalization architecture (Clarifications Q1, FR-004, SC-003)

**Decision**: implement a single `normalize_sbom_path(&Path) -> String` helper that wraps `path.to_string_lossy().into_owned()` with a backslash → forward-slash `replace` step. Apply at the deduplicator boundary (one chokepoint in `scan_fs/mod.rs`) + defensively at the 3 JSON-emission sites.

**Implementation pseudo**:

```rust
// mikebom-cli/src/scan_fs/sbom_path.rs (new module)

use std::path::Path;

/// Normalize a filesystem path for SBOM JSON emission per milestone-100
/// Clarifications Q1. On Windows, replaces backslash separators with
/// forward-slash; on Unix, returns the native string unchanged.
///
/// Why: SBOM JSON is a cross-platform artifact. Operators consuming
/// SBOMs from heterogeneous CI fleets shouldn't see noisy diffs from
/// path-format differences. The de facto industry convention (syft +
/// trivy) is forward-slash everywhere; this helper enforces it.
///
/// Note: only the *separator character* is normalized. Drive-letter
/// prefixes (`C:`) are preserved verbatim — a Windows path like
/// `C:\Users\dev\project\Cargo.toml` becomes `C:/Users/dev/project/Cargo.toml`.
pub fn normalize_sbom_path(path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        raw.replace('\\', '/')
    } else {
        raw
    }
}

/// Convenience variant for `&str` callers (e.g., a `source_path: String`
/// field already on a `PackageDbEntry`).
pub fn normalize_sbom_path_str(s: &str) -> String {
    if cfg!(windows) {
        s.replace('\\', '/')
    } else {
        s.to_string()
    }
}
```

**Chokepoint application** (per plan.md §Structure Decision):

The `scan_fs::scan_path` function (the deduplicator + ResolvedComponent builder at `scan_fs/mod.rs:534` and `scan_fs/mod.rs:167`) populates `ResolvedComponent.evidence.source_file_paths`. Add a single transformation pass:

```rust
// At scan_fs/mod.rs near line 542 where the ResolvedComponent is built:
evidence: ResolutionEvidence {
    technique: ResolutionTechnique::PackageDatabase,
    confidence: PACKAGE_DB_CONFIDENCE,
    source_connection_ids: vec![],
    source_file_paths: vec![
        normalize_sbom_path_str(&entry.source_path)
    ],
    deps_dev_match: None,
},
```

**Defensive normalization at emission sites**: 3 spots in the generators iterate `FileOccurrence.location` and emit it into JSON. Wrap each with `normalize_sbom_path_str(&o.location)` so future code paths bypassing the deduplicator can't leak backslashes. The double-normalization on Unix is a no-op `String::to_string()`; on Windows it idempotently re-validates.

**Estimated diff**:
- New module: ~30 lines (function + 2 unit tests + doc comments).
- Chokepoint call site: 1-line addition.
- Defensive emission-site updates: 3 sites × 1 line each.
- Total: ~35 lines net code.

**Alternatives considered**:

- **Normalize at all 88 `to_string_lossy()` call sites**: rejected — mechanical churn across many readers, easier to introduce bugs (some `to_string_lossy()` is used for log messages, not SBOM emission; would over-normalize). The single-chokepoint approach is more maintainable.
- **Use the `path_slash` crate**: rejected per FR-005 (zero new Cargo deps). The `replace('\\', '/')` operation is trivial without a crate.
- **Always normalize on every host, not just Windows**: equivalent behavior since `replace('\\', '/')` is a no-op on Unix paths. Code is simpler with the `cfg!(windows)` branch; on Unix we skip the replace entirely. Decision: keep the cfg branch for clarity + the no-op skip's tiny perf savings.

## §3 — CI Windows lane shape (FR-008)

**Decision**: clone the existing `lint-and-test-macos` job (at `.github/workflows/ci.yml:217`) onto `windows-latest`. Run the same `cargo +stable clippy --workspace --all-targets -- -D warnings` + `cargo +stable test --workspace` gate. Cache fixture repo + cargo build the same way.

**Job template** (per `data-model.md §lint-and-test-windows`):

```yaml
lint-and-test-windows:
  name: Lint + test (windows-latest)
  runs-on: windows-latest
  steps:
    - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2
      with:
        persist-credentials: false

    - name: Install stable Rust
      uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable
      with:
        components: clippy

    - name: Cache cargo + build artifacts
      uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1

    # Milestone 090 — fixture-repo cache. Same `~/.cache/mikebom/fixtures`
    # path; actions/cache handles the Windows-side path translation.
    - name: Cache fixture repo
      uses: actions/cache@27d5ce7f107fe9357f9df03efb73ab90386fccae # v5.0.5
      with:
        path: ~/.cache/mikebom/fixtures
        key: mikebom-fixtures-${{ runner.os }}-${{ hashFiles('tests/fixtures.rev') }}

    # No eBPF + nightly + bpf-linker steps (Linux-only).
    # No sbomqs steps (Linux/macOS only; gracefully skip on Windows).
    # No Python+spdx3-validate steps (would need separate setup-python action).
    - name: Clippy
      run: cargo +stable clippy --workspace --all-targets -- -D warnings

    - name: Tests
      run: cargo +stable test --workspace
```

**Notes**:
- `runner.os` evaluates to `Windows` on `windows-latest`, so the cache key differs from Linux/macOS (correct — Windows cargo build artifacts aren't compatible with Unix-host artifacts).
- The fixture-repo cache key uses the same hash-of-`tests/fixtures.rev` pattern; mikebom's build.rs already handles path-resolution cross-platform per milestone 090's fallback chain.
- SPDX 3 validator integration (`MIKEBOM_REQUIRE_SPDX3_VALIDATOR=1`) is *not* added to the Windows lane in v1. Python + the validator install can land in a follow-up; the SBOM emission itself is verified by the existing format-parity tests which run unchanged.

**Alternatives considered**:

- **Run Windows lane only on push to main, not on every PR**: rejected — defeats the regression-detection purpose; Linux + macOS run on every PR, Windows should match.
- **Skip the fixture cache**: rejected — the cache works on Windows runners (verified at planning time via the actions/cache docs); no reason to skip.

## §4 — Release pipeline Windows build job (FR-009)

**Decision**: add `build-windows-x86_64` to `.github/workflows/release.yml` after `build-macos-aarch64` (at `release.yml:209`). Use the macOS job as the template; swap target string, swap tarball-packaging step for zip-packaging.

**Job template**:

```yaml
build-windows-x86_64:
  name: Build — windows-x86_64
  runs-on: windows-latest
  env:
    TARGET: x86_64-pc-windows-msvc
  steps:
    - uses: actions/checkout@de0fac2e4500dabe0009e67214ff5f5447ce83dd # v6.0.2

    - name: Install stable Rust (x86_64-pc-windows-msvc)
      uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8 # stable
      with:
        targets: x86_64-pc-windows-msvc

    - name: Cache cargo + build
      uses: Swatinem/rust-cache@e18b497796c12c097a38f9edb9d0641fb99eee32 # v2.9.1
      with:
        key: ${{ env.TARGET }}

    - name: Build userspace binary
      run: cargo build --release --target $env:TARGET --package mikebom
      shell: pwsh

    - name: Package zip
      shell: pwsh
      run: |
        $stage = "mikebom-$env:GITHUB_REF_NAME-$env:TARGET"
        New-Item -ItemType Directory -Path $stage | Out-Null
        Copy-Item "target/$env:TARGET/release/mikebom.exe" "$stage/mikebom.exe"
        Copy-Item "README.md" "$stage/README.md"
        if (Test-Path "LICENSE") { Copy-Item "LICENSE" "$stage/LICENSE" }
        Compress-Archive -Path $stage -DestinationPath "$stage.zip"
        Get-Item "$stage.zip"

    - name: Upload zip artifact
      uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
      with:
        name: mikebom-${{ github.ref_name }}-x86_64-pc-windows-msvc
        path: mikebom-${{ github.ref_name }}-${{ env.TARGET }}.zip
        if-no-files-found: error
```

**Aggregation job update**: the `release` job (currently `release.yml:250`) has `needs: [build-linux-x86_64, build-linux-aarch64, build-macos-aarch64]`. Add `build-windows-x86_64` to the list.

**SHA256SUMS generation** is at `release.yml:265+`; it `shasum`/`sha256sum`-style aggregates ALL artifact files in the download directory. The new `.zip` is picked up automatically without code changes.

**Alternatives considered**:

- **Use Bash via Git Bash on Windows runner**: rejected — `windows-latest` runners come with PowerShell; cleaner to use it natively than to install Git Bash. PowerShell's `Compress-Archive` produces a valid `.zip` that's standard across Windows extractors.
- **`.tar.gz` on Windows**: rejected — Windows users overwhelmingly expect `.zip` for downloads. PowerShell's built-in extraction handles `.zip` without third-party tools; `.tar.gz` requires `tar` (now bundled in Windows 10 1803+, but inconsistent across older systems).
- **MSI installer**: explicitly out of scope per spec.

## §5 — Path-string sites and emission-site audit (FR-004, FR-007)

**Decision**: 88 `to_string_lossy()` call sites identified across the readers. They populate `PackageDbEntry.source_path` (the internal carrier). The single chokepoint per §2 is sufficient — those internal strings get normalized once when the deduplicator builds the public `ResolvedComponent`.

**JSON-emission sites that read path strings** (3 confirmed):

1. `mikebom-cli/src/generate/cyclonedx/evidence.rs:84` — `"location": o.location` in CDX `evidence.occurrences[]`.
2. `mikebom-cli/src/generate/spdx/annotations.rs:260` — `push(&mut out, "evidence.occurrences", json!(items))` in SPDX 2.3.
3. `mikebom-cli/src/generate/spdx/v3_annotations.rs:272` — same shape for SPDX 3.

**Additional path-field emission** (need verification at implementation time):
- CDX builder's `source_path` field — check if it's emitted to JSON or used only internally.
- SPDX 3's snippet-file references — check if any path strings flow through there.

**Implementer's audit checklist** (T-tasks will enforce):
1. Run `grep -rn 'location\|source_path\|file_path' mikebom-cli/src/generate/` after Phase 1 implementation.
2. For each match, verify the path string was sourced via the chokepoint OR add a defensive `normalize_sbom_path_str(...)` call.

## §6 — POSIX-only tests audit (FR-002, FR-010)

**Decision**: at minimum, gate the milestone-054 symlink-loop tests + any test referencing `/bin/`, `/usr/`, `/etc/`, `/var/` hardcoded paths. Implementer expands the list as Windows CI lane surfaces failures.

**Pre-known POSIX-only tests** (planning-time grep):
- `tests/scan_binary.rs::find_system_binary()` — gracefully skips when `/bin/ls` absent (existing behavior, no gate needed).
- Tests under `binary/mod.rs::tests` that use `std::os::unix::fs::symlink` — already `#[cfg(unix)]`-gated.
- Various `package_db/dpkg.rs::tests` referencing `/var/lib/dpkg/status` — gracefully no-op via the same path-absence check that production uses.
- Milestone-054 `tests/filesystem_walker_*.rs` symlink-loop tests — need to verify gate; some use `std::os::unix::fs::symlink` directly without `#[cfg(unix)]`. **Implementer must add `#[cfg(unix)]` to any test that creates symlinks unconditionally**.

**Iterative discovery**: the Windows CI lane bring-up will produce a list of test failures. The implementer triages each:
- **POSIX-specific behavior the test asserts**: add `#[cfg(unix)]` to the test function.
- **Test bug exposed on Windows** (e.g., assumes forward-slash output before milestone-100's normalization): fix the test.
- **mikebom bug exposed on Windows** (e.g., a code path missing the normalization): fix the code.

The path-normalization-already-applied makes most golden-comparison failures the "test bug" category — fixes are usually adjusting an expected string. The pre-PR gate (T-final) ensures all 3 categories close before PR.

## Coverage map

| Spec section | Resolution |
|--------------|------------|
| FR-001 (Cargo build on Windows) | §1 → no new gates required; existing `#[cfg(unix)]` correctly isolates POSIX code |
| FR-002 (cargo test on Windows) | §6 → POSIX-only tests gated `#[cfg(unix)]`; graceful-skip for missing fixtures |
| FR-003 (clippy clean) | inherits from §1 + §2 + §6 |
| FR-004 (cross-host SBOM byte-identical) | §2 → forward-slash normalization at chokepoint + emission sites |
| FR-005 (cross-format on Windows) | by-construction — no schema changes |
| FR-006 (binary scanner unchanged) | by-construction — `object` crate is cross-platform |
| FR-007 (Linux-only readers compile + no-op) | §1 → existing path-absence checks return empty Vec on Windows |
| FR-008 (CI Windows lane) | §3 → job template defined |
| FR-009 (Windows release artifact) | §4 → build job + zip-packaging defined |
| FR-010 (POSIX tests gated) | §6 → iterative discovery during CI bring-up |
| FR-011 (README update) | new task in tasks.md Phase 6 |
| SC-001 (Windows scan emits valid CDX) | §2 + §3 → Windows CI lane runs scan_outputs test against fixtures |
| SC-002 (cross-format binary scanning) | by-construction |
| SC-003 (cross-host parity) | §2 → forward-slash normalization preserves byte-identity goldens |
| SC-004/SC-005 (clippy + tests clean) | §3 + §6 |
| SC-006 (Windows release artifact) | §4 |
| SC-007 (zero new deps) | inherits |
| SC-008 (zero schema changes) | §2 — only *value* normalization, not *shape* change |

All open spec questions resolved. Ready for Phase 1 (data-model + contracts + quickstart).
