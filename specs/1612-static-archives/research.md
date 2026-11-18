# Research: Open static archives on installed trees

## R1. What is inside the SDK's archives (FR-001)

**Measured.** `census-archives` run 44368338991 in `kusari-sandbox/waybill-fixture-therock-sdk`, over the pinned SDK tarball. The script is `census/archives.py`; it was checked locally first on synthetic GNU, BSD and thin archives built with `llvm-ar`. The step summary is in `measurements/census-archives.md`.

**Archives:**
- 822 regular files, all GNU format, none thin, none malformed;
- 11,703 ELF members and 929 LLVM-bitcode members;
- sizes:

| | p50 | p90 | p99 | max |
|---|---|---|---|---|
| archive | 183 KB | 4.6 MB | 55 MB | 1.76 GB (`lib/libdevice_conv_operations.a`) |
| members per archive | 2 | 29 | 179 | 1,106 |
| largest member | 107 KB | 1.3 MB | 15.7 MB | 65 MB |

Only 105 of the 822 archives have a member carrying any classifiable source path.

**What the members carry, against the document** (`measure-flags` run 44343809611, CycloneDX):

| Contents | Archives | Already identified? |
|---|---|---|
| ROCm's own code: `submodule:llvm-project` (52 archives), `submodule:…composablekernel` (the 2 large `libdevice_*` archives, 1,049 members), amdsmi | ~56 | Yes, through their own packages and binaries |
| `vendored:grpc`, the 122 archives under `lib/rdc/` | 43 with paths | **Yes, every family with a version**, from installed records: `absl` 20250512, `grpc`/`grpc++` 1.78.1, `protobuf`/`protobuf-lite`/`upb` 31.1.0, `re2` 9.0.0, `c-ares` 1.34.5, `utf8_range` 1.0. The archives are listed as evidence of those records. |
| `libssl.a` (38 members) and `libcrypto.a` (376 members) | 2 | Listed as evidence of `grpc@1.78.1`. Their member paths classify as `vendored:grpc`, so they are most likely gRPC's bundled BoringSSL. That is an inference, and no component names BoringSSL. |
| Source paths classified `other:` | 2 | `libprotoc.a` (protobuf headers; protobuf is already identified) and `libclangDriver.a` (one path) |

**No third-party tree occurs only inside archives** under the census's classification, apart from the likely BoringSSL. That one is a tree nested inside another vendored tree. #1532's path rules stop at the first vendored directory, so they report it as gRPC wherever it occurs, in archives or in binaries.

**Not measured:** curated version strings and symbol fingerprints over members. Every third-party family the paths reveal already carries a versioned record, so these would at most confirm known versions.

**Consequence for this feature:**
- **Content:** on the TheRock SDK, opening archives would add up to 822 archive components and read 4.58 GB more, and would identify no third-party package the document does not already version.
- **The only new information** is the likely BoringSSL, which needs nested vendored-tree recognition (#1532's rules), not archive opening.

Whether to proceed is a scope decision for the maintainer. The plan is not written until it is made.

## R2. Where archives are opened

**Decision:** decide per archive, with spec 1541's per-file rule for binaries. Open an archive when the scan is:
- of an image;
- of a tree where an OS package database claims files;
- or, failing both, the archive sits under an installed library's own prefix (`installed_lib::resolve::installed_prefixes`).

**Rationale:** spec 1533's `InstalledTree` is decided for the whole tree, in `scan_fs/mod.rs`. Any installed record no OS package owns makes the whole tree "recognised". Spec 1541 recorded that one vendored `lib/cmake/<pkg>/` in a source tree does this. TheRock's own source document carries `"installed-tree": "recognised"` in C226 (`measure-source` run 44355849978, `branch.sub.cdx.json`). FR-002 as first written would have opened every archive in TheRock's source tree, including LLVM's `test/**/Inputs/*.a`.

`skip_test_inputs` and `installed_prefixes` are already computed just before `binary::read`, for exactly this distinction. The archive decision reuses both.

**Alternatives:**
- spec 1533's whole-tree flag: rejected for the reason above;
- only archives already listed in a package record's evidence: rejected, because it would never open an archive no record lists, which is the case this feature is for.

The spec's FR-002 is amended to match.

## R3. The archive format

**Decision:** a new parser in the binary tier, `scan_fs/binary/ar.rs`. It reads:
- `!<arch>` with GNU names: `/` and `/SYM64/` symbol indexes, the `//` long-name table, `/N` references, and `name/` short names;
- `!<arch>` with BSD names: `#1/N` inline names, and `__.SYMDEF` / `__.SYMDEF SORTED` indexes;
- `!<thin>`: recognised and counted, never read through.

It yields `(name, byte range)` for each object member, borrowed from the archive's bytes.

The ipk reader's `parse_ar_archive` (milestone 187) is left alone. It reads ipk files, whose members are always short BSD names, and replacing it would be a refactor this feature does not need.

**Rationale:** the census parser (`census/archives.py`) reads the same structure and was checked locally on GNU, BSD and thin archives built with `llvm-ar`. Duplicate member names are common in toolchain archives, so members are addressed by position, never by name.

**Alternatives:**
- shelling out to `ar`: rejected (Constitution I, and `ar x` overwrites duplicates);
- extending ipk's parser in place: rejected (scope).

## R4. Ownership by an OS package (FR-003a)

**Decision:** use the binary tier's existing claim check, `is_path_claimed(path, claimed_paths, claimed_inodes)`.

**Rationale:** `claimed_paths` is the OS package databases' claims (dpkg, rpm, apk, alpm, opkg), plus `.ipk` archive claims. That is the "a package database owns this file" set the binary tier already honours (`package_db/mod.rs`; the OS claims are folded in at `claimed.extend(os_claimed…)`). Package-record evidence (exported targets, version files, embedded paths) never enters it. So the 758 archives listed as evidence of a package are opened, and keep that attribution (FR-009).

## R5. Per-member analysis

**Decision:** each ELF object member is parsed with `object`, and three inputs are read from it.

| Input | Read from | Why |
|---|---|---|
| String region | every section whose name is `.rodata` or starts with `.rodata.`, plus `.data.rel.ro`, capped as `collect_string_region` caps | In a relocatable object, string literals sit in `.rodata.str1.1` / `.rodata.str1.8`. `collect_string_region` looks up exactly `.rodata` and `.data.rel.ro`, so for a member it returns nothing. Measured locally on an object built with `clang --target=x86_64-linux-gnu -c`: the path literal sits in `.rodata.str1.1`. |
| Symbols | the defined global and weak symbols of `.symtab` | `symbol_names` comes from `dynamic_symbols()`, which is empty for a relocatable object. |
| Analyses | `embedded_paths::extract`, `version_strings::scan` and the symbol-fingerprint scanner, run unchanged on those inputs | |

Members that are not ELF (bitcode, data, nested archives) are counted by type, not analysed.

**Rationale:** both existing helpers silently return nothing on a relocatable object. Run unchanged, the feature would report no findings for any archive, which is a zero that looks like a result. Unit tests assert findings from a member whose strings sit only in `.rodata.str1.1`.

## R6. Representation (FR-008)

**Decision:** an opened archive becomes one binary component with a `file-sha256` identity, as a shared library's file-level component does. Its findings are nested under it with `entry::bundled_tree_to_entry(name, path, parent_purl)`, the builder #1532 uses for bundled trees under a binary. Evidence per finding is:
- the evidence kind;
- the supporting member names (sorted, deduplicated, capped);
- the version, where a version string gave one.

## R7. The coverage record (FR-011)

**Decision:** a new catalog row, **C236** `waybill:static-archive-coverage`. It is document-scope, a JSON object encoded as a string with sorted keys, as C225, C226 and C228 are. Its keys:
- `found`, `opened`, `partly-opened`;
- `skipped`, an object keyed by reason: `bound`, `thin`, `fat`, `not-an-archive`, `malformed`, `test-input`, `owned-by-os-package`, `outside-installed-prefix`;
- `members`, an object keyed by member type.

The counts satisfy `opened + partly-opened + sum(skipped) = found`. The row, its three extractors and the parity entry land in one commit, ahead of any emission (`feedback_sbom_format_mapping_extractor_gate`).

## R8. Where the work runs (FR-013)

**Decision:** an archive selected by R2 goes through the binary tier's parallel prepare stage (#1613) instead of the early build-intermediate drop. The worker:
1. reserves the archive's size against the 512 MiB read budget;
2. reads it once;
3. hashes it;
4. walks its members and analyses each;
5. returns the merged findings and drops the bytes.

The serial loop emits the archive component and its nested findings in walk order, so the output is independent of scheduling (FR-014). No path reads an archive twice.

## R9. Bounds (FR-010)

Pending the R1 decision. The census gives the distribution any bounds would be set from: archive p99 55 MB and max 1.76 GB; members per archive p99 179 and max 1,106; largest member p99 15.7 MB and max 65 MB.
