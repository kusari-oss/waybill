# Research: dynamic linkage and embedded source paths are labelled binary analysis

Observations are at `main` `33c445f6` (branch base `3a12bf45`). "Read" means read from code, "measured" means executed, and anything else is labelled inference.

## R1. The kinds a binary reader emits, and the one decision (read)

`scan_fs/binary/mod.rs:68` already defines `BINARY_EVIDENCE_KINDS`, the list of every evidence kind a binary reader emits. Its drift test (`binary_evidence_kind_drift`) fails when `entry.rs`, `linkage.rs`, `jdk_collapse.rs` or `python_collapse.rs` emits an unlisted literal, and it has a control that proves the scan finds literals. The list:

| Kind | Read from | Decision (FR-002, FR-003, FR-006) |
|---|---|---|
| `embedded-version-string` | a version banner in the binary's read-only data | `binary-analysis` (spec 1646) |
| `symbol-fingerprint` | the binary's exported symbols | `binary-analysis` (spec 1646) |
| `dynamic-linkage` | a `DT_NEEDED` entry in the binary's dynamic section | **`binary-analysis`** |
| `embedded-source-path` | source paths compiled into the binary (spec 1534) | **`binary-analysis`** |
| `cargo-auditable` | the `.dep-v0` section the Rust toolchain wrote | `manifest-analysis`, unchanged |
| `elf-note-package` | the `.note.package` section a distribution's build wrote | `manifest-analysis`, unchanged |
| `jdk-runtime-collapsed` | the **path** of JDK files (`/usr/lib/jvm/java-<N>-…`), not their content (`jdk_collapse.rs` module doc) | `manifest-analysis`, unchanged |
| `python-stdlib-collapsed` | the **path** of CPython files (`.cpython-<ver>-*.so`, `libpython*`) (`python_collapse.rs` module doc) | `manifest-analysis`, unchanged |

The two collapse kinds are outside #1668 and the spec. They identify a runtime by file-path layout, which is neither binary analysis nor a package manager. CycloneDX's `filename` technique fits them better (inference). This spec only records their decision as "unchanged" so the decision is total. It does not relabel them, and filing the `filename` question as a follow-up is left to the close-out.

Go BuildInfo components carry no evidence kind (`package_db/go_binary.rs::emit_entries_from_info` sets `evidence_kind: None`), so they are not in the list. They keep `manifest-analysis` without any change.

**Decision**: add `binary_kind_technique(kind: &str) -> Option<ResolutionTechnique>` next to `BINARY_EVIDENCE_KINDS`:
- `BinaryAnalysis` for the four binary-analysis kinds;
- `PackageDatabase` for the four unchanged kinds;
- `None` for a kind not in the list.

A unit test asserts that every entry of `BINARY_EVIDENCE_KINDS` returns `Some`. So a new kind, which the drift test already forces into the list, cannot be added without a decision (FR-006).

Both current users of spec 1646's `resolve/deduplicator.rs::BINARY_KINDS` switch to it:
- `scan_fs/mod.rs::entry_technique` (and `nix/closure/contents.rs::component_from_entry`) take `binary_kind_technique(kind).unwrap_or(PackageDatabase)`;
- the deduplicator's `effective_technique` and `is_binary_side` take `== Some(BinaryAnalysis)`.

`BINARY_KINDS` is then removed: one list instead of two.

**Alternatives considered**:
- Extending `BINARY_KINDS` with the two kinds would keep two hand-maintained lists of overlapping strings, and the second has no drift test.
- An `EvidenceKind` enum would give compiler exhaustiveness, but it would touch every reader's `evidence_kind` field (29 literal kinds across 18 files). That is out of proportion to this change.

## R2. Merges: one gate (decision)

Spec 1646's absorption runs when either side's effective technique is `BinaryAnalysis`. With R1, that holds automatically for the two new kinds (FR-004).

A merge needs equal PURLs:
- A `dynamic-linkage` component is `pkg:generic/<soname>`, for example `pkg:generic/libc.so.6`.
- A package-database record is `pkg:deb/…`, `pkg:rpm/…` and so on, so it never merges with one.
- A pkg-config record is `pkg:generic/<pc-name>@<version>`, which could, if its name equals the soname.
- An `embedded-source-path` component names the bundled package (`pkg:generic/<name>@<version>`), which could equal a pkg-config or CMake record.

So merges are possible but expected to be rare (inference). FR-001 counts them.

## R3. Correcting the Go census label (read)

`census_entry_technique` (`scan_fs/mod.rs:281-300`) labels an entry `go-buildinfo` when `detected_go == Some(true)`. Two sources set that flag:
- `scan_fs/binary/entry.rs:1412`, on the Go binary's **file-level** component;
- `golang/legacy.rs:1076`, on a Go **source-tree** main module.

The components read from BuildInfo (`go_binary.rs::emit_entries_from_info`) set neither. So spec 1646's 38 and 1 count file-level and source components, not build info (inference; the census measures it).

**Decision**: the log-only commit relabels that census value `detected-go`, which is what it measures, and adds an origin-based count. `emit_entries_from_info` emits a `binary_derived_kind` event with kind `go-buildinfo` for each entry it creates, carrying the PURL and the binary's path. The census records both counts side by side (FR-005). Nothing about a Go component's output changes.

## R4. The log-only census (decision, as in spec 1646)

The log-only commit changes no output. It adds:
- **`technique_change`**, reusing spec 1646's event name and shape: emitted at conversion for each entry whose `binary_kind_technique` differs from the technique it is given. On the log-only commit that is every `dynamic-linkage` and `embedded-source-path` entry.
- **`kind_merge`**: emitted at spec 1646's merge sites S1 and S2 when either side's kind is one that R1 newly makes binary, with both sides' kind and mechanism. These are the merges whose methods would change.
- **`go-buildinfo`**, as in R3.

At the wired commit, `technique_change` and `kind_merge` are removed, as spec 1646 removed `technique_change`. Spec 1646's permanent `binary_merge` then reports the absorptions.

## R5. Where to count (decision)

- **TheRock and the quality corpus**: `census-versions.yml` in kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main` with full SHAs, with every `refs.txt` verified. `census/versions.py` parses `kind_merge` and the relabelled kinds.
- **Public corpus**: the log-only commit must pass 166 of 166. At the wired commit, every golden difference is traced. None is expected: the goldens carry no `dynamic-linkage` or `embedded-source-path` component (spec Background).
- **Positive control, in CI**: the `control` job adds real builds on the runner:
  - a dynamically linked C program (`gcc`), which gives `dynamic-linkage`;
  - a small Go program (`go build`), which gives Go build info.

  It asserts their census events in the log-only and wired states.
- **Positive control, in the repository**: the integration test file (R6) covers every kind in the list with synthetic fixtures, including `embedded-source-path`, `cargo-auditable` and `elf-note-package`, which need no toolchain.

## R6. Fixtures (decision)

A new `waybill-cli/tests/binary_kind_technique.rs` scans one synthetic `waybill-fixture-*` input per kind in all three formats, and asserts the technique and the census event. It reuses existing builders:
- `elf_with_package_note` (`python_owned_binaries.rs`) for `elf-note-package`;
- the `.dep-v0` cargo-auditable fixture (`closure_binary_contents.rs`);
- the embedded-source-path tree (`installed_lib_embedded_paths.rs`);
- a synthetic Go BuildInfo binary (the builder in `go_binary.rs`'s unit tests);
- `elf_shared_lib` with a version banner, the spec 1646 baseline.

`dynamic-linkage` today is tested only against a real `/bin/ls`. A test-only `elf_with_needed(needed: &[&str])` is added to `tests/common/installed_lib.rs`, writing `.dynamic` with `DT_NEEDED` entries and a `.dynstr`.

Each builder is moved or copied into `tests/common` only as far as the new file needs it. A copied builder keeps a comment naming its source.

## R7. Mutations (decision)

- **M1**: `dynamic-linkage` decided `PackageDatabase`. Killed by the dynamic-linkage case.
- **M2**: `embedded-source-path` decided `PackageDatabase`. Killed by its case.
- **M3**: `cargo-auditable` decided `BinaryAnalysis`. Killed by its case, which must stay byte-identical to `main`.
- **M4**: the deduplicator's gate reverted to spec 1646's two kinds. Killed by the merge case (User Story 3).
- **M5**: the totality unit test's assertion inverted, or a listed kind removed from the decision. Killed by the totality test.
