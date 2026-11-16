# Research: native binaries inside a Python package

All file references are at `main` `1ae157d5` plus the spec commits on this branch.

## R1. How a `RECORD` claim reaches the binary scan today

**Observed:**
- `pip::collect_claimed_paths` (`package_db/pip/dist_info.rs:39`) inserts every `RECORD` path into the same `claimed` set that dpkg, apk, alpm, rpm and `.ipk` use (`package_db/mod.rs:1955`).
- `scan_fs/mod.rs:598` passes that set to `binary::read`. `is_path_claimed` (`binary/mod.rs:199`) sets `path_claimed`, which sets `skip_file_level` and `skip_secondary_evidence` (`binary/mod.rs:617–621`). That suppresses the file-level component, linkage, the ELF note, version strings, symbol fingerprints and embedded paths.
- cargo-auditable crates are emitted regardless (milestone 130).
- `go_binary` emits parsed Go modules regardless of claims (`go_binary.rs:532`); only its diagnostics are claim-gated.
- The OS-only set from spec 1531 (`os_claimed_paths`) already exists. A unit test (`package_db/mod.rs:3398`) asserts that a pip claim is in the shared set and not in the OS set.

**Decision:** split the pip claims out of the shared set.
- `pip::collect_claimed_paths` instead fills a new `RecordOwners`: path, and on Unix `(dev, ino)`, mapped to the owning `.dist-info` directory.
- The shared `claimed` set keeps everything else: OS databases and `.ipk`.
- `binary::read` takes `&RecordOwners`. A binary is **owned** when `RecordOwners` resolves it and the shared set does not claim it.
- An owned binary goes through the unclaimed path (FR-001).
- A binary in both sets keeps today's behaviour (FR-005, FR-007).

**Rationale:** FR-005 and FR-007 need "RECORD and nothing else". With the pip claims out of the shared set, that becomes a set-membership test that cannot drift. The other choice, testing `claimed ∖ os_claimed`, would also free `.ipk`-claimed binaries, which FR-007 forbids.

**Alternatives considered:**
- Keep pip in the shared set and test `os_claimed`. Rejected, because it frees `.ipk` claims.
- Drop the pip claim entirely. Rejected: it gives FR-006's identification, but no owner for FR-002.

**Consequence:** the test at `package_db/mod.rs:3445` changes. It now asserts that the pip claim is in `RecordOwners` and is in neither claim set. Its subset assertion is unchanged.

**Other consumers of the shared set.** I enumerated them all with a full, untruncated grep. Two besides `binary::read` lose the pip claims:
- **`go_binary`** (`go_binary.rs:662`, `:739`) uses the set only to suppress the diagnostic `pkg:generic/<name>` it emits for a Go binary whose build info doesn't parse. The binary scan already emits a file-level component for that binary, so for an owned binary a diagnostic would be a second component for the same file. **Decision:** `go_binary` takes `&RecordOwners` and treats an owned path as claimed for diagnostics only. Parsed modules are emitted regardless, as today, and R4 nests them.
- **`LinkageAggregator::add_with_claim_check`** probes standard library search paths for a soname. A pip-owned file is on such a path only when a package installs outside `site-packages`. Then a binary the package doesn't own, linking it, emits a linkage component where it was suppressed before. That is the accurate report: the library is not part of the linking binary's package. **Decision:** accept this. A test in T012 pins the behaviour.

## R2. The owner's identity and determinism

**Observed:**
- `read_venv_dist_info` builds the `pkg:pypi` PURL from `METADATA`. Its entries carry `source_path` = `<dist-info>/METADATA`.
- `candidate_site_packages_roots` already includes the scan root when it holds a `.dist-info` (#1529). So both layouts already go through one code path (FR-006 holds structurally).

**Decision:**
- `RecordOwners` maps a path to a `.dist-info` directory. After the readers run, the owner's PURL is resolved from the pip entry whose `source_path` is that directory's `METADATA`.
- **Two `RECORD`s claiming one file:** the `.dist-info` path that sorts first wins (`BTreeMap` insert-if-absent over sorted directories). This satisfies FR-010.
- **A `RECORD` whose `.dist-info` yields no pip entry** (unreadable `METADATA`): the binary is analysed as an unclaimed binary, top-level, and is not counted in C230.

**Rationale:** the PURL is taken from the component waybill actually emits, never rebuilt from the directory name; per the memory note "never model the reader". When there is no owner, ownership can't be stated. Analysing without nesting reports what is there and asserts nothing false (Principle IX).

## R3. Which analysis an owned binary gets

**Decision:** for an owned binary:
- `path_claimed` is false;
- the Python and JDK collapsers are not consulted, because the binary already has an owner and must not become a `cpython` umbrella;
- `rpm_dir_heuristic` and `go_in_linux` apply unchanged.

So an owned Go binary still gets no linkage noise, and its `go_binary` modules nest under it (R4).

**Rationale:** FR-001 says "as an unclaimed file does". The collapsers are what makes an unclaimed CPython tree into one component, and their layout match is for CPython's own `lib-dynload`. That directory is never `RECORD`-claimed, so the edge case "CPython is unchanged" still holds.

## R4. Nesting and edges: one pass after the binary scan

**Observed:**
- Version-string, symbol-fingerprint, v2-matcher and ELF-note entries are emitted flat today. Only bundled trees (spec 1534) and cargo-auditable crates carry `parent_purl`.
- `LinkageAggregator` dedups globally by soname. Its `_parent_bom_ref` is "reserved", so no edge starts at a binary.
- The no-`.dist-info` repro shows every edge starting at the document root, through the CycloneDX primary-dependency fallback (`cyclonedx/dependencies.rs:44`).
- `PackageDbEntry.depends` accepts an exact emitted PURL (`resolve_exact_purl_dep`, `scan_fs/mod.rs:245`).

**Decision:** `binary::read` additionally returns one `OwnedBinary` record per owned binary it analysed:
- its path and owner;
- its file-level PURL;
- the linkage PURLs its imports resolved to;
- its internal links;
- whether a content-derived identification fired.

A new pass, `attach_owned` in a new module `scan_fs/binary/owned.rs`, runs in `scan_fs/mod.rs` after `binary::read` and before the name index is built. It does five things:
1. Sets the binary's file-level entry `parent_purl` to the owner's PURL, and appends the binary's PURL to the owner entry's `depends`.
2. For every entry derived from that path, sets `parent_purl` to the binary's PURL (unless it is already set, as cargo-auditable's is) and appends it to the binary's `depends`. "Derived from that path" means a version string, symbol fingerprint, v2 match, ELF note, bundled tree, cargo-auditable crate, or a `go_binary` module whose `source_path` is the path.
3. Appends each external linkage PURL to the binary's `depends`. The linkage components themselves stay top-level and globally deduplicated: they are libraries outside the package.
4. **Internal link (FR-004):** if a soname's base name equals the file name of another binary the same owner owns, the soname is not added to the linkage aggregator. The binary's `depends` gets that binary's PURL instead.
5. Counts `analysed` and `identified` for C230.

**Rationale:**
- Edges go through the existing exact-PURL `depends` path, so all three formats get them from one source.
- The pass keys on paths, so it covers `go_binary` without changing that reader.
- Internal links are matched by name against the owner's own file list. auditwheel (`$ORIGIN/../<pkg>.libs`) and delocate (`@loader_path/.dylibs`) resolve them through rpaths that the linkage probe does not follow. The owner's file list is the exact set the link can resolve into.

**Alternatives considered:**
- Thread a parent into every emitter inside `binary::read`. Rejected: Go modules come from another reader.
- Resolve rpaths. Rejected: it is new machinery for the same answer.

## R5. Three-level nesting in the formats

**Observed:**
- **CycloneDX:** `build_components` (`cyclonedx/builder.rs:2190`) folds children only into parents whose `bom-ref` equals their PURL, which means top-level components. A grandchild's parent is nested, so the grandchild falls back to top-level.
- **SPDX 2.3:** `CONTAINS` edges are emitted only from top-level parents (`spdx/relationships.rs:445`).
- **SPDX 3:** `build_containment_relationships` (`v3_relationships.rs:371`) accepts any parent.
- **Dependencies:** CycloneDX `dependencies[].ref` is keyed by PURL (`dependencies.rs:57`), so a nested component's dependency entry names a ref that isn't its composite `bom-ref`.
- **Measured, 2026-11-16:** the 21 committed corpus CycloneDX goldens contain **0** nested components and **0** composite refs. No CycloneDX fixture under `waybill-cli/tests/` has a composite `bom-ref`.

**Decision:**
- **CycloneDX:** fold recursively, deepest first. A nested `bom-ref` is `<purl>#<parent bom-ref>`, so the whole ancestor chain is in the ref and stays unique even when one library sits under two packages. Every `dependencies[].ref` and `dependsOn` target is rewritten through a PURL → `bom-ref` map. A target PURL with several copies resolves to the copy whose parent is the edge's source, else to the top-level copy. A source PURL with several copies emits its edges once per copy.
- **SPDX 2.3:** emit `CONTAINS` from any emitted parent, not only a top-level one.
- **SPDX 3:** unchanged.
- **The same PURL nested under two parents** (one library vendored by two packages): the SPDX identifier of a nested component derives from its PURL plus its parent chain, so the two copies get distinct identifiers. A test with two packages vendoring a byte-identical library pins this (data-model V4), validated by `spdx3-validate` and the SPDX 2.3 schema.

**Rationale:** with no nested components in any golden, changing how they are emitted is invisible outside this feature. That is measured, not assumed. The other order (key edges by PURL and accept dangling refs) is the existing tolerance for declared-not-shipped dependencies, and it would leave the package → binary → library chain unresolvable in CycloneDX.

## R6. The auditwheel hash suffix (FR-008)

**Decision:**
- The suffix is stripped only for an owned binary that sits directly in a directory whose name ends in `.libs`, which is auditwheel's layout. The pattern is `^(?P<stem>.+)-[0-9a-f]{8}(?P<ext>\.so(\..+)?)$`. The stripped name is the component's `name` and its PURL name segment.
- The `file-sha256` qualifier still carries the identity.
- delocate does not rename, so `.dylibs/` needs no strip.

**Rationale:** confining the strip to auditwheel's directory avoids stripping a legitimate `-deadbeef` from an unrelated library. **Provisional:** T001 confirms the pattern on the real `manylinux` wheel in CI before T0xx implements it (memory note: re-verify research claims at implement time).

## R7. FR-009's carrier: C230

**Decision:** a new document-scope row, **C230 `waybill:python-owned-binaries`**. Its value is a JSON object as a string, with sorted keys: `{"analysed":N,"identified":M}`.
- It is emitted only when `analysed > 0`.
- `identified` counts owned binaries with at least one content-derived identification (clarification 4). Linkage does not count.
- It follows the C228 and C229 pattern: CycloneDX `metadata.properties`, and SPDX 2.3 and SPDX 3 document annotations, with parity extractors in the same change (memory note: adding rows requires extractors).

**Rationale:** no existing row counts this. C228 counts bundled trees, which is a different population (Principle V allows a new row only when no native or existing carrier fits).

## R8. The real wheels (SC-004)

**Observed:**
- TheRock publishes the ROCm SDK as wheels: `rocm-sdk-core`, `rocm-sdk-libraries` and `rocm-sdk-device-<gfx>`. The indexes are `https://<channel>.repo.amd.com/rocm/whl-next/` (`RELEASES.md` in ROCm/TheRock, read through the GitHub API on 2026-11-16).
- The PyPI numerical package with both repaired layouts is NumPy. Its `manylinux` wheels carry `numpy.libs/`, and its macOS wheels carry `numpy/.dylibs/`. This is to be confirmed by T001: nothing has been downloaded yet.

**Decision:** a new workflow in `kusari-sandbox/waybill-fixture-therock-sdk`, `measure-wheels.yml`, inputs `waybill_ref` and `base_ref`. It:
- downloads `rocm-sdk-libraries` from the stable index, plus one NumPy `manylinux_x86_64` wheel and one NumPy `macosx_arm64` wheel, each pinned by version and SHA-256;
- unpacks each and records its layout census: libraries, hash-suffixed names, `RECORD` entries outside the package, and sizes against `--max-file-size`;
- scans each at `base_ref` and at `waybill_ref` with its `.dist-info`, and at `waybill_ref` without it;
- reports, per wheel, owned binaries, identifications, edges and C230. It diffs the identifications and linkage against the no-`.dist-info` scan.

Nothing from TheRock, or any of these wheels, is downloaded locally.

## R9. Predicted corpus effect (SC-002)

**Prediction, not measurement:**
- The corpus targets are source trees, Nix closures, one Debian image and Pants examples. The Debian image's Python packages are dpkg-owned, so they are unchanged under FR-005.
- A change can appear only where a scanned tree holds a `.dist-info` with a `RECORD` listing a native file that no OS database claims.
- The expected result is byte-identity, or changes confined to such files.

T0xx regenerates the goldens in CI and checks the prediction.

## R10. Cost

Owned binaries now get the same analysis that the same files get today without a `.dist-info`. The cost is that of the no-`.dist-info` scan, which already exists. The SC-004 workflow records wall time for both scans of each wheel. No new budget is set.
