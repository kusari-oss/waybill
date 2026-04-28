---
description: "Task list — milestone 029 cargo-auditable extraction"
---

# Tasks: cargo-auditable extraction — Tighter Spec

**Input**: Design documents from `/specs/029-cargo-auditable-extraction/`
**Prerequisites**: spec.md (✅), plan.md (✅), checklists/requirements.md (✅)

**Tests**: 5+ new inline parser tests in `cargo_auditable.rs::tests` +
2 new bag/per-crate-emission tests in `entry.rs::tests` + 1 new
fixture-driven integration test in `tests/scan_binary.rs` +
holistic_parity continuing to pass + sbom_format_mapping_coverage
continuing to pass.

**Organization**: Single user story (US1, P1). Three atomic commits.

## Path Conventions

- Touches `mikebom-cli/src/scan_fs/binary/{cargo_auditable,entry,scan,mod}.rs`,
  `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3,mod}.rs`
  (additive), `docs/reference/sbom-format-mapping.md` (additive),
  `mikebom-cli/tests/scan_binary.rs` (additive).
- Does NOT touch `mikebom-common/`, `mikebom-cli/src/cli/`,
  `mikebom-cli/src/resolve/`, `mikebom-cli/src/generate/`,
  `mikebom-cli/src/scan_fs/binary/{elf,macho,pe,version_strings,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`,
  or any `mikebom-cli/src/scan_fs/package_db/` file.

---

## Phase 1: Setup + baseline

- [X] T001 Recon done. Confirmed:
      - `flate2 = "1"` is in `mikebom-cli/Cargo.toml:48` (already in deps).
      - `serde / serde_json` are in workspace deps (no addition needed).
      - `object`'s `section_by_name_bytes` is proven for ELF /
        Mach-O / PE section reads via `scan.rs:122-135`
        (`.note.package` / `.note.gnu.build-id` etc.).
      - cargo-auditable's manifest schema is documented + stable;
        zlib-compressed JSON is the wire format.
      - Bag-amortization pattern from 023/024/025/028 supports the
        new `mikebom:detected-cargo-auditable` annotation.
- [ ] T002 Snapshot baseline: `./scripts/pre-pr.sh 2>&1 | tee /tmp/baseline-029.txt | grep -cE '^test [a-z_:]+ \.\.\. ok' > /tmp/baseline-029-count.txt`.

---

## Phase 2: Commit 1 — `029/parsers`

**Goal**: `cargo_auditable.rs` becomes a real module with
`parse_dep_v0` + types + 5 inline tests; dead-code allowed for this
commit only.

- [ ] T003 [US1] Create `mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`
      with module header documenting the cargo-auditable format,
      the `.dep-v0` section name across ELF/Mach-O/PE, the
      zlib+JSON wire format, and the milestone reference.
- [ ] T004 [US1] Add public types:
      ```rust
      #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
      pub struct CargoAuditableManifest {
          pub packages: Vec<CargoAuditablePackage>,
      }

      #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
      pub struct CargoAuditablePackage {
          pub name: String,
          pub version: String,
          pub source: String,
          #[serde(default)]
          pub kind: Option<String>,
          #[serde(default)]
          pub dependencies: Vec<usize>,
          #[serde(default)]
          pub root: bool,
      }
      ```
- [ ] T005 [US1] Add `pub fn parse_dep_v0(bytes: &[u8]) ->
      Option<CargoAuditableManifest>`. Walks: zlib-decompress via
      `flate2::read::ZlibDecoder` → `serde_json::from_slice` → return
      `Option<CargoAuditableManifest>`. Returns `None` on any
      failure. No panics.
- [ ] T006 [US1] Declare the new module in
      `mikebom-cli/src/scan_fs/binary/mod.rs` (`pub(super) mod
      cargo_auditable;`).
- [ ] T007 [US1] Add inline tests in `#[cfg(test)] mod tests`:
      - `parse_dep_v0_round_trips_synthetic_manifest` — hand-build
        JSON, zlib-compress, parse, assert deserialized struct
        matches.
      - `parse_dep_v0_returns_none_for_corrupt_zlib` — bytes that
        fail zlib decompression → None.
      - `parse_dep_v0_returns_none_for_invalid_json` — valid zlib
        of non-JSON bytes → None.
      - `parse_dep_v0_returns_none_for_missing_required_field` —
        JSON with packages missing `name` or `version` → None.
      - `parse_dep_v0_handles_optional_kind_field` — entries
        without `kind` deserialize successfully (older format).
- [ ] T008 [US1] Add `#[allow(dead_code)]` on the public types +
      `parse_dep_v0` since they're not yet wired up. Removed in
      commit 2.
- [ ] T009 [US1] Verify: `cargo +stable test -p mikebom --bin
      mikebom scan_fs::binary::cargo_auditable` includes the new
      tests + they pass. `./scripts/pre-pr.sh` clean.
- [ ] T010 [US1] Commit: `feat(029/parsers): add cargo-auditable .dep-v0 manifest extractor`.

---

## Phase 3: Commit 2 — `029/wire-up-bag-and-entries`

**Goal**: BinaryScan gains `cargo_auditable` field; scan.rs reads
.dep-v0 section; entry.rs translates manifest into bag annotation +
per-crate components.

- [ ] T011 [US1] Edit `binary/entry.rs::BinaryScan`: add
      `pub cargo_auditable: Option<cargo_auditable::CargoAuditableManifest>`
      field after the macho_/pe_ identity fields. Doc comment naming
      the source (`.dep-v0` ELF section / `__DATA,.dep-v0` Mach-O).
- [ ] T012 [US1] Update the 4 BinaryScan struct-literal sites:
      - `scan.rs` non-fat ELF/PE arm — populate
        `cargo_auditable` by reading `.dep-v0` section bytes and
        calling `super::cargo_auditable::parse_dep_v0(bytes)`.
      - `scan.rs` non-fat Mach-O arm — same as ELF/PE.
      - `scan.rs` fat-Mach-O arm — read first slice's `.dep-v0`
        section (per the FR-004 first-slice convention from 024).
      - `entry.rs::tests::fake_binary_scan` test helper — `None`.
- [ ] T013 [US1] Edit `entry.rs::make_file_level_component`: when
      `scan.cargo_auditable.is_some()`, insert
      `mikebom:detected-cargo-auditable = Value::Bool(true)` into
      the file-level component's `extra_annotations` bag. Symmetric
      to `mikebom:detected-go` from milestone 005.
- [ ] T014 [US1] Add new helper
      `entry.rs::cargo_auditable_packages_to_entries(scan: &BinaryScan,
      file_level_purl: &Purl, path: &Path) -> Vec<PackageDbEntry>`:
      - Iterate `scan.cargo_auditable.as_ref()?.packages.iter().enumerate()`.
      - For each `(idx, pkg)`:
        - Build PURL: `pkg:cargo/<name>@<version>` with optional
          `?vcs_url=<url>` for git sources, `?file=local` for
          local/path/unknown.
        - Build `PackageDbEntry`:
          - `purl`: above
          - `name`, `version`: from manifest
          - `source_path`: binary path (these crates are linked in)
          - `evidence_kind`: `Some("cargo-auditable".into())`
          - `confidence`: `Some("high".into())`
          - `parent_purl`: `Some(file_level_purl.clone())`
          - `depends`: resolved from `pkg.dependencies` (each idx
            → resolved `pkg:cargo/<name>@<version>` via the manifest
            packages array). Sort the resulting `depends` Vec by
            PURL string for determinism.
          - `extra_annotations`: optionally
            `mikebom:cargo-auditable-kind = "<kind>"` if `pkg.kind`
            is `Some(_)` and not `"runtime"`; optionally
            `mikebom:cargo-auditable-source = "<source>"` if
            `pkg.source != "registry"`.
          - All other fields: defaults.
      - Sort the returned Vec by `(name, version, source)` triple
        for deterministic emission order.
- [ ] T015 [US1] Edit `binary/mod.rs::scan_binary_walker` (or the
      per-binary entry-list dispatch site): emit the per-crate
      entries from `cargo_auditable_packages_to_entries` alongside
      the existing file-level + linkage-evidence + version-string
      entries. Find the existing version-string emission call site
      and add a parallel call to the new helper.
- [ ] T016 [US1] Remove `#[allow(dead_code)]` from
      `cargo_auditable.rs` parsers + types.
- [ ] T017 [US1] Add 2 new inline tests in `entry.rs::tests`:
      - `make_file_level_component_emits_detected_cargo_auditable_when_manifest_present`
        — populated `BinaryScan.cargo_auditable` → bag has
        `mikebom:detected-cargo-auditable = true`.
      - `cargo_auditable_packages_to_entries_emits_per_crate_components`
        — synthetic 3-crate manifest → 3 entries with correct
        PURLs, evidence-kind, confidence, parent_purl, depends.
- [ ] T018 [US1] Add a fixture-driven integration test in
      `mikebom-cli/tests/scan_binary.rs::scan_emits_cargo_auditable_components_for_binary_with_dep_v0`:
      - Build a synthetic ELF binary in tempdir with a `.dep-v0`
        section carrying a hand-constructed zlib-compressed JSON
        manifest with 3 crates.
      - Run `mikebom sbom scan` against it.
      - Assert: file-level component carries
        `mikebom:detected-cargo-auditable = true`; 3
        `pkg:cargo/<name>@<version>` components emit with the
        expected fields.
      - Negative test variant: same binary without `.dep-v0` section
        — assert no `pkg:cargo` components and no
        `mikebom:detected-cargo-auditable` annotation.
- [ ] T019 [US1] Verify: `cargo +stable test -p mikebom --bin
      mikebom scan_fs::binary::entry::tests` green.
      `cargo +stable test -p mikebom --test scan_binary` includes
      the new test and it passes. `./scripts/pre-pr.sh` clean.
- [ ] T020 [US1] Commit: `feat(029/wire-up-bag-and-entries): populate cargo-auditable manifest into the bag and emit per-crate components`.

---

## Phase 4: Commit 3 — `029/parity-row`

**Goal**: 1 new C-section catalog row + per-format extractor +
EXTRACTORS row.

- [ ] T021 [US1] Edit `docs/reference/sbom-format-mapping.md`: add
      1 new C-section row (C36 — next available after milestone
      028's C35) for `mikebom:detected-cargo-auditable`.
      Classification: `Present` × 3 formats × `SymmetricEqual`.
      Justification: build-time-truth cross-link to per-crate
      `pkg:cargo` components, the Rust analog of
      `mikebom:detected-go` (milestone 005).
- [ ] T022 [US1] Edit `mikebom-cli/src/parity/extractors/cdx.rs`:
      add `cdx_anno!(c36_cdx, "mikebom:detected-cargo-auditable", component);`
      after the existing C30-C35 block.
- [ ] T023 [US1] Edit `mikebom-cli/src/parity/extractors/spdx2.rs`:
      add mirror `spdx23_anno!(c36_spdx23, ...)`.
- [ ] T024 [US1] Edit `mikebom-cli/src/parity/extractors/spdx3.rs`:
      add mirror `spdx3_anno!(c36_spdx3, ...)`.
- [ ] T025 [US1] Edit
      `mikebom-cli/src/parity/extractors/mod.rs::EXTRACTORS`: add
      1 new `ParityExtractor` row + 3 fn imports across the
      `use cdx::{...}`, `use spdx2::{...}`, `use spdx3::{...}`
      blocks.
- [ ] T026 [US1] Verify: `cargo +stable test -p mikebom --test
      holistic_parity` green. `cargo +stable test -p mikebom --test
      sbom_format_mapping_coverage` green.
- [ ] T027 [US1] `./scripts/pre-pr.sh` clean.
- [ ] T028 [US1] Commit: `feat(029/parity-row): wire detected-cargo-auditable into the holistic-parity matrix`.

---

## Phase 5: Verification

- [ ] T029 SC-001 verification: 4 standard gates green.
- [ ] T030 SC-002 verification: `cargo_auditable.rs::tests` covers
      all 5 parser scenarios + the FR-010 integration test.
- [ ] T031 SC-003 verification: `git diff main..HEAD --
      mikebom-cli/src/scan_fs/binary/{elf,macho,pe,version_strings,linkage,packer,predicates,jdk_collapse,python_collapse}.rs`
      empty.
- [ ] T032 SC-004 verification: `wc -l mikebom-cli/src/scan_fs/binary/cargo_auditable.rs`
      ≤ 400.
- [ ] T033 SC-005 verification: `git diff main..HEAD --
      mikebom-common/ mikebom-cli/src/cli/ mikebom-cli/src/resolve/
      mikebom-cli/src/generate/ mikebom-cli/src/scan_fs/package_db/`
      empty. **5th amortization-proof consumer.**
- [ ] T034 SC-007 verification: 27-golden regen
      (`MIKEBOM_UPDATE_*_GOLDENS=1`) produces zero diff. No
      existing fixture binary contains `.dep-v0`.
- [ ] T035 SC-008 verification: `cargo tree -p mikebom` shows no
      new crate additions vs main.
- [ ] T036 Push branch; observe all 3 CI lanes green (SC-006).
- [ ] T037 Author the PR description: 3-commit summary, 5th-consumer
      bag-amortization attestation, byte-identity attestation.

---

## Dependency graph

```text
T001-T002 (recon + baseline, recon done)
   │
   ↓
T003-T010 [Commit 1: parsers + dead_code]
   │
   ↓
T011-T020 [Commit 2: wire-up-bag + per-crate-entries + integration test]
   │
   ↓
T021-T028 [Commit 3: parity-row]
   │
   ↓
T029-T037 (verification + PR)
```

## Estimated effort

| Phase | Effort | Notes |
|---|---|---|
| Phase 1 (baseline) | 5 min | T001 done; just snapshot |
| Phase 2 (parsers) | 2 hr | small surface — zlib + serde_json deserialization |
| Phase 3 (wire-up + integration test) | 3 hr | 4 BinaryScan literal sites + per-crate-emission helper + fixture-driven test |
| Phase 4 (parity row) | 30 min | mechanical |
| Phase 5 (verify + PR) | 1 hr | golden regen + CI watch |
| **Total** | **~6-7 hr** | sits between 026 (3 hr) and 023 (9 hr). |
