# Compiled-binary identity and provenance

Every binary Waybill scans carries a cross-platform identity in the
SBOM. This page is the reference for the per-format binary-identity
annotations, Go VCS provenance, Rust crate-closure provenance, and
the curated embedded-version-string detector. For where each
annotation lands per output format, see
[SBOM format mapping](sbom-format-mapping.md).

## Per-format binary identity

- **ELF (Linux):** `NT_GNU_BUILD_ID`, `DT_RPATH` / `DT_RUNPATH`,
  `.gnu_debuglink` → `waybill:elf-build-id` /
  `waybill:elf-runpath` / `waybill:elf-debuglink`. The build-id is
  the canonical Linux binary-identity field used by `eu-unstrip`,
  `coredumpctl`, `debuginfod`, and `*-dbgsym` packaging.
- **Mach-O (macOS / iOS):** `LC_UUID`, `LC_RPATH`, and minimum-OS
  version (`LC_BUILD_VERSION` or `LC_VERSION_MIN_*`) →
  `waybill:macho-uuid` / `waybill:macho-rpath` /
  `waybill:macho-min-os`. The UUID is what `dwarfdump`,
  `xcrun symbolicatecrash`, the macOS crash reporter, and every
  `*.dSYM` bundle key on for symbol matching. Fat / universal
  binaries report from the first slice.

  Plus codesign metadata from the `LC_CODE_SIGNATURE` SuperBlob's
  CodeDirectory: `waybill:macho-codesign-identifier` (e.g.
  `com.apple.ls`), `waybill:macho-codesign-flags` (decoded names
  from the flags bitfield — `hardened-runtime`,
  `library-validation`, `adhoc`, etc.), and
  `waybill:macho-codesign-team-id` (10-char Apple Team ID for
  developer-signed binaries). This is what `codesign -dvv` reads.
- **PE (Windows):** CodeView pdb-id (`<guid>:<age>` from
  `IMAGE_DIRECTORY_ENTRY_DEBUG`), machine type
  (`IMAGE_FILE_HEADER.Machine`), and subsystem
  (`IMAGE_OPTIONAL_HEADER.Subsystem`) → `waybill:pe-pdb-id` /
  `waybill:pe-machine` / `waybill:pe-subsystem`. The pdb-id is
  what Microsoft Symbol Server, Mozilla / Chromium symbol stores,
  WinDbg, and drmingw use to locate matching `.pdb` files.

All twelve annotations emit symmetrically across CDX, SPDX 2.3,
and SPDX 3, making cross-image binary dedup, debug-symbol
correlation, and signing-identity provenance a direct lookup
regardless of OS.

## Go VCS provenance

Waybill extracts `vcs.revision` (commit SHA), `vcs.time` (RFC 3339
build timestamp), and `vcs.modified` (dirty-tree flag) from every Go
binary's BuildInfo. Surfaced as `waybill:go-vcs-revision` /
`waybill:go-vcs-time` / `waybill:go-vcs-modified` on the main-module
entry. Same data `go version -m` shows, baked into the SBOM so
consumers don't have to shell out.

## Rust crate-closure provenance

Waybill extracts the full build-time crate dependency closure from
the `.dep-v0` linker section that
[`cargo auditable build`](https://github.com/rust-secure-code/cargo-auditable)
embeds. Each crate becomes a `pkg:cargo/<name>@<version>` component
with `evidence-kind = "cargo-auditable"` and `confidence = "high"`
(build-time truth — distinct from `embedded-version-string`'s
heuristic tier), `parent_purl` cross-linking back to the file-level
binary. The binary itself gains a
`waybill:detected-cargo-auditable = true` cross-link annotation
(the Rust analog of `waybill:detected-go = true`). Cargo wrappers
shipped with **Debian Trixie+, Fedora 40+, Alpine Edge, and the
official Rust container images** auto-enable the embedding, so most
Rust binaries built in those environments surface their full crate
closure without source access. Cross-format: ELF / Mach-O / PE.

## Embedded-version-string detection

Curated detection for **11 high-CVE-volume native libraries**
statically-linked into compiled binaries — the heuristic-tier
counterpart to source-tree manifest parsing. Waybill walks the
binary's read-only string region (`.rodata` / `__TEXT,__cstring` /
`.rdata` — never the full image, to bound the false-positive
surface) and recognises each library's canonical version banner
anchored at a NUL boundary:

- **Crypto / TLS:** OpenSSL, BoringSSL, LibreSSL, GnuTLS
- **Compression / data:** zlib, SQLite
- **Networking:** curl
- **Regex:** PCRE, PCRE2
- **Compiler / runtime:** LLVM, OpenJDK (handles both modern JEP-322
  `21.0.1+12` and legacy Java-8 `8u362-b09`)

Each detection emits a `pkg:generic/<library>@<version>` component
with `waybill:evidence-kind = "embedded-version-string"` and
`waybill:confidence = "heuristic"`, so downstream CVE matchers
(Vex / OSV / NVD / Kusari Inspector) have pre-resolved coordinates
to query against — no need to know in advance which libraries a
binary statically links.
