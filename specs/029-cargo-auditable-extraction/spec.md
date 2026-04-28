---
description: "Extract the cargo-auditable JSON manifest from Rust binaries' `.dep-v0` linker section to surface the full build-time crate dependency closure as `pkg:cargo/<name>@<version>` components."
status: spec
milestone: 029
---

# Spec: cargo-auditable extraction

## Background

`cargo-auditable` is a Rust-ecosystem tool (`cargo install cargo-auditable`)
that compresses the full build-time crate dependency manifest as JSON
and embeds it into a `.dep-v0` linker section of the produced binary.
Distributions including **Debian Trixie+, Fedora 40+, Alpine Edge, and
the official Rust container images** ship a pre-configured Cargo
wrapper so most Rust binaries built in those environments get the
embedded manifest automatically. The format is documented at
https://github.com/rust-secure-code/cargo-auditable.

Today mikebom's binary scanner classifies a Rust binary as a generic
`pkg:generic/<filename>?file-sha256=<hex>` component (via
`binary/entry.rs::make_file_level_component`) and runs the curated
embedded-version-string scanner against the read-only string region.
That scanner can detect ~11 well-known native libraries
(milestone 026) but cannot enumerate the **full Rust crate dependency
closure**, even when the binary carries that closure verbatim in
`.dep-v0`. Scanned Rust binaries that ship in container images,
sidecars, or `/usr/local/bin` end up with a `pkg:generic/<filename>`
entry plus the linkage-evidence `pkg:generic/<soname>` shadow
components — useful for symbol-server lookup but missing the
build-time-truth crate inventory that would let a downstream CVE
matcher (Vex / OSV / NVD) directly query each statically-linked Rust
crate.

The `.dep-v0` extraction surface is small and well-defined:

- **ELF / Mach-O / PE**: object 0.36 already exposes
  `file.section_by_name_bytes(b".dep-v0")` for ELF (already in use for
  `.note.package` / `.note.gnu.build-id` / `.gnu_debuglink` /
  `.dynamic` / `.dynstr`). Mach-O equivalents land at
  `__DATA,.dep-v0`; PE equivalents land at `.dep-v0` in the COFF
  section table. Object's API handles all three uniformly.
- **Wire format**: zlib-compressed JSON. mikebom's existing `flate2 = "1"`
  dep handles decompression.
- **JSON schema**: `{ "packages": [ { "name": <str>, "version": <semver>,
  "source": <"registry"|"git"|"local"|"path"|"unknown">, "kind":
  <"runtime"|"build"|"dev">, "dependencies": [<index>...], "root":
  <bool> } ... ] }`. mikebom's existing `serde / serde_json` deps
  parse it.

This milestone consumes the milestone-023 `extra_annotations` bag for
the **first** post-PE-trifecta usage — would be the **5th**
amortization-proof consumer (after 023 / 024 / 025 / 028) since 026
was a coverage-breadth milestone that didn't touch the bag.

## User story (US1, P1)

**As an SBOM consumer correlating a Rust binary to known CVEs in its
statically-linked crate closure**, I want mikebom's binary scanner to
extract the cargo-auditable manifest when present, so that every Rust
binary built in a `cargo auditable`-aware environment surfaces with:
(a) per-crate `pkg:cargo/<name>@<version>` components flowing into the
SBOM with `evidence-kind = "cargo-auditable"` and `confidence = "high"`,
(b) a `mikebom:detected-cargo-auditable = true` cross-link annotation
on the file-level binary component (the Rust analog of
`mikebom:detected-go = true` from milestone 005), and (c) intra-crate
dep-graph edges preserving the manifest's `dependencies` index list.

**Why P1 (not P2):** correctness-flavored, not hygiene. Without
extraction, a Rust container that ships `cargo auditable`-built
binaries presents to the SBOM consumer as opaque
`pkg:generic/<filename>?file-sha256=<hex>` blobs even though the
binary itself carries the full build-time-truth crate inventory.
That's the same data-quality argument that justified milestones 023
(ELF identity) and 025 (Go BuildInfo VCS) — the binary already carries
the answer; mikebom needs to read it.

### Independent test

After implementation:
- `cargo +stable test -p mikebom --bin mikebom scan_fs::binary::cargo_auditable`
  exercises new inline parser tests against hand-constructed
  zlib-compressed JSON blobs.
- `cargo +stable test -p mikebom --test scan_binary` gains a fixture
  test for an ELF binary with a synthetic `.dep-v0` section: asserts
  the file-level component carries `mikebom:detected-cargo-auditable
  = true` and the per-crate `pkg:cargo/<name>@<version>` components
  emit with `evidence-kind = "cargo-auditable"`.
- `cargo +stable test -p mikebom --test holistic_parity` continues
  green; a new C-section catalog row (C36 — next available after
  028's C35) covers the new annotation key in
  `docs/reference/sbom-format-mapping.md`.
- 27-golden regen produces zero diff (no fixture binary contains a
  `.dep-v0` section — same null-deltas invariant that held for
  023/024/028).

## Acceptance scenarios

**Scenario 1: Standard cargo-auditable binary**
```
Given: an ELF binary with a `.dep-v0` section carrying the
       zlib-compressed JSON `{"packages": [
         {"name": "myapp", "version": "1.2.3", "source": "local",
          "kind": "runtime", "dependencies": [1, 2], "root": true},
         {"name": "serde", "version": "1.0.193", "source": "registry",
          "kind": "runtime", "dependencies": [], "root": false},
         {"name": "tokio", "version": "1.35.1", "source": "registry",
          "kind": "runtime", "dependencies": [], "root": false}
       ]}`
When:  mikebom scans it
Then:  the file-level binary component (`pkg:generic/<filename>?file-sha256=...`)
       carries `mikebom:detected-cargo-auditable = true` (the
       cross-link annotation), AND three new components emit:
       `pkg:cargo/myapp@1.2.3`, `pkg:cargo/serde@1.0.193`,
       `pkg:cargo/tokio@1.35.1` — each with
       `mikebom:evidence-kind = "cargo-auditable"`,
       `mikebom:confidence = "high"`. The `myapp` component carries
       `parent_purl = <file-level-component's-purl>` (cross-link
       in the other direction). Each non-root component's `depends`
       list is empty (no transitives in this fixture); the root's
       `depends` list is `[pkg:cargo/serde@1.0.193, pkg:cargo/tokio@1.35.1]`.
```

**Scenario 2: Binary without `.dep-v0` section (the common case)**
```
Given: an ELF binary built without `cargo auditable` (no `.dep-v0`
       section)
When:  mikebom scans it
Then:  no `mikebom:detected-cargo-auditable` annotation emits (no
       false-true), no `pkg:cargo/*` components emit. Existing
       file-level / linkage-evidence emissions are unchanged.
```

**Scenario 3: Cross-format — Mach-O Rust binary**
```
Given: a Mach-O binary with a `__DATA,.dep-v0` section carrying the
       same JSON shape as Scenario 1
When:  mikebom scans it
Then:  same component + annotation emissions as Scenario 1.
       cargo-auditable's section is universal across formats; the
       extractor uses object's `section_by_name_bytes` which handles
       Mach-O's segment-prefixed naming convention transparently.
```

**Scenario 4: Malformed `.dep-v0` (corrupt zlib or invalid JSON)**
```
Given: an ELF binary with a `.dep-v0` section whose contents fail
       zlib decompression OR whose decompressed bytes fail JSON
       parsing
When:  mikebom scans it
Then:  no panic, no crash. The `mikebom:detected-cargo-auditable`
       annotation does NOT emit (treating the malformed section as
       absent). A `tracing::warn!` line records the failure with the
       binary path for operator visibility. Other emissions are
       unchanged.
```

**Scenario 5: Source-qualifier mapping**
```
Given: a `.dep-v0` manifest with packages carrying source = `registry`
       (default crates.io), `git`, `local`, `path`, and `unknown`
When:  mikebom emits per-crate components
Then:  the corresponding `pkg:cargo/<name>@<version>` PURLs follow
       the cargo-purl convention: registry → no qualifier (crates.io
       is the default registry); git → `?vcs_url=<url>` if the
       manifest provided one (cargo-auditable's git-source entries
       carry the URL); local / path / unknown → `?file=local`
       qualifier marking it as not-from-a-registry. Per-crate
       `mikebom:source-type` annotation (existing field) records
       the raw source value.
```

## Edge cases

- **Section name on Mach-O**: cargo-auditable specifies
  `__DATA,.dep-v0` segment+section. object's
  `section_by_name_bytes(b".dep-v0")` matches by section name
  ignoring segment prefix on Mach-O — verified to work for the
  symmetric `__DATA,__cstring` reads already in
  `scan.rs::collect_string_region`. Any segment-prefix mismatch
  surfaces as a fixture-test fail before merge.

- **Duplicate crate entries** (same `name@version` appearing twice
  with different sources, e.g. registry vs git fork): mikebom emits
  one component per `(name, version, source)` triple. PURL alone
  isn't unique in this case — qualifiers disambiguate. Same handling
  as the cargo lockfile reader's existing duplicate detection.

- **Workspace members vs crate-graph root**: cargo-auditable marks
  exactly one entry as `root: true` — the crate the binary's `main()`
  came from. Workspace members that are NOT the binary entry-point
  are NOT in the manifest at all (cargo-auditable only records what
  was actually compiled and linked into THIS binary). No
  workspace-aware handling needed — the manifest is per-binary by
  construction.

- **Build-time-only deps (`kind: "build"`)**: build-script deps
  appear in the manifest with `kind: "build"`. Spec emits them as
  `pkg:cargo` components with `mikebom:cargo-auditable-kind =
  "build"` annotation — consumers can filter if they only care about
  runtime deps. Don't drop them entirely (they ARE in the binary's
  dep closure, just at build-time rather than link-time).

- **Dev deps (`kind: "dev"`)**: same handling as build deps —
  emitted with `mikebom:cargo-auditable-kind = "dev"`. cargo-
  auditable typically excludes dev deps but documents that some
  build configurations may include them.

- **Massive manifests**: a sufficiently complex Rust binary can have
  500+ crates in its dep closure (e.g. an `axum`-based service).
  Each becomes a component. SBOM output growth is O(n) on crate
  count. Cap: none in this spec — emit them all. If a future user
  reports SBOM size pressure, we add a cap at that point.

- **Determinism**: the manifest's `packages[]` order is
  cargo-auditable's choice (typically registry-source-keyed); the
  emitted-components order needs to be deterministic regardless. We
  sort by `(name, version, source)` triple before emission, matching
  the existing cargo lockfile reader's ordering contract.

## Functional requirements

- **FR-001**: New module `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`
  provides `pub fn parse_dep_v0(bytes: &[u8]) -> Option<CargoAuditableManifest>`.
  Walks: zlib-decompress (via `flate2::read::ZlibDecoder`) → JSON
  parse (via `serde_json::from_slice`) into a typed struct.
  Returns `None` on any failure (malformed zlib, malformed JSON,
  schema mismatch). No panics.

- **FR-002**: New private types in `cargo_auditable.rs`:
  ```rust
  #[derive(serde::Deserialize)]
  pub struct CargoAuditableManifest {
      pub packages: Vec<CargoAuditablePackage>,
  }
  #[derive(serde::Deserialize)]
  pub struct CargoAuditablePackage {
      pub name: String,
      pub version: String,
      pub source: String, // "registry"|"git"|"local"|"path"|"unknown"
      pub kind: Option<String>, // "runtime"|"build"|"dev"; may be absent on older format versions
      #[serde(default)]
      pub dependencies: Vec<usize>,
      #[serde(default)]
      pub root: bool,
  }
  ```

- **FR-003**: `mikebom-cli/src/scan_fs/binary/scan.rs::scan_binary`
  calls `super::cargo_auditable::parse_dep_v0(section_bytes)` when
  `class` is one of `elf` / `macho` / `pe` AND
  `file.section_by_name_bytes(b".dep-v0")` returns Some. Result
  attaches to a new `BinaryScan::cargo_auditable: Option<CargoAuditableManifest>`
  field. Defaults to None for binaries without the section.

- **FR-004**: `mikebom-cli/src/scan_fs/binary/entry.rs::make_file_level_component`
  emits `mikebom:detected-cargo-auditable = true` (Value::Bool) into
  the file-level component's `extra_annotations` bag when
  `scan.cargo_auditable.is_some()`. **5th bag-amortization consumer**
  after 023 / 024 / 025 / 028.

- **FR-005**: New helper `mikebom-cli/src/scan_fs/binary/entry.rs::cargo_auditable_packages_to_entries`
  converts each `CargoAuditablePackage` into a `PackageDbEntry`:
  - `purl`: `pkg:cargo/<name>@<version>` per the cargo PURL convention
    (with no qualifier for `source = "registry"`; `?vcs_url=...` /
    `?file=local` for other sources).
  - `name`, `version`: from the manifest entry.
  - `source_path`: same as the binary's path (these crates were
    statically linked into THIS binary).
  - `evidence_kind`: `"cargo-auditable"`.
  - `confidence`: `"high"` (cargo-auditable is build-time truth, not
    a heuristic).
  - `parent_purl`: the file-level binary component's PURL — cross-
    links the per-crate components back to their containing binary.
  - `depends`: resolved from the manifest entry's `dependencies`
    indices (each index → resolved `pkg:cargo/<name>@<version>`).
  - `extra_annotations`: optionally `mikebom:cargo-auditable-kind =
    "<runtime|build|dev>"` if the manifest's `kind` is present and
    not `"runtime"` (omitted for runtime — that's the implied default).
    Optionally `mikebom:cargo-auditable-source = "<source>"` if the
    source is anything other than `"registry"`.

- **FR-006**: `mikebom-cli/src/scan_fs/binary/mod.rs::scan_binary_walker`
  (or whichever site dispatches the per-binary entry list) emits the
  cargo-auditable per-crate entries alongside the existing file-
  level + linkage-evidence + curated-version-string entries.

- **FR-007**: `docs/reference/sbom-format-mapping.md` gains one new
  C-section row (C36 — next available after milestone 028's C35) for
  `mikebom:detected-cargo-auditable`. Classification: `Present` × 3
  formats × `SymmetricEqual`. Justification: build-time-truth
  cross-link to the per-crate `pkg:cargo` components.

- **FR-008**: `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`
  each gain one new annotation extractor via `*_anno!` macros for
  `mikebom:detected-cargo-auditable`. `parity/extractors/mod.rs::EXTRACTORS`
  gains one new `ParityExtractor` row + 3 fn imports. Symmetric to
  how `mikebom:detected-go` (milestone 005) is wired.

- **FR-009**: Inline tests in `cargo_auditable.rs::tests` cover:
  - `parse_dep_v0_round_trips_synthetic_manifest` — hand-constructed
    JSON → zlib-compressed → `parse_dep_v0` returns the expected
    typed manifest.
  - `parse_dep_v0_returns_none_for_corrupt_zlib` — malformed gzip
    header → None, no panic.
  - `parse_dep_v0_returns_none_for_invalid_json` — valid zlib + bad
    JSON → None, no panic.
  - `parse_dep_v0_returns_none_for_missing_required_field` — JSON
    with `packages[]` containing entries that lack `name` → None.
  - `parse_dep_v0_handles_optional_kind_field` — entries with no
    `kind` field deserialize successfully (older cargo-auditable
    format).

- **FR-010**: A new fixture-driven test in `mikebom-cli/tests/scan_binary.rs`
  exercises the full pipeline against a synthetic ELF binary with a
  `.dep-v0` section. Asserts: `mikebom:detected-cargo-auditable = true`
  on the file-level component; per-crate `pkg:cargo` components emit
  with the expected PURLs, evidence-kind, and confidence; dep edges
  preserve the manifest's `dependencies` indices.

- **FR-011**: 27 byte-identity goldens regen produces zero diff (no
  existing fixture contains a `.dep-v0` section).

- **FR-012**: Per-commit `./scripts/pre-pr.sh` clean. Three atomic
  commits: parsers, wire-up, parity-row + catalog.

## Success criteria

- **SC-001**: All standard verification gates green:
  - `./scripts/pre-pr.sh` clean.
  - `cargo +stable test -p mikebom --test scan_binary` includes the
    new fixture test and it passes.
  - `cargo +stable test -p mikebom --test holistic_parity` green.
  - `cargo +stable test -p mikebom --test sbom_format_mapping_coverage`
    green (parser finds C36; every emitted field has a row).

- **SC-002**: `cargo_auditable.rs::tests` covers all 5 FR-009 cases
  + the FR-010 integration scenario.

- **SC-003**: `git diff main..HEAD -- mikebom-cli/src/scan_fs/binary/{elf,macho,pe,version_strings,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`
  is empty. The change is contained to the new `cargo_auditable.rs`
  + small wire-up in `entry.rs` / `scan.rs` / `mod.rs`.

- **SC-004**: `wc -l mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`
  ≤ 400 LOC. Smaller than 023's elf.rs / 024's macho.rs / 028's pe.rs
  because the work is "decompress + serde-deserialize" rather than
  "byte-level header parsing"; tests + fixture builders dominate the
  surface.

- **SC-005**: `git diff main..HEAD -- mikebom-common/ mikebom-cli/src/cli/
  mikebom-cli/src/resolve/ mikebom-cli/src/generate/ mikebom-cli/src/scan_fs/package_db/`
  is empty. **5th amortization-proof bag consumer.**

- **SC-006**: All 3 CI lanes (Linux default + Linux ebpf + macOS) green.

- **SC-007**: 27-golden regen zero diff (no fixture binary contains
  `.dep-v0`).

- **SC-008**: No new crate dependencies. `flate2` and `serde / serde_json`
  are already in deps; `cargo tree` after this PR shows no additions.

## Clarifications

- **`mikebom:detected-cargo-auditable` as a Bool, not a String**:
  matches `mikebom:detected-go` (milestone 005) — both are
  cross-link flags. The bag stores `serde_json::Value::Bool(true)`
  which serializes to `"true"` in CDX properties (which are stringly
  typed) and to the JSON literal `true` in SPDX 2.3/3 annotations
  (which are JSON-typed). Catalog row C36's `Present × Present ×
  Present` × `SymmetricEqual` classification holds.

- **Per-crate components flow through PackageDbEntry, NOT the bag**:
  the per-crate emissions are first-class components in the SBOM
  (with their own PURLs, depends-edges, evidence-kind), so they go
  through the established `PackageDbEntry` path. Only the
  cross-link flag (`detected-cargo-auditable`) goes through the bag.
  This mirrors how milestone 005 emits per-Go-module components for
  Go binaries plus a `detected-go = true` flag on the file-level.

- **Dep-graph edges preserve the manifest's `dependencies` array**:
  cargo-auditable's `dependencies: [<index>]` is index-based against
  the same `packages[]` list. We resolve indices to PURLs and store
  them in the per-crate component's `depends: Vec<Purl>`. Cycle
  handling: cargo's dep graph is a DAG by construction (cargo would
  refuse to compile a cycle); no cycle-detection needed.

- **PURL qualifier for `source = "git"`**: cargo-auditable's git
  entries carry a URL (the git-source URL). When present, we emit
  `pkg:cargo/<name>@<version>?vcs_url=<url>`. When absent (older
  format versions), we emit `?vcs_url=git` as a placeholder marking
  the source as non-registry. The `vcs_url` qualifier is the
  packageurl-spec idiom for git sources.

- **Confidence tier `"high"`, not `"heuristic"`**: cargo-auditable is
  build-time truth — what the cargo build process recorded into the
  binary's metadata. Distinct from the `embedded-version-string`
  scanner's heuristic-tier guesses that watch for self-identifying
  banners. Consumers can filter by `mikebom:confidence` to choose
  their tolerance for fuzzy data.

## Out of scope

- **The `kind` field's filtering** (runtime / build / dev) — emitted
  as a per-component annotation but mikebom doesn't act on it.
  Downstream consumers can filter to taste.
- **Cross-binary deduplication** of cargo-auditable entries (two
  binaries in the same scan that statically link the same `serde@1.0.193`
  produce two distinct components). The deduplication concern is
  shared with milestone 023 / 024 / 028's per-binary identity work
  and would be a separate cross-binary milestone.
- **Cargo workspace awareness**. cargo-auditable already records the
  per-binary closure; mikebom doesn't need to re-derive workspace
  membership.
- **Source-tree validation**. We don't try to validate that the
  recorded crate versions are reachable from `Cargo.lock` or a
  registry — the cargo-auditable manifest is the source of truth
  for what was linked.
- **Container-layer cross-link** (which layer this binary lives in).
  That's milestone 027's concern.
- **`mikebom:cargo-auditable-format-version`** — cargo-auditable
  has a format version field; we ignore it as long as the schema
  we deserialize against works. If/when the format changes
  incompatibly, upgrade the deserializer at that point.
