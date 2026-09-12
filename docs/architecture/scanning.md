# Scanning

The scan layer turns a filesystem tree (or an extracted container image) into
a set of candidate components. It is the left-hand entry point of the
pipeline for `sbom scan`; for `trace run` / `sbom generate`, the equivalent
role is played by the [resolution pipeline](resolution.md) reading attestation
events.

**Key files:**

- `waybill-cli/src/scan_fs/mod.rs` — scan entry point (`scan_path`), ecosystem
  orchestration, relationship resolution, generation-context selection.
- `waybill-cli/src/scan_fs/walker.rs` — generic directory walker, per-file
  streaming SHA-256, artifact-suffix filtering, size cap.
- `waybill-cli/src/scan_fs/docker_image.rs` — `docker save` tarball extractor:
  layer merging, OCI whiteout handling, os-release reader.
- `waybill-cli/src/scan_fs/os_release.rs` — `/etc/os-release` + fallback
  `/usr/lib/os-release` parser. Reads `ID` + `VERSION_ID` and populates the
  `distro=<namespace>-<VERSION_ID>` PURL qualifier shared by deb, rpm, and
  apk (e.g., `distro=debian-12`, `distro=ubuntu-24.04`, `distro=alpine-3.19`).
- `waybill-cli/src/scan_fs/package_db/*.rs` — one module per ecosystem.

## The three evidence sources

Per-component evidence falls into one of three categories, ordered by trust:

| Source | Technique | Confidence | Who knows it |
|---|---|---|---|
| **Installed-package DB** | `PackageDatabase` | 0.85 | dpkg, apk, rpm sqlite, npm lockfile, Cargo.lock, go.sum, Gemfile.lock, Poetry/Pipfile — the OS or package manager's authoritative record of what *is* installed (or should be per the lock). |
| **Artifact file** | `FilePathPattern` / `filename` | 0.70 | waybill, via directory walk + SHA-256. The file physically exists on disk with matching bytes. |
| **External lookup** | `HashMatch` (deps.dev) | 0.90 | deps.dev, consulted with a content hash pulled from an attestation's TLS response. Only active in `sbom generate` / trace mode. |

The walker stream-hashes every file whose extension matches one of the
recognised artifact suffixes (`.deb`, `.crate`, `.whl`, `.jar`, `.gem`,
`.apk`, `.rpm`, `.tar.gz`, …); these flow to `FilePathPattern` resolution. The
package-DB modules do not stream-hash — their authority is the package
manager's bookkeeping, not the bytes.

## Container-image scanning (`--image`)

`scan_fs::docker_image::extract` handles both formats `docker save` can
produce:

- Legacy `layer/layer.tar` format (pre-buildkit images).
- Modern `blobs/sha256/<digest>` OCI form.

Layers are extracted in manifest order into a tempdir; OCI whiteout files
(`.wh.<name>`) and opaque directories (`.wh..wh..opq`) suppress files and
directories from lower layers. The result is a rootfs-shaped tempdir that the
scanner processes as if `--path <tempdir>` had been passed. `/etc/os-release`
(with fallback to `/usr/lib/os-release`) is read to auto-detect the distro
identity — `ID` + `VERSION_ID` become the `distro=<namespace>-<VERSION_ID>`
PURL qualifier (e.g., `distro=debian-12`). See
[PURL canonicalization](purls-and-cpes.md) for the full rule — the same
shape applies across deb, rpm, and apk so downstream consumers don't need
per-ecosystem branching.

## Per-ecosystem detection

Each `package_db/*.rs` module knows how to find packages in its ecosystem's
idiom. The coverage matrix:

| Ecosystem | Module | Primary source | Dep-graph source | Notes |
|---|---|---|---|---|
| apk | `apk.rs` | `/lib/apk/db/installed` stanzas (P/V/A/D keys) | DB (direct `D:` only) | Alpine apk |
| cargo | `cargo.rs` | `Cargo.lock` v1-v4 | Lockfile | Includes legacy roots and metadata checksums |
| deb | `dpkg.rs` + `copyright.rs` + `file_hashes.rs` | `/var/lib/dpkg/status` + per-file `.list` manifests | DB (`Depends:`) | Optional deep per-file SHA-256 |
| gem | `gem.rs` | `Gemfile.lock` indent structure + `specifications/*.gemspec` | Lockfile indent-6 | Gemspec walker catches stdlib gems |
| golang | `golang.rs` + `go_binary.rs` | `go.mod` / `go.sum` / `$GOMODCACHE/cache/download/<escaped>/@v/<v>.mod` walker; `runtime/debug.BuildInfo` for Go 1.18+ binaries | Cache walker (source); **none** for binaries | Pre-1.18 binaries: `buildinfo-status=unsupported` |
| maven | `maven.rs` | Project `pom.xml`, JAR `META-INF/maven/.../pom.properties,pom.xml`, `~/.m2/repository/.../*.pom`, `/usr/share/maven-poms/` sidecar | Layered: project → JAR-embedded → M2 BFS → parent POM chain → deps.dev → empty | Most complex ecosystem — see [Dep-graph resolution strategy (Maven)](../ecosystems.md#dep-graph-resolution-strategy-maven). Shade-plugin fat-jars also parsed via `META-INF/DEPENDENCIES` with bytecode-presence gating (feature 009); see [`ecosystems.md#maven`](../ecosystems.md#maven). |
| npm | `npm.rs` | `package-lock.json` v2/v3, `pnpm-lock.yaml`, `node_modules/` | Lockfile | v1 rejected |
| pip | `pip.rs` | venv `dist-info/METADATA`, Poetry/Pipfile locks, `requirements.txt --hash=` | Poetry/Pipfile (full), venv flat | `requirements.txt --hash=alg:hex` flags captured for per-component integrity |
| rpm | `rpm.rs` + `rpmdb_sqlite/` | `/var/lib/rpm/rpmdb.sqlite` (pure-Rust reader) | DB (`REQUIRES`) | BDB `Packages` format detected but not parsed — diagnostic log, zero components (flag `--include-legacy-rpmdb` threads through for future BDB reader) |

See [`ecosystems.md`](../ecosystems.md) for per-ecosystem detail including
PURL format notes, hash sources, and known limitations.

## Relationship resolution

Package DBs emit dependency edges alongside components. `scan_fs::mod.rs` runs
a post-pass over the combined relationship list to:

1. Drop relationships whose source or target isn't in the deduplicated
   component set (dangling-target filter). Dangling edges typically come from
   `Depends:` lines referencing virtual packages or packages outside the
   install set.
2. Normalize names for cross-package equality (see the `normalize_dep_name`
   function and the [same-artifactId-different-groupId note in
   the maven section](../ecosystems.md#maven)).

## Container- vs. path-mode differences

`scan_cmd.rs` tracks a `ScanMode` enum (`Image` vs. `Path`) that flows down
through the pipeline. The only feature currently gated on it is feature 005's
npm internals filtering: inside an extracted image, `node_modules/npm/node_modules/**`
entries are marked `waybill:npm-role = internal`; in path mode they are
filtered out before resolution. Future scan-mode-aware logic (e.g. treating
`node_modules/` as authoritative vs. derivable) hooks onto the same enum.

## Generation context

Scan mode stamps one of three `GenerationContext` values on the output:

- `FilesystemScan` — `--path <dir>` where the dir is not a rootfs-shaped tree.
  Most filesystem scans (cache directories, source trees).
- `ContainerImageScan` — `--image <tar>`. The scanner extracts the tarball
  and stamps this context.
- `BuildTimeTrace` — never stamped by `sbom scan`; see
  [generation.md](generation.md) for where this value comes from.

This value lands at the top of the CycloneDX BOM under
`metadata.component.properties.waybill:generation-context` so downstream
consumers know what kind of evidence produced the SBOM.

## Trace-mode compiler-pipeline enrichment (milestone 210)

When the trace captures compiler invocations, the SBOM projection gains a
per-component source attribution layer independent of the ecosystem-
reader path above:

1. **In-kernel capture** — three eBPF tracepoints on
   `sched_process_{exec,fork,exit}` recognize whitelisted compilers,
   assign monotonic `invocation_id`s via `bpf_ktime_get_ns`, and
   propagate PID ancestry through the `COMPILER_INVOCATIONS` HashMap so
   `rustc` invocations spawned by `cargo` inherit the cargo parent's
   invocation id.
2. **User-space aggregation** — `waybill-cli/src/trace/compiler_pipeline.rs`
   drains the ring buffer, buckets file-open events per invocation into
   `read_set` + `write_set` bags, applies FR-016 trace-noise filters
   (system dirs, user cache, ephemeral tmp, secrets-adjacent paths), and
   assembles the finalized `CompilerPipelineData` at scan-end.
3. **Attestation injection** — the aggregated data lands as an additive
   `Option<CompilerPipelineData>` field on `BuildTracePredicate`; the
   `Option` shape guarantees byte-identity for pre-m210 attestation
   goldens when the field is absent (scan-mode + traces without any
   compiler exec).
4. **Per-component annotation** — at SBOM emission time,
   `waybill-cli/src/generate/compiler_pipeline_annotation.rs::map_component_to_source_read_set`
   walks each `ResolvedComponent`'s known file paths (m133 evidence +
   `occurrences[].location`) and intersects them against every
   invocation's write-set. Matches produce C130
   `waybill:source-read-set` (transitive-closed union of the matched +
   ancestor read-sets, deterministically sorted) + C131
   `waybill:read-set-source = "traced"`. Non-matching components get
   C131 = `"unknown"` only.
5. **Document-scope transparency** — three companion annotations ride
   along per contracts/annotations.md: C132
   `waybill:compiler-pipeline-completeness` (always emitted; carries the
   `CompletenessState` shape), C133 `waybill:secrets-read-filtered`
   (emitted when non-zero), and C134 `waybill:trace-attach-late` (per-
   component when the doc-scope state is `Partial(AttachLate)`).

The end-to-end data flow: eBPF program → `COMPILER_INVOCATIONS` map →
ring buffer → user-space aggregator → `CompilerPipelineAggregator` →
`BuildTracePredicate.compiler_pipeline` → per-component + doc-scope
annotations on all three SBOM formats. Complete details live in
[attestations.md](attestations.md#compiler_pipeline-compilerpipelinedata-milestone-210).

## Shared-walker reader registry (milestone 664)

Milestone 664 introduced a single-pass filesystem walker + reader-registry that consolidates ~28 ecosystem-reader `safe_walk` calls into ONE tree traversal per scan. The perf motivation: prior to m664, scanning a mongo/pytorch/ansible checkout walked the source tree 20+ times (once per ecosystem reader). Post-m664, the shared walker traverses once and dispatches per-file matches to every interested reader's callback in registration order.

### What the shared walker is

- `waybill-cli/src/scan_fs/walk_registry/` — the crate module.
  - `walker.rs::SharedWalker` — the single-pass tree walker. Inherits `safe_walk`-equivalent semantics: canonicalize-keyed visited-set (m054 loop guard), m113 `ExclusionSet`, m114 permissive canonicalize/read_dir errors.
  - `registry.rs::ReaderRegistry` — the dispatch table. Each `ReaderRegistration` carries a `globset::GlobSet` of filename patterns plus optional `on_file` + `on_dir` callbacks and opaque per-scan `state` (an `Arc<dyn Any>` slot).
  - `dispatch.rs` — the per-file/per-dir dispatch loop. Iterates registrations in insertion order (contract C1); every callback runs inside `catch_unwind` for panic isolation (contract C4).
  - `dir_index.rs::DirIndex` — the in-memory (directory → sorted-filenames) map that reader callbacks consult for sibling-lookup (FR-003 / Clarify Q1 zero-extra-syscalls contract).
  - `walk_context.rs::SharedWalkerContext` — the reader-facing handle exposing `dir_index()`, `exclude_set()`, `push(reader_id, entry)`, and typed state retrieval via `state::<T>(reader_id)`.

### How a reader migrates

Full walkthrough at `specs/664-single-pass-walker/quickstart.md`. Short form:

1. **Add a `ReaderId::YOUR_READER` const** in `walk_registry/mod.rs` plus append it to `ALL_READER_IDS` (contract C9 uniqueness).
2. **Add a `YourReaderDiscoveredPaths` state struct** in `<reader>.rs` — usually one `Vec<PathBuf>` per legacy walker site.
3. **Write the `on_file` (or `on_dir`) callback** that filters + pushes into state.
4. **Write `registration()` + `extract_paths()` + `finalize(paths, ...)` helpers**.
5. **Refactor `pub fn read()` → `#[allow(dead_code)]` shim** that calls `pants_common::discover_build_files`-style safe_walk then delegates to `finalize` (retained per FR-004 coexistence for test paths).
6. **Wire into `SharedPilotOutput` + `run_shared_walker_pilot`** in `waybill-cli/src/scan_fs/package_db/mod.rs`.
7. **Swap `read_all`** — `<reader>::read(...)` becomes `std::mem::take(&mut shared_pilot.<reader>)`.
8. **Verify FR-006 byte-identity**: 5017/0 test suite + walker-audit diff empty + goldens byte-identical.

For readers whose skip set is a strict subset of the shared walker default (e.g., dart, elixir), no filter is needed in the callback. Readers with legacy-only skip additions (`_`-prefix, `testdata`, `Pods/`, `DerivedData/`, `deps/`, `go/pkg/mod`) use ancestor-path filtering inside the callback to preserve byte-identity.

### FR-005 escape-hatch conditions

A walker CANNOT migrate to the shared registry when it's one of:

- **Per-project-anchored** (bounded to a subtree already discovered by the outer walker; e.g., npm's `walk_node_modules`, pants_shell's per-target glob resolver, golang's per-project `package main` enumeration).
- **Non-scan-tree** (walks a cache like `~/.m2/`, `~/.cache/go-build/`, or an archive-internal structure via the `zip` crate).
- **Descend-into-required** (needs to enter dirs the shared walker skips by default: `vendor/`, `target/`, `build/`, `dist/`, `venv/`). The per-registration `descend_into` API extension (contract C10) landed 2026-08-23 and same-day unblocked **T039 maven** (target/ descent via `descend_into: [target]` + ancestor-path filter in `finalize` for the top-level-pom semantic) plus **T057 go_binary** (build-output dirs via `descend_into: [build, dist, out, coverage, venv]` + a two-phase candidate-collection pilot pattern for `claimed_paths` availability from OS-package readers). **All three originally-deferred m664 readers (T029/T039/T057) are now resolved** — every production-path scan-tree walker uses the shared walker.

The `descend_into` API (contract C10 in `contracts/registry-api.md`) adds a `Option<globset::GlobSet>` field to `ReaderRegistration`. When set, the walker descends into normally-skipped dirs whose basename matches. **Byte-identity guarantee**: dispatch under a descended-only subtree is scoped to the reader(s) whose `descend_into` opened the door — non-requesting readers never see files under that subtree. This preserves the 21 already-migrated readers' skip semantics without any code change on their side.

Every escape-hatch site carries an inline `// FR-005 permanent escape hatch — <reason>` annotation citing its milestone number. The CI walker-audit gate (T065) points contributors to the T064 rationale doc at `waybill-cli/src/scan_fs/walk.audit-allowlist.rationale.md` which classifies every retained allowlist entry into (A) FR-005 escape hatch / (B) deferred reader / (C) non-scan-tree walker / (D) shared-walker infrastructure.

### Reference

- Spec: `specs/664-single-pass-walker/spec.md`
- Migration guide: `specs/664-single-pass-walker/quickstart.md`
- API contracts (C1–C9): `specs/664-single-pass-walker/contracts/registry-api.md`
- Data model: `specs/664-single-pass-walker/data-model.md`
- Rationale for retained walker-audit entries: `waybill-cli/src/scan_fs/walk.audit-allowlist.rationale.md`
- SC-005 microbenchmark: `waybill-cli/tests/perf_walk_dispatch.rs::sc005_synthetic_10k_file_tree_p95_dispatch_overhead`

### Operator-controlled reader gating (milestone 665)

The registration site inside `run_shared_walker_pilot` is also an operator-visible gate. Milestone 665 (`--no-binary-scan=<MODE>` / `WAYBILL_NO_BINARY_SCAN=<MODE>`) uses this seam to elide `go_binary::registration()` at pilot time when the operator opts out of statically-linked-Go BuildInfo probing — trading module attribution for wall-time on large trees (mongo 3.04s → ~0.7s). The gate is a one-line `if !skip_go_binary { ... }` around the existing `register("go_binary", ...)` call; `finalize()` downstream self-elides on the empty candidate-path list, so no separate suppression is needed at the post-pilot site. Setting the flag also emits a document-scope `waybill:binary-scan-suppressed=<mode>` annotation (C153) across CDX / SPDX 2.3 / SPDX 3 so downstream consumers can detect the opt-out without inspecting waybill invocation state. See `specs/665-no-binary-scan-flag/`.

The design intent: `run_shared_walker_pilot` becomes the natural place to gate reader participation by operator preference (or, future, by scan-tier / trust-level policy). The m665 pattern is extensible — the `BinaryScanMode` enum reserves `all` / `elf` / `symbols` variants for future opt-outs of m096 ELF section reader, m099 symbol fingerprint, and m104 binary-role classification without new C-rows.

---

*Moved here from `docs/architecture/overview.md` when that document was retired (#827). That document described the walker three times — milestone 054, 114 and 664 — in three separate sections, and the oldest was written in MUST language describing hand-rolled per-walker loop protection that m114 centralised and m664 replaced outright. Only the current design is reproduced here.*
