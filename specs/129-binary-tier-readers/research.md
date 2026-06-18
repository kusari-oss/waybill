# Research: Image-tier binary-extracted package readers (milestone 129)

## R1. `.deps.json` wire format

**Decision**: Parse the top-level `libraries` map as the authoritative dependency list. Read `runtimeTarget.name`
as a single per-document annotation (e.g. `.NETCoreApp,Version=v8.0`) for transparency. Skip the `targets` map
entirely on first pass; revisit if SC-001's 5% accuracy target proves un-reachable from `libraries` alone.

**Rationale**: A `.deps.json` document has four top-level keys:

```json
{
  "runtimeTarget": { "name": ".NETCoreApp,Version=v8.0", "signature": "" },
  "compilationOptions": { ... },
  "targets": {
    ".NETCoreApp,Version=v8.0": { "MyApp/1.0.0": { ... }, "Microsoft.AspNetCore.App.Ref/8.0.0": { ... } }
  },
  "libraries": {
    "MyApp/1.0.0": { "type": "project", "serviceable": false, "sha512": "" },
    "Microsoft.AspNetCore.App.Ref/8.0.0": { "type": "package", "serviceable": true, "sha512": "sha512-..." }
  }
}
```

The `libraries` map has one key per `(name, version)` tuple — exactly what we need to enumerate as
`pkg:nuget` components. The `targets` map duplicates this information but adds per-runtime-target context
(which assembly file paths are loaded, which `dependencies` the package brings in transitively). For the
P1 user story (enumerate every NuGet package), `libraries` alone is sufficient. The `targets` map's
relationship graph is a Phase 11 (transitive correctness) target — out of scope for milestone 129.

**Alternatives considered**:

- Parse `targets` as the primary signal: more complete (carries assembly paths) but ~4× the JSON depth
  and complexity, and the audit's syft comparison shows syft also uses `libraries` only.
- Parse the `<app>.runtimeconfig.json` file too: it carries the .NET runtime version but no per-package
  data. Already covered by the `runtimeTarget.name` field. Defer to Phase 11 if needed.

## R2. CLR managed-assembly metadata table layout (ECMA-335 §II.22)

**Decision**: Hand-roll a minimal metadata-table reader on top of `object::read::pe::PeFile` using its
existing `optional_header().data_directories()[14]` accessor to locate the `COR20_HEADER` (the
`IMAGE_COR20_HEADER` struct at the start of the CLR metadata blob). From the `MetaData` directory inside
that header, locate the `#~` (metadata tables) stream and the `#Strings` (UTF-8 string heap) stream.
Read the `Assembly` table (token `0x20`) for `AssemblyName`, `AssemblyVersion`, and `PublicKeyToken`.
Read the `CustomAttribute` table for `AssemblyFileVersionAttribute` and
`AssemblyInformationalVersionAttribute` rows; each carries a `Blob` heap reference whose first byte sequence
is the prolog `01 00` followed by a UTF-8 length-prefixed string.

**Rationale**: The `object` crate (0.36, workspace dep) gives us PE COFF section + data-directory parsing
for free, but does NOT understand CLR metadata. The `pelite` crate adds CLR metadata parsing but pulls
~5 KLOC of code we don't need (its scope covers full PE inspection including unwind info, debug data,
resources, etc.). For our scope — three string fields per managed DLL — hand-rolling is ~150 LOC and
zero new deps. The format is stable since .NET 2.0 (~2005) and is documented in ECMA-335 §II.22.30
(`Assembly` table) and §II.23.3 (custom-attribute blob serialization).

**Alternatives considered**:

- Pull in `pelite = "0.10"`: adds ~5 KLOC, ~30 transitive deps. Rejected per Constitution I (every new
  dep is a supply-chain surface) and the "zero new Cargo deps" plan goal.
- Use a fork of `object` with CLR support: doesn't exist upstream; would require maintaining a fork.
- Shell out to `monodis` or `ikdasm`: requires Mono installation; violates `--offline` mode and adds
  external runtime requirement.

## R3. PE COFF + CLR header — detection

**Decision**: A PE file is a managed assembly iff its `IMAGE_OPTIONAL_HEADER.DataDirectory[14]`
(`IMAGE_DIRECTORY_ENTRY_COM_DESCRIPTOR`) has a non-zero `VirtualAddress` AND non-zero `Size`. Native
DLLs (Win32, Win64, MSVC runtime, etc.) have this entry zeroed.

**Rationale**: Single-condition check; available directly via `object::read::pe::PeFile64::nt_headers().optional_header.data_directories[14]`. False positives are vanishingly rare (the CLR directory is reserved
for managed code by spec). The reader's `is_managed_assembly()` helper returns `bool` from this check
and is called BEFORE attempting any metadata-table parse — guarantees the reader never tries to interpret
a native DLL's `.text` section as a `#Strings` heap.

## R4. cargo-auditable v0 wire format

**Decision**: ELF section named `.dep-v0` carries a raw deflate-compressed (NOT gzip-framed) JSON payload
with schema `{packages: [{name: String, version: String, source: String, kind: Option<String>,
dependencies: Vec<usize>, root: Option<bool>}]}`. The `source` field's values include `"local"` (path
dep), `"crates-io"` (registry default), `"git+https://...#<sha>"` (git dep), and `"unknown"` (fallback).
The `kind` field defaults to `"runtime"` if absent; permitted values per the upstream spec are
`"runtime"`, `"build"`, `"dev"`.

**Rationale**: Documented at `https://github.com/rust-secure-code/cargo-auditable` and consumed by the
`rust-audit-info` reference tool. The format is stable; mikebom's reader handles v0 only (FR-014). Future
versions (v1, etc.) will use a different section name; we'll add support in a follow-up milestone after
the format ratifies.

**Decompression path**: `flate2::read::DeflateDecoder::new(&section_bytes[..])` → `read_to_string()` →
`serde_json::from_str::<CargoAuditableManifest>()`. The deflate stream is **raw deflate** (no gzip
header, no zlib header) per the cargo-auditable v0 spec.

**Section discovery**: `object::ObjectSection::name()` returns `Option<&str>` for each section.
Iterate via `file.sections()` and `.find(|s| s.name() == Ok(".dep-v0"))`.

**Alternatives considered**:

- Probe by ELF note section instead of section name: cargo-auditable v0 uses `SHT_PROGBITS`, not
  `SHT_NOTE`, so the note-iteration API doesn't apply.
- Use `auditable-extract` crate (the upstream reference reader): adds a new dep; the reader is ~50 LOC
  and hand-rolling matches Constitution I posture.

## R5. cargo-auditable `kind` field → mikebom `lifecycle_scope`

**Decision**: Map per resolved clarification Q1:

| cargo-auditable `kind` | mikebom `ResolvedComponent.lifecycle_scope` | Native CDX `scope` | Native SPDX 2.3 relationship type |
|---|---|---|---|
| `"runtime"` (default) | `LifecycleScope::Runtime` | `required` | `DEPENDENCY_OF` |
| `"build"` | `LifecycleScope::Build` | `optional` | `BUILD_DEPENDENCY_OF` |
| `"dev"` | `LifecycleScope::Test` | `optional` | `TEST_DEPENDENCY_OF` (matches `[dev-dependencies]` per milestone 052) |

**Rationale**: Matches the existing source-tier Cargo reader (which translates `[dev-dependencies]` to
`LifecycleScope::Test`). Avoids inventing a `Dev` variant absent from the rest of the binary-tier
vocabulary. Routes through the existing `ResolvedComponent.lifecycle_scope` field, which the milestone-052
emitters consume natively — no new `mikebom:*` annotation. Principle V compliant.

**Validation**: The cargo-auditable v0 spec at upstream commit `c8d8f3c` lists `runtime / build / dev` as
the three kinds. mikebom's source-tier reader at `mikebom-cli/src/scan_fs/package_db/cargo.rs` maps these
identically (verified via `grep -n LifecycleScope::Test mikebom-cli/src/scan_fs/package_db/cargo.rs`).

## R6. Nested-JAR walker — recursion shape

**Decision**: Add a `walk_nested_archives(archive_bytes: &[u8], depth: u8, visited: &mut HashSet<[u8; 32]>,
emitter: &mut Vec<PackageDbEntry>)` recursive function inside the existing
`mikebom-cli/src/scan_fs/package_db/maven/jar.rs`. Each call: SHA-256 the input bytes, dedup-skip if
already-visited, then `zip::ZipArchive::new(Cursor::new(archive_bytes))`, iterate entries, for each
entry whose name ends in `.jar` / `.war` / `.ear` (per resolved clarification Q2): extract entry bytes
into a `Vec<u8>`, increment depth, recurse with depth check `if depth >= 8 { warn!(...); return; }`.

**Rationale**: SHA-256-keyed visited set mirrors the milestone-128 include-chain cycle protection. The
`zip::ZipArchive::new(Cursor::new(&[u8]))` pattern works because `Cursor<&[u8]>` implements both `Read`
and `Seek`. Depth limit at 8 matches `INCLUDE_DEPTH_LIMIT` from milestone 128.

**1 GB decompressed-size cap**: enforced per-entry via `zip::read::ZipFile::size()` (the central-directory
declared uncompressed size). Entries declaring size >1 GB are skipped with a `warn` log; never extracted.

**Alternatives considered**:

- Use `walkdir` against a tempdir + `unzip` shellout: violates `--offline` (subprocess); adds runtime
  filesystem cost; tempdir cleanup is complicated.
- Add a `zip_bomb_detector` crate: not maintained; we can inline the check with five LOC.

## R7. Audit-image probe

**Decision**: The audit's syft run (against `remediation-planner:latest`) produced 1,489 NuGet hits and
986 Cargo hits. mikebom's SC-001/002/003 acceptance targets are set at 95% of these (1,415 and 937
respectively). The targets are well within reach because the underlying input artifacts (`.deps.json`
files, `cargo auditable`-built binaries) are deterministic — both tools should hit the same enumeration
count once the reader exists.

**Audit-image probe paths**:

- `.deps.json` discovery: walked via `safe_walk` rooted at the image rootfs; matched by extension
  (`.endswith(".deps.json")`). Expected count for `remediation-planner:latest`: ~50 `.deps.json` files,
  collectively listing ~1,500 NuGet libraries.
- `cargo-auditable` binary discovery: walked via `safe_walk`; matched by ELF magic byte sequence (the
  existing milestone-096 `is_elf` helper from `scan_fs/binary/symbol_fingerprint.rs`). Expected count:
  ~3 binaries with `.dep-v0` sections (`uv`, `uvx`, and one or two others).
- Nested JAR discovery: walked at top level via the existing milestone-009 maven reader; the milestone-129
  extension recurses INSIDE each detected JAR.

## R8. SPDX 3 + CDX native typed-edge model (re-confirming milestone 052)

**Decision**: The existing `ResolvedComponent` struct already carries a `lifecycle_scope:
Option<LifecycleScope>` field (variants: `Runtime`, `Build`, `Test`, `Optional`). The emission paths in:

- `mikebom-cli/src/generate/cyclonedx/builder.rs` — translates `Build`/`Test` to CDX `scope: "optional"`,
  `Runtime` to `scope: "required"`. Native CDX 1.6 field.
- `mikebom-cli/src/generate/spdx/document.rs` — translates `Build`/`Test` to SPDX 2.3 typed relationships
  `BUILD_DEPENDENCY_OF` / `TEST_DEPENDENCY_OF`.
- `mikebom-cli/src/generate/spdx/v3_document.rs` — translates to SPDX 3 `LifecycleScopeType`.

All three paths already exist and are tested. Milestone 129's cargo-auditable reader only needs to
populate `lifecycle_scope` on the emitted `ResolvedComponent`s; the rest is unchanged.

**Validation**: `grep -rn "lifecycle_scope" mikebom-cli/src/generate/` lists all three emission sites.

## R9. mikebom's polyglot scanner pipeline vs eBPF trace pipeline (Principle II clarification)

**Decision**: This milestone targets the **`mikebom sbom scan` polyglot scanner pipeline**, NOT the
`mikebom trace` eBPF pipeline. Principle II / Strict Boundary #1 govern the latter.

**Rationale**: The codebase has two top-level commands:

- `mikebom trace` (binary entrypoint `mikebom::cli::trace_cmd`) — the eBPF pipeline. Implements Principles
  II + III + VIII + XII. Source manifests are explicitly forbidden as a dependency-source.
- `mikebom sbom scan` (binary entrypoint `mikebom::cli::scan_cmd`) — the user-space FS scanner. Reads
  lockfiles, manifests, binaries as primary dependency sources. Originated in milestone 002 and has been
  the path of every milestone since (cargo, maven, npm, gem, pip, yocto, etc.).

The constitution does not currently expressly say "`sbom scan` reads source as a primary signal" — that's
documented by the source tree itself. This plan calls out the distinction explicitly so reviewers don't
mis-cite Strict Boundary #1 as a milestone-129 blocker. No constitution amendment is requested; the
existing model holds.

## R10. The 4 new `mikebom:*` annotation keys — Principle V audits

For the catalog committed to `docs/reference/sbom-format-mapping.md` (C-rows C87..C90):

### C87: `mikebom:assembly-version-informational`

- **CDX 1.6 native equivalent?** `version` field is single-valued. No.
- **SPDX 2.3 native equivalent?** `Package.versionInfo` is single-valued. No.
- **SPDX 3 native equivalent?** `software_packageVersion` is single-valued. No.
- **Verdict**: Valid parity-bridging extension per Principle V's "finer-grained information the standard
  does not express" carve-out. Catalogued with audit narrative.

### C88: `mikebom:assembly-version-file`

Same as C87. Three orthogonal version fields exist on .NET assemblies; preserving them all is
finer-grained than any of the three formats natively supports.

### C89: `mikebom:assembly-version-runtime`

Same as C87/C88. Preserves the binding-time `AssemblyVersion` for vulnerability research that may key on
that value rather than the published NuGet version.

### C90: `mikebom:image-presence`

- **CDX 1.6 native equivalent?** No. CDX has `compositions[].aggregate` (`complete` / `incomplete`) at
  the document level but no per-component "declared vs installed" boolean.
- **SPDX 2.3 native equivalent?** No. The `Package.filesAnalyzed` field controls per-package file
  analysis depth, not declaration vs installation.
- **SPDX 3 native equivalent?** No.
- **Verdict**: Valid parity-bridging extension per the carve-out.

All four catalog entries will be emitted as SymmetricEqual across CDX + SPDX 2.3 + SPDX 3 (the standard
SBOM-format-mapping shape).

## Decisions summary

| ID | Topic | Decision | Status |
|---|---|---|---|
| R1 | `.deps.json` parse depth | `libraries` map only (skip `targets` until Phase 11) | Decided |
| R2 | CLR metadata reader | Hand-roll on `object`'s PE primitives (~150 LOC, zero new deps) | Decided |
| R3 | Managed-assembly detection | `DataDirectory[14]` non-zero `VirtualAddress` + `Size` | Decided |
| R4 | cargo-auditable parse | Raw deflate of `.dep-v0` section; `serde_json` into typed struct | Decided |
| R5 | cargo `kind` → lifecycle_scope | runtime→Runtime, build→Build, dev→Test (clarification Q1) | Decided |
| R6 | Nested-JAR walker | SHA-256 visited set + 8-level depth + 1 GB cap; `.jar`/`.war`/`.ear` only (Q2) | Decided |
| R7 | Audit-image probe | SC-001/002/003 targets set at 95% of syft's audit counts | Decided |
| R8 | Native typed-edge re-confirm | `ResolvedComponent.lifecycle_scope` drives CDX `scope` + SPDX rel-types | Decided |
| R9 | Pipeline disambiguation | `sbom scan` = polyglot scanner; Principle II governs `trace` only | Decided |
| R10 | 4 new mikebom:* keys audit | All four valid per parity-bridging carve-out; catalogue with narratives | Decided |
