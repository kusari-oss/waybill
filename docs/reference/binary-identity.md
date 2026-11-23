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

Curated detection for **18 native libraries and runtimes** that are
commonly linked into or bundled with compiled binaries — the
heuristic-tier counterpart to source-tree manifest parsing. Waybill
walks the binary's read-only string region (ELF `.rodata` and
`.data.rel.ro`, Mach-O `__cstring` and `__const`, PE `.rdata` — never
the full image, to bound the false-positive surface) and recognises
each library's canonical version banner:

- **Crypto / TLS:** OpenSSL, BoringSSL, LibreSSL, GnuTLS
- **Compression / data:** zlib, SQLite, expat, bzip2, liblzma (xz)
- **Arithmetic:** GMP
- **Terminal:** ncurses
- **Networking:** curl
- **Regex:** PCRE, PCRE2
- **Compiler / runtime:** LLVM, OpenJDK (handles both modern JEP-322
  `21.0.1+12` and legacy Java-8 `8u362-b09`), PHP, Node.js

What may precede a banner depends on how much of it is fixed (spec
1643, #1568, #1641):

- **zlib, expat and ncurses** are fixed at both ends: the library's
  name starts the banner, and a fixed suffix (zlib's ` Copyright`) or a
  required NUL ends it. Any preceding byte except a letter, digit or
  `_` is accepted, because those sections mix strings with other
  constant data and a real banner can follow any byte. For zlib the
  test applies to the byte before its leading space.
- **The other twelve** must start a string: the byte before the banner
  is NUL. Their banners end at any byte that is not part of a version,
  so without the NUL rule a mention inside another program's text
  (`requires OpenSSL 1.1.1 or later`) would match.

**bzip2, GMP and liblzma** state their version with no name beside it
(spec 1644, #1641): bzip2 as `1.0.8, 13-Jul-2019`, GMP and liblzma as a
bare `6.3.0` or `5.8.1`. A bare version names nothing, so each is read
only when the same binary also holds the library's **anchor**:

| Library | Anchor | Version string |
|---|---|---|
| bzip2 | the text `bzip2/libbzip2` (from its internal-error message) | `X.Y.Z, DD-Mon-YYYY`; the version is `X.Y.Z` |
| GMP | the text `GNU MP:` (from its allocation-failure messages) | `X.Y.Z` |
| liblzma | a **defined** `lzma_version_string` or `lzma_version_number` symbol: in an ELF file's dynamic or static symbol table (`.dynsym` or `.symtab`), a Mach-O external that is not undefined, or a PE export | `X.Y.Z`, optionally followed by `alpha` or `beta` |

The rules for the version string:

- It must be a whole string. It ends in NUL, and it is preceded by NUL,
  by the start of the region, or by a byte that is not printable text,
  because a constant table can end right before it, as GMP's does.
  A version inside text (`requires 6.3.0 or later`) is never read.
- With exactly one such string, the library is versioned. With two or
  more, it gets no version; one is never chosen.
- GMP and liblzma share the `X.Y.Z` form. A string that both of them,
  anchored in the same binary, would accept versions neither.
- A program that only *imports* liblzma's symbols does not anchor it,
  from either symbol table.
- A statically linked liblzma is versioned when the program is not
  stripped: a static link leaves liblzma's symbols in `.symtab` (spec
  1647, #1664). If the same binary also holds GMP's anchor text, a
  version string both libraries accept versions neither, as above.
- A **stripped** static liblzma gets no version. It keeps its version
  string, but no symbol and no text that names the library. The xz
  stream header magic it carries is not used as an anchor: 7-Zip carries
  the same bytes, with its own xz code, beside a single bare `13.2.0`,
  its compiler's version.
- A fat Mach-O gets no liblzma version. The PE path is not covered by a
  test fixture.

The component names, `bzip2`, `gmp` and `liblzma`, are the names of the
libraries' upstream pkg-config files. So when a tree also ships
`gmp.pc` or `liblzma.pc` at the same version, both pieces of evidence
land on one component. The reason a binary got no version (no
candidate, ambiguous, or shared) is reported by the
`waybill::version_census` debug log, not in the document.

**LLVM and ncurses report the version their banner states** (spec 1645,
#1662). That includes LLVM's suffix (`LLVM version 23.0.0git` gives
`23.0.0git`, and `18.1.0rc1` stays `18.1.0rc1`) and ncurses' patch date
(`ncurses 6.6.20251230` gives `6.6.20251230`). The suffix is lowercase
letters and digits attached to `X.Y.Z` and ended by a NUL or a space.
For ncurses, the CPE follows NVD's form, whichever reader found the
component: the release goes in the version field and the patch date in
the update field (`cpe:2.3:a:invisible-island:ncurses:6.6:20251230:…`).

**When a pkg-config or CMake record names the same library
differently** (spec 1645, #1662), the two become one component when all
of these hold:

- the record's own library files include the file the banner was read
  from;
- the record's name is one of the library's names. For ncurses that is
  `ncurses`, `ncursesw`, `ncursest` or `ncursestw`, the names ncurses'
  own build gives it (its `INSTALL`: `--enable-widec` appends `w`,
  `--with-pthread` appends `t`). Every other library has only its own
  name, so a record for a library that bundles zlib is never merged
  into zlib;
- the versions agree: equal, or the banner's version followed by a `.`
  begins the record's (`2.7.5` agrees with `2.7.5.1`, not with
  `2.7.50`);
- exactly one record qualifies. When two do, neither is chosen.

The merged component takes the library's name, which is the name the
CPE table knows, and the record's version. It lists both readers'
files, and it keeps the record's source mechanism and tier. Edges that
named the record follow it. Each such file and its verdict
(`merge`, `version-disagrees`, `name-unrelated` or `ambiguous`) is
reported by the `waybill::version_census` debug log as `shared_file`.

**A merged component keeps the binary match visible** (spec 1646,
#1665). A banner or symbol match is identified by `binary-analysis`, the
CycloneDX 1.6 technique, in `evidence.identity[].methods[]`. When it
merges into a pkg-config or CMake component, which keeps that reader's
`manifest-analysis`, the merged component lists both methods; SPDX 2.3
and SPDX 3 add the same `methods` to their `evidence.identity`
annotation. The merged component's `waybill:evidence-kind` and
`waybill:confidence` stay the winning reader's. The
`waybill::version_census` debug log reports each such merge as
`binary_merge`.

Each detection emits a `pkg:generic/<library>@<version>` component
with `waybill:evidence-kind = "embedded-version-string"` and
`waybill:confidence = "heuristic"`, so downstream CVE matchers
(Vex / OSV / NVD / Kusari Inspector) have pre-resolved coordinates
to query against — no need to know in advance which libraries a
binary statically links.
