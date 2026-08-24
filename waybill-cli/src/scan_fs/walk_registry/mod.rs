//! Milestone 664 — single-pass filesystem walker + reader-registry.
//!
//! Design: `specs/664-single-pass-walker/`
//!   - spec.md — user stories, FRs, SCs
//!   - plan.md — technical context, constitution check
//!   - research.md — R1..R12 (globset, ReaderId newtype, dispatch model, …)
//!   - data-model.md — entity shapes
//!   - contracts/registry-api.md — 9 contract clauses (C1..C9)
//!   - quickstart.md — reader-migration recipe
//!
//! Post-Phase-2: bodies wired, tests passing. Zero readers migrated yet
//! (US1 starts adding `pub const` declarations in the US1 tasks).

// Some `pub use` re-exports have no in-crate consumers until US1 lands,
// but they're the module's public API surface; suppress the pre-migration
// dead-code / unused-import warnings.
#![allow(dead_code, unused_imports)]

pub(crate) mod dir_index;
pub(crate) mod dispatch;
pub(crate) mod perf_metrics;
pub(crate) mod registry;
pub(crate) mod walk_context;
pub(crate) mod walker;

use std::path::Path;

pub use dir_index::DirIndex;
pub use perf_metrics::WalkerMetrics;
pub use registry::{ReaderRegistry, ReaderRegistryBuilder, ReaderRegistryError};
pub use walk_context::SharedWalkerContext;
pub use walker::SharedWalker;

/// Reader identity — the key into per-reader output aggregation and the
/// value that appears in FR-009 diagnostic logs.
///
/// See `contracts/registry-api.md` §"Public API surface" for the full
/// contract. Per-reader `pub const` declarations land at migration time
/// (US1/US2 tasks). Every new const MUST also be appended to
/// `ALL_READER_IDS` below so the C9 uniqueness test catches drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReaderId(&'static str);

impl ReaderId {
    /// Construct a `ReaderId` from a compile-time string. Used by the
    /// per-reader `pub const` declarations added at migration time.
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    /// The reader's stable string identifier — appears verbatim in the
    /// FR-009 log line's `per_reader_dispatch_counts` field.
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    // ---- Per-reader constants (added at migration time) ----

    /// Haskell reader — matches `*.cabal`, `cabal.project`,
    /// `cabal.project.freeze`, `stack.yaml`, `stack.yaml.lock`,
    /// `package.yaml`. Migrated in milestone-664 US1 T032.
    pub const HASKELL: ReaderId = ReaderId::new("haskell");

    /// Erlang reader — matches `rebar.lock`, `rebar.config`, `*.app.src`.
    /// Migrated in milestone-664 US1 T030.
    pub const ERLANG: ReaderId = ReaderId::new("erlang");

    /// Scala reader — matches `*.sbt.lock`, `build.sbt`,
    /// `project/build.properties`, `project/Dependencies.scala`.
    /// Parent-directory filtering (`project/`) for the last two happens
    /// inside the callback since globs only match basename.
    /// Migrated in milestone-664 US1 T031.
    pub const SCALA: ReaderId = ReaderId::new("scala");

    /// Ipk-file reader — matches `*.ipk` (case-insensitive). First reader
    /// to exercise the `ReaderRegistration.state` extension: carries
    /// `IpkReaderConfig` + a per-scan-computed `distro_tag` derived from
    /// `os_release`. Migrated in milestone-664 US1 T026.
    pub const IPK_FILE: ReaderId = ReaderId::new("ipk_file");

    /// Rpm-file reader — matches `*.rpm` (case-insensitive) with
    /// additional magic-byte check inside the callback. State carries
    /// `RpmReaderConfig` (env-var-populated from `WAYBILL_MAX_RPM_BYTES`
    /// / `WAYBILL_RPM_DISTRO`) + `distro_version` + `os_release_id`.
    /// Migrated in milestone-664 US1 T027.
    pub const RPM_FILE: ReaderId = ReaderId::new("rpm_file");

    /// Pip reader — matches Python project-root markers (`pyproject.toml`,
    /// `poetry.lock`, `Pipfile.lock`, `uv.lock`, `requirements*.txt`).
    /// Callback records the marker file's parent directory as a
    /// candidate project root; post-walker iteration parses lockfiles
    /// + emits main-modules. Migrated in milestone-664 US2 T036.
    pub const PIP: ReaderId = ReaderId::new("pip");

    /// Cargo reader — matches `Cargo.toml` and `Cargo.lock`. Callback
    /// dispatches by basename into two path vectors (manifests +
    /// lockfiles). Post-walker: existing read pipeline consumes both.
    /// Migrated in milestone-664 US2 T037.
    pub const CARGO: ReaderId = ReaderId::new("cargo");

    /// Kotlin DSL (build.gradle.kts) reader — matches
    /// `settings.gradle.kts`, `build.gradle.kts`, `libs.versions.toml`.
    /// libs.versions.toml is nested under `gradle/` so the callback
    /// filters by parent-dir basename. Migrated in milestone-664 US2 T042.
    pub const KOTLIN_DSL: ReaderId = ReaderId::new("kotlin_dsl");

    /// Gradle reader — first reader to use the sibling-lookup pattern
    /// via `on_dir` callback + `ctx.dir_index().contains(...)`. Records
    /// any directory containing at least one Gradle marker
    /// (`build.gradle`, `build.gradle.kts`, `settings.gradle`,
    /// `settings.gradle.kts`). Post-walker: `finalize()` runs the m235
    /// ladder + lockfile parse per dir and mutates `ScanDiagnostics`.
    /// Migrated in milestone-664 US2 T041.
    pub const GRADLE: ReaderId = ReaderId::new("gradle");

    /// Gem reader — matches `*.gemspec` (case-insensitive), `Gemfile`,
    /// `Gemfile.lock`. Callback uses ancestor-path filtering to split
    /// `.gemspec` matches into "top-level" vs "install-tree" (under
    /// `specifications/`) buckets, mirroring the legacy 4 walker sites'
    /// separate outputs. Uses sibling-lookup to enforce FR-007
    /// (gemspec-wins over Gemfile in same dir). Migrated in
    /// milestone-664 US2 T038.
    pub const GEM: ReaderId = ReaderId::new("gem");

    /// Maven reader — matches `pom.xml` + `.jar/.war/.ear`
    /// (case-insensitive per legacy `to_ascii_lowercase` checks). The
    /// two legacy walkers (`find_maven_artifacts` + `find_top_level_poms`)
    /// consolidate into a single registration; the shared walker's
    /// default skip set (which includes `target/`) means poms in
    /// `target/` are no longer visited — matches walker 2's intent
    /// exactly and matches walker 1's intent for every realistic maven
    /// project layout (poms don't live in `target/` in practice — that
    /// dir holds only build outputs). Migrated in milestone-664 US2 T039.
    pub const MAVEN: ReaderId = ReaderId::new("maven");

    /// Npm reader (outer project-root discovery only). Matches 5 npm
    /// project-root markers: `package.json`, `package-lock.json`,
    /// `pnpm-lock.yaml`, `bun.lock`, `yarn.lock`. Callback records
    /// parent directory. FR-005 permanent escape hatch: the INNER
    /// `node_modules/**` walk stays on legacy safe_walk in
    /// `npm/walk.rs::walk_node_modules` — it needs content-driven
    /// bounded descent that the shared walker doesn't provide.
    /// Migrated in milestone-664 US2 T043.
    pub const NPM: ReaderId = ReaderId::new("npm");

    /// Nuget reader — consolidates 3 legacy walker sites (csproj/vbproj/fsproj
    /// project files, `.deps.json` runtime files, `.dll` PE-CLR assemblies)
    /// into one registration. Case-insensitive extension match matches
    /// legacy `eq_ignore_ascii_case`. Migrated in milestone-664 US2 T044.
    pub const NUGET: ReaderId = ReaderId::new("nuget");

    /// Pants shell reader — collects `BUILD` files during the shared
    /// walker's descent, replacing the m225 `discover_build_files` call
    /// inside `pants_shell::read`. The per-target glob-resolver walker
    /// in `pants_shell::target_resolver` stays on legacy `safe_walk`
    /// (it's a per-BUILD-file bounded glob operation, not a scan-tree
    /// discovery walker — analogous to the FR-005 npm inner
    /// `node_modules/**` escape hatch). Migrated in milestone-664 US2 T046.
    pub const PANTS_SHELL: ReaderId = ReaderId::new("pants_shell");

    /// Go binary reader — two-phase migration (T057, resolved 2026-08-23).
    /// The pilot COLLECTS candidate binary paths (files matching size
    /// plus non-intermediate-extension filter); post-pilot `finalize()`
    /// runs the read_binary probe with `claimed_paths` plus
    /// `claimed_inodes` available from OS-package readers. Declares
    /// `descend_into: [build, dist, out, coverage, venv]` per C10 —
    /// Go binaries live in these build-output dirs which the shared
    /// walker skips by default. C10 scoping keeps that visibility
    /// go_binary-only (byte-identity for other readers). Legacy skips
    /// NOT in the shared default (`_`-prefix, `testdata`, `proc`,
    /// `sys`, `go/pkg/mod`) enforced in-callback via
    /// `under_legacy_only_skip_dir`. Registration matches `**/*` (Go
    /// binaries have no reliable filename pattern).
    pub const GO_BINARY: ReaderId = ReaderId::new("go_binary");

    /// Yocto layers reader — collects `.bbappend` files + `conf/layer.conf`
    /// files during the shared descent, replacing the two secondary
    /// yocto walkers (`layer_conf::build_index` + `bbappend::build_from_walk`).
    /// The primary `.bb` recipe walker inside `yocto/recipe::read`
    /// stays on legacy (deferred T029 — full recipe reader migration
    /// is out of scope for this bundle). Legacy skip set matches
    /// shared walker default exactly (both use
    /// `should_skip_default_descent`); no ancestor-path filter needed.
    /// Precomputed paths thread through `SharedPilotOutput` →
    /// `recipe::read`'s new `precomputed_yocto_*_paths` params.
    /// Migrated in milestone-664 US2 T059.
    pub const YOCTO_LAYERS: ReaderId = ReaderId::new("yocto_layers");

    /// Golang legacy reader — collects `go.mod` paths during the
    /// shared descent, replacing the internal `candidate_project_roots`
    /// safe_walk site. Applies legacy-parity ancestor-path filters
    /// (skip if any ancestor named `testdata`, if any ancestor starts
    /// with `_`, or if the 3-component sliding window matches
    /// `go/pkg/mod` — the Go module cache); the shared walker default
    /// already handles `.` prefix, `vendor`, `node_modules`, `target`,
    /// `dist`, `build`, `__pycache__`. Precomputed paths thread
    /// through `DbScanResult.golang_go_mod_paths` to
    /// `golang::legacy::read`'s new `precomputed_go_mod_paths`
    /// parameter. The per-project-root `extract_go_package_main_directory_names`
    /// walker (bounded to one project subtree) stays on legacy —
    /// analogous to FR-005 npm inner-tree escape hatch. Migrated in
    /// milestone-664 US2 T058.
    pub const GOLANG: ReaderId = ReaderId::new("golang");

    /// CMake reader — marker-detect registration. CMake's discovery
    /// is subdir-targeted (only `<scan_root>/CMakeLists.txt` plus
    /// recursive walks of `cmake/` / `Modules/` / `third_party/`),
    /// not a whole-tree scan. Registering here lets the shared walker
    /// gate the O(1) subdir-existence checks on presence of a
    /// `CMakeLists.txt` OR `*.cmake` marker anywhere in the tree.
    /// The subdir-scoped safe_walk calls inside `discover_cmake_files`
    /// stay on legacy (they're targeted glob operations, not tree-
    /// wide discovery — analogous to the FR-005 npm inner
    /// `node_modules/**` escape hatch). Migrated in milestone-664
    /// US2 T056.
    pub const CMAKE: ReaderId = ReaderId::new("cmake");

    /// Bazel reader — marker-detect registration. Fixed-root scan of
    /// `MODULE.bazel` / `WORKSPACE.bazel` / `WORKSPACE` at scan root
    /// only; no tree walker. Registered here so the shared walker
    /// gates the up-to-three-file read on presence of any Bazel marker
    /// seen in the tree. Byte-identity preserved via fs-existence
    /// fallback at finalize. Migrated in milestone-664 US2 T055.
    pub const BAZEL: ReaderId = ReaderId::new("bazel");

    /// Conan reader — marker-detect registration. Fixed-root scan of
    /// `conanfile.txt` + `conanfile.py` at scan root only; no tree
    /// walker. Registered here so the shared walker gates the two-file
    /// read on presence of either marker seen anywhere in the tree.
    /// Byte-identity preserved via an fs-existence fallback at
    /// finalize. Migrated in milestone-664 US2 T054.
    pub const CONAN: ReaderId = ReaderId::new("conan");

    /// Vcpkg reader — marker-detect registration. The reader itself
    /// has no tree walker (fixed-root scan of `vcpkg.json` at scan
    /// root only); registering it here lets the shared walker gate
    /// the one-file read on presence of a `vcpkg.json` marker seen
    /// anywhere in the tree. Byte-identity preserved via an existence-
    /// check fallback at finalize. Migrated in milestone-664 US2 T053.
    pub const VCPKG: ReaderId = ReaderId::new("vcpkg");

    /// Swift reader — sibling-lookup registration. Uses `on_dir` +
    /// `ctx.dir_index().contains(dir, "Package.resolved")` /
    /// `Package.swift` to detect SwiftPM project roots during the
    /// single-pass descent. The legacy `.build` skip is covered by
    /// the shared walker's leading-`.` default; no ancestor-path
    /// filter needed. Migrated in milestone-664 US2 T052.
    pub const SWIFT: ReaderId = ReaderId::new("swift");

    /// Elixir reader — consolidates 2 legacy walker sites (`mix.lock`
    /// plus `mix.exs`) into one registration. The legacy skip set adds
    /// `_build` / `deps` / `priv` / `cover` on top of the shared
    /// walker's default (leading-`.` plus `build`, `dist`, `node_modules`,
    /// `target`, `out`, …); these are enforced via ancestor-path
    /// filtering in the callback so nested `mix.exs` inside
    /// `deps/<pkg>/` (Elixir's per-project dep vendor tree) aren't
    /// double-emitted. Migrated in milestone-664 US2 T051.
    pub const ELIXIR: ReaderId = ReaderId::new("elixir");

    /// Dart reader — collects `pubspec.yaml` manifests during the
    /// shared descent. Legacy skip set (`.dart_tool`, `.pub-cache`,
    /// `build`, `.git`, `.hg`, `.svn`, `node_modules`) is a strict
    /// subset of the shared walker's default (`. prefix + build +
    /// node_modules + …`) — no ancestor-path filter needed in the
    /// callback. Migrated in milestone-664 US2 T050.
    pub const DART: ReaderId = ReaderId::new("dart");

    /// Composer reader — collects `composer.json` manifests during the
    /// shared descent (`vendor/` is in the shared walker's default
    /// skip set — matches the legacy `should_skip_manifest_descent`
    /// exactly). Case-insensitive filename match preserves legacy
    /// `eq_ignore_ascii_case` behavior. The `installed.json` walker
    /// stays on legacy `safe_walk` because installed.json lives at
    /// `vendor/composer/installed.json` — under `vendor/` which the
    /// shared walker skips by default. Wiring composer's Pass B into
    /// the shared walker would require a per-registration override on
    /// the default skip set (a bigger walker-API change; deferred).
    /// Migrated in milestone-664 US2 T049.
    pub const COMPOSER: ReaderId = ReaderId::new("composer");

    /// Cocoapods reader — consolidates 3 legacy walker sites
    /// (`Podfile.lock` project-root discovery, `Pods/Manifest.lock`
    /// deployed-tier discovery, `Podfile` design-tier discovery) into
    /// one registration. Ancestor-path filtering inside the callback
    /// preserves the legacy per-walker skip predicates (`Pods/` skip
    /// for Podfile.lock + Podfile; `DerivedData/` skip on all three).
    /// Migrated in milestone-664 US2 T048.
    pub const COCOAPODS: ReaderId = ReaderId::new("cocoapods");

    /// Pants Go reader — enrichment-only registration. Collects `BUILD`
    /// files during the shared walker's descent so `pants_go::enrich`
    /// (called from `scan_fs/mod.rs` outside `read_all`) can reuse the
    /// pilot's precomputed set instead of double-walking. The
    /// fixed-root `pants_go::read` (reads `pants.toml` at scan root
    /// only) is not walker-driven and stays as-is. Migrated in
    /// milestone-664 US2 T047 (bundle closer for the deferred T028
    /// pants_common consolidation).
    pub const PANTS_GO: ReaderId = ReaderId::new("pants_go");

    /// Pants coursier-JVM reader — marker-detect registration.
    /// The reader itself has no tree walker (fixed-root scan of
    /// `3rdparty/jvm/*.lock` + `pants.toml`); registering it here lets
    /// the shared walker gate the two-fs-call read on presence of a
    /// pants signal (either `pants.toml` anywhere in the tree, or a
    /// `*.lock` under a `3rdparty/jvm/` directory). Byte-identity
    /// preserved via an existence-check fallback at finalize when the
    /// walker saw no signal but the layout is a pathological corner
    /// case. Migrated in milestone-664 US2 T045.
    pub const PANTS_JVM: ReaderId = ReaderId::new("pants_jvm");
}

impl std::fmt::Display for ReaderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Compile-time list of every declared `ReaderId::*` const, kept in sync
/// with each US1/US2 reader-migration PR. Contract C9 uniqueness test
/// iterates this slice and asserts pairwise-distinct `&'static str`
/// values. Adding a new `pub const` to `ReaderId` WITHOUT appending it
/// here is a bug the test won't catch — the reader-migration PR checklist
/// in `quickstart.md` step 1 flags this.
///
/// Post-Phase-2 (this file's current state): empty. US1 T026-T032 add
/// the seven pilot readers.
pub(crate) const ALL_READER_IDS: &[ReaderId] = &[
    ReaderId::HASKELL,
    ReaderId::ERLANG,
    ReaderId::SCALA,
    ReaderId::IPK_FILE,
    ReaderId::RPM_FILE,
    ReaderId::PIP,
    ReaderId::CARGO,
    ReaderId::KOTLIN_DSL,
    ReaderId::GRADLE,
    ReaderId::GEM,
    ReaderId::MAVEN,
    ReaderId::NPM,
    ReaderId::NUGET,
    ReaderId::PANTS_JVM,
    ReaderId::PANTS_SHELL,
    ReaderId::PANTS_GO,
    ReaderId::COCOAPODS,
    ReaderId::COMPOSER,
    ReaderId::DART,
    ReaderId::ELIXIR,
    ReaderId::SWIFT,
    ReaderId::VCPKG,
    ReaderId::CONAN,
    ReaderId::BAZEL,
    ReaderId::CMAKE,
    ReaderId::GOLANG,
    ReaderId::YOCTO_LAYERS,
    ReaderId::GO_BINARY,
];

/// Per-file callback signature. Fires when the shared walker visits a
/// file whose basename matches one of the reader's registered patterns.
pub type FileCallback = fn(&Path, &SharedWalkerContext<'_>);

/// Per-directory callback signature. Fires once per directory the shared
/// walker descends into, AFTER the directory's contents are indexed.
/// Used by two-phase readers that need per-project-root logic.
pub type DirCallback = fn(&Path, &SharedWalkerContext<'_>);

/// Reader interest declaration. See `data-model.md` §"ReaderRegistration".
///
/// The `state` field carries opaque per-scan configuration for readers
/// that today take extra `read()` parameters beyond `(rootfs, exclude_set)`
/// (e.g. `ipk_file` with `IpkReaderConfig` + a per-scan-computed
/// `distro_tag`, or `rpm_file` with `RpmReaderConfig` populated from
/// env vars). Callbacks retrieve their state via
/// `SharedWalkerContext::state::<T>(reader_id)`. Added at Phase 3 US1
/// implementation time when the first migration surfaced the need
/// (documented as a spec deviation in tasks.md Phase 3 checkpoint).
#[derive(Debug)]
pub struct ReaderRegistration {
    pub reader_id: ReaderId,
    pub patterns: globset::GlobSet,
    /// Optional per-scan state. `None` for readers that need no extra
    /// input beyond what `SharedWalkerContext` already exposes.
    pub state: Option<std::sync::Arc<dyn std::any::Any + Send + Sync>>,
    pub on_file: Option<FileCallback>,
    pub on_dir: Option<DirCallback>,
    /// Milestone 664 US4 (post-2026-08-23 API extension) — contract C10.
    ///
    /// Directory-basename patterns the reader WANTS to descend into
    /// even when the shared walker's default skip set would block them.
    /// The walker consults this at subdir-descent time: if any
    /// registration's `descend_into` matches a normally-skipped
    /// directory basename, the walker descends anyway.
    ///
    /// **Scoping**: dispatch under a descended-only subtree is
    /// restricted to the set of readers whose `descend_into` opened
    /// the door. Non-requesting readers do NOT receive dispatch under
    /// such subtrees — preserves byte-identity for the 21 already-
    /// migrated readers whose skip-set didn't include the descended-
    /// into dir.
    ///
    /// Usage: T039 maven (`target/` descent for jar walker), T057
    /// go_binary (`build/`/`dist/`/`out/`/`coverage/`/`venv/` descent).
    ///
    /// `None` (the common case) means "respect the walker's default
    /// skip set unchanged."
    pub descend_into: Option<globset::GlobSet>,
}

/// Reader ergonomics helper — compile a list of glob patterns into a
/// `GlobSet`. Used by `<reader>::registration()` sites per quickstart.md.
pub fn globset_from_patterns(patterns: &[&str]) -> anyhow::Result<globset::GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pat in patterns {
        let glob = globset::Glob::new(pat)
            .map_err(|e| anyhow::anyhow!("invalid glob pattern {:?}: {}", pat, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build GlobSet: {}", e))
}

/// Case-insensitive variant of `globset_from_patterns`. Used by readers
/// that historically matched file extensions via `str::eq_ignore_ascii_case`
/// (ipk, rpm, jar, etc.) — preserves that behavior when migrating to
/// glob-based dispatch.
pub fn globset_from_patterns_case_insensitive(patterns: &[&str]) -> anyhow::Result<globset::GlobSet> {
    let mut builder = globset::GlobSetBuilder::new();
    for pat in patterns {
        let glob = globset::GlobBuilder::new(pat)
            .case_insensitive(true)
            .build()
            .map_err(|e| anyhow::anyhow!("invalid glob pattern {:?}: {}", pat, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|e| anyhow::anyhow!("failed to build GlobSet: {}", e))
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// T010 — contract C9: every declared `ReaderId::*` const is unique.
    /// Post-Phase-2 the slice is empty so this test is vacuously true;
    /// as US1/US2 migrations add consts + entries here, uniqueness is
    /// automatically enforced.
    #[test]
    fn all_reader_ids_are_unique() {
        let mut seen: HashSet<&'static str> = HashSet::new();
        for id in ALL_READER_IDS {
            assert!(
                seen.insert(id.as_str()),
                "duplicate ReaderId string encountered: {:?}",
                id.as_str(),
            );
        }
    }

    #[test]
    fn globset_helper_compiles_valid_patterns() {
        let gs = globset_from_patterns(&["**/Cargo.toml", "**/*.lock"]).unwrap();
        assert!(gs.is_match("path/to/Cargo.toml"));
        assert!(gs.is_match("Cargo.lock"));
        assert!(!gs.is_match("random.txt"));
    }

    #[test]
    fn globset_helper_rejects_invalid_pattern() {
        // Unbalanced `[` is a globset parse error.
        let err = globset_from_patterns(&["[unclosed"]).unwrap_err();
        assert!(err.to_string().contains("invalid glob pattern"));
    }

    #[test]
    fn reader_id_string_roundtrip() {
        let id = ReaderId::new("test-reader");
        assert_eq!(id.as_str(), "test-reader");
        assert_eq!(format!("{}", id), "test-reader");
    }
}
