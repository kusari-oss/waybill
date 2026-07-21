# waybill component tiers

waybill emits THREE component tiers in its SBOMs. Understanding the
tier distinction lets you query the SBOM efficiently and interpret
what each component represents.

This reference doc is cited from PR reviews of code touching file-tier
emission. The behavioral contract here is normative; the catalog at
`docs/reference/sbom-format-mapping.md` is the per-annotation wire-shape
source of truth.

> **Not the same as the `sbom_tier` traceability ladder.** This file
> covers the **component-tier axis** (package / binary / file — what
> kind of *evidence* backs the component). The **`sbom_tier`
> axis** (design / source / analyzed / deployed / build — how strongly
> the resolved *version* is pinned) is orthogonal and documented at
> [reading-a-waybill-sbom.md → Design-tier components](reading-a-waybill-sbom.md).
> Milestone 175 adds an INFO-level advisory log at scan time when the
> scan produces ≥1 `sbom_tier = "design"` component; the advisory can
> be suppressed in CI via `WAYBILL_NO_DESIGN_TIER_ADVISORY=1`.

## The three tiers

### Tier 1: Package-tier

A component representing a known package (cargo crate, npm package,
deb / rpm / apk package, NuGet assembly, Maven artifact, Go module,
PyPI package, RubyGems gem, etc.) identified by its PURL.

- **Identity**: Package URL (PURL) following the PURL specification.
- **Discovery**: package-DB readers (apk, dpkg, rpm), manifest /
  lockfile parsers (Cargo.lock, package-lock.json, requirements.txt,
  pom.xml, etc.), PE/CLR metadata reader, Go binary `BuildInfo`
  extractor, cargo-auditable parser.
- **CDX type**: `library` (or `application` / `operating-system`
  for OS-level packages).
- **SPDX 2.3**: `Package` with `externalRefs[purl]`.
- **SPDX 3**: `software_Package` with `software_packageUrl`.
- **Properties** (milestone 133): every reader populates
  `waybill:source-files` (JSON-encoded array of rootfs-relative
  paths) and the CDX-native `evidence.occurrences[].location` for
  cross-format consumers. Image scans additionally carry
  `waybill:layer-digest` (the OCI layer SHA-256 that wrote the
  component's primary source path).

### Tier 2: Binary-tier

A component representing an identified binary artifact that doesn't
map to a known package, but whose identity has been derived via
symbol fingerprinting, embedded metadata (Go BuildInfo,
cargo-auditable), or content-hash matching against a curated corpus.

- **Identity**: synthesized PURL (e.g. `pkg:generic/<name>@<version>`)
  with hash-based fallback when the fingerprint matcher returns
  high-confidence but no canonical identity.
- **Discovery**: milestone-099 symbol fingerprinter; milestone-104
  binary-role classifier; milestone-108 / -110 fingerprint corpus
  matchers; milestone-098 compiler-version extractor.
- **CDX type**: `library`, `application`, or `file` depending on
  the binary role inferred from headers + section table.
- **SPDX 2.3**: `Package`.
- **SPDX 3**: `software_Package` (or `software_File` when the
  binary role is `Object` / file-shaped).
- **Hash**: per-file SHA-256 always populated (milestone 038
  onward).

### Tier 3: File-tier (NEW in milestone 133)

A component representing content on the rootfs that survived every
package-tier and binary-tier reader. Identity is the file's SHA-256.

- **Identity**: SHA-256 hash. NO PURL (FR-009). The in-process
  `ResolvedComponent` carries a placeholder
  `pkg:generic/file-tier?content-sha256=<hex>` for type uniformity
  with the rest of the resolver pipeline; the per-format emitters
  recognize the `waybill:component-tier = "file"` annotation and
  STRIP the PURL field at write time, so the wire shape honors
  FR-009.
- **Discovery**: rootfs walker (`scan_fs::file_tier::walker`)
  applies the FR-005 content-shape allowlist + path-prefix
  exclusion list, then runs the FR-011 hybrid dedupe (path
  coverage from `ResolvedComponent.occurrences[].location` —
  populated by US2.3 for 99.96 % of audit-baseline components —
  OR hash coverage from binary-tier `hashes[]` SHA-256 entries).
- **CDX type**: `file`.
- **SPDX 2.3**: `Package` with `filesAnalyzed: false` and no
  `externalRefs[purl]`.
- **SPDX 3**: `software_File` (the native element type per the
  SPDX 3.0.1 Software profile).
- **Annotation**: every file-tier component MUST carry
  `waybill:component-tier = "file"` for unambiguous
  identification across formats. The SPDX 2.3 `Package` shape is
  otherwise indistinguishable from a package-tier `Package`; the
  annotation is the cross-format-symmetric tier signal.
- **Paths**: every observed path is carried in
  `waybill:file-paths` as a JSON-encoded sorted array, capped at
  100 entries; truncation fires the companion
  `waybill:file-paths-truncated = "true"` annotation.

## How the tiers compose

For a given file on a rootfs scan, the precedence is:

1. **Package-tier readers run first**. If a package-DB or
   manifest claims a path or contains the file's content, the
   package-tier component is emitted with its PURL identity.
   Every reader records the on-disk path it parsed via
   `evidence.occurrences[].location` (CDX-native + SPDX
   annotation envelope) and `waybill:source-files`.

2. **Binary-tier readers run second**. For files surviving step 1
   that pass binary-tier discovery criteria (ELF / PE / Mach-O
   magic + fingerprint match), a binary-tier component is
   emitted with its hash and inferred binary role.

3. **File-tier walker runs last** in default (`orphan`) mode.
   For files surviving steps 1 and 2 AND passing the FR-005
   content-shape allowlist AND failing the FR-011 hybrid dedupe
   (path NOR hash covered), a file-tier component is emitted.

In `--file-inventory=full` mode, step 3 emits per-unique-hash
file-tier components for every file passing the content-shape
allowlist regardless of package or binary coverage. Duplicates
with package-tier components are EXPECTED in this mode; the SBOM
carries a document-level `waybill:file-inventory-mode = "full"`
annotation per Constitution Strict Boundary §5 (1.5.0) so
consumers can detect the override at parse time and filter the
file-tier set when the duplication is unwanted.

## Orphan content-shape allowlist (FR-005)

Default-mode file-tier emission applies a content-shape allowlist
to avoid flooding SBOMs with source code, docs, and configs.
Files qualify ONLY when they match one of these shapes:

- **Unattributed ELF / Mach-O / PE binary** — magic-number probe
  (first 4 bytes).
- **Unattributed shared library** — `.so` / `.so.*` (versioned) /
  `.dylib` / `.dll` extension OR file-magic match.
- **Unattributed archive** — `.jar` / `.war` / `.ear` / `.deb` /
  `.rpm` / `.apk` / `.tar` / `.tgz` / `.tar.xz` / `.tar.bz2` /
  `.zip` extension.
- **Lone package manifest** — `Cargo.toml`, `package.json`,
  `pom.xml`, `requirements.txt`, `Gemfile`, `go.mod` WITH NO
  ADJACENT LOCKFILE in the same directory (or any parent up to
  the workspace root for `Cargo.toml`; for `pom.xml`, a sibling
  `target/` build-output directory disqualifies the manifest
  too).
- **Executable script** — first 2 bytes = `#!`.

Files NOT matching any shape are skipped silently.

### Path-prefix exclusion (FR-005 post-tightening)

Even when a file passes the content-shape allowlist, file-tier
emission is SKIPPED when the file lives under one of these
well-known package install roots:

```
**/dotnet/packs/**
**/dotnet/shared/**
**/dotnet/sdk/**
**/dotnet/store/**
**/usr/share/dotnet/**
**/node_modules/**
**/lib/python*/site-packages/**
**/.cargo/registry/**
**/ruby/gems/**
**/jvm/openjdk*/lib/**
```

Rationale: these are install roots where a package-tier reader
DOES know the package identity (via PURL) but doesn't yet emit
per-file path coverage on every entry. The exclusion list is a
pragmatic stop-gap until per-reader path tracking expands.
Surfaced via FR-022's measure-first projection during milestone-
133 planning — see `specs/133-file-tier-components/research.md
§Orphan projection` for the empirical justification.

## Content shapes EXPLICITLY excluded from orphan mode

These shapes are NEVER orphan-emitted, regardless of path:

- **Source code**: `.rs`, `.py`, `.go`, `.c`, `.cpp` / `.cc` /
  `.cxx`, `.h` / `.hpp`, `.cs`, `.java`, `.js`, `.ts` / `.jsx` /
  `.tsx`, `.rb`, `.php`, `.swift`, `.kt` / `.kts`, `.scala`,
  `.clj`, `.ex` / `.exs`, `.erl`, `.lua`, `.pl` / `.pm`.
- **Plain text / docs**: `.md`, `.txt`, `.rst`, `.adoc` /
  `.asciidoc`, `.tex`.
- **Structured config (not archives)**: `.json`, `.yaml` / `.yml`,
  `.toml`, `.ini`, `.conf` / `.cfg`, `.xml` (when not a known
  archive shape).
- **Build scaffolding**: `Dockerfile`, `Makefile`, `Rakefile`,
  bare `Gemfile` (without adjacent lockfile suppression — handled
  by the lone-manifest branch above), `.lock`, `.sum`, `.list`.

Rationale: these shapes are pure noise for SBOM consumers. Vuln
scanners don't key on `.md` files; license auditors don't key on
`.py` source. The exclusions keep the orphan output signal-dense.

## Full mode (`--file-inventory=full`)

Opt-in via `--file-inventory=full`. Emits per-unique-hash
file-tier components for every file passing the content-shape
allowlist (path-prefix exclusion + adjacent-lockfile check still
apply; hybrid dedupe is BYPASSED). Targeted use cases:

- **Forensics**: "is `sha256:abc…` (a known IOC) present anywhere
  on this image?" — one component lookup against the document's
  file-tier set answers the question.
- **Image diff**: full-mode SBOMs from two image versions show
  file-level deltas — newly added, modified, removed file
  hashes.
- **Malware detection**: a shared "bad" file appearing across
  multiple package contexts shows up as ONE file-tier component
  with all paths in `waybill:file-paths`.

Full-mode SBOMs carry a document-level
`waybill:file-inventory-mode = "full"` annotation per
Constitution Strict Boundary §5 (1.5.0). Consumers MAY use this
annotation to detect that the SBOM contains duplicate
(file-tier × package-tier) coverage of the same content.

## Worked examples

### Example: orphan ELF binary, default mode

A statically-linked `curl` at `/usr/local/bin/curl-vendored` not in
any package DB and not matched by binary-tier fingerprinting.

| Format | Wire shape |
|---|---|
| CDX 1.6 | `{ "type": "file", "name": "curl-vendored", "bom-ref": "...", "hashes": [{"alg":"SHA-256","content":"<hex>"}], "properties": [{"name":"waybill:component-tier","value":"file"},{"name":"waybill:file-paths","value":"[\"usr/local/bin/curl-vendored\"]"}] }` |
| SPDX 2.3 | `{ "SPDXID":"SPDXRef-Package-...", "name":"curl-vendored", "versionInfo":"", "filesAnalyzed":false, "checksums":[{"algorithm":"SHA256","checksumValue":"<hex>"}], "annotations":[{"comment":"…\"waybill:component-tier\"…\"file\"…"},{"comment":"…\"waybill:file-paths\"…"}] }` |
| SPDX 3 | `{ "type":"software_File", "spdxId":".../pkg-…", "name":"curl-vendored", "verifiedUsing":[{"type":"Hash","algorithm":"sha256","hashValue":"<hex>"}] }` + a paired `Annotation` element carrying `waybill:component-tier` + `waybill:file-paths`. |

### Example: package-tier component (cargo crate)

`pkg:cargo/serde@1.0.197` discovered from `app/Cargo.lock`.

| Format | Wire shape (abridged) |
|---|---|
| CDX 1.6 | `{ "type":"library", "name":"serde", "version":"1.0.197", "purl":"pkg:cargo/serde@1.0.197", "evidence":{"occurrences":[{"location":"app/Cargo.lock","additionalContext":"{\"sha256\":\"<hex>\"}"}]}, "properties":[{"name":"waybill:source-files","value":"[\"app/Cargo.lock\"]"}] }` |
| SPDX 2.3 | `{ "name":"serde", "versionInfo":"1.0.197", "externalRefs":[{"referenceCategory":"PACKAGE-MANAGER","referenceType":"purl","referenceLocator":"pkg:cargo/serde@1.0.197"}], "annotations":[{"comment":"…waybill:source-files…"}] }` |
| SPDX 3 | `{ "type":"software_Package", "name":"serde", "software_packageVersion":"1.0.197", "software_packageUrl":"pkg:cargo/serde@1.0.197", "externalIdentifier":[{"externalIdentifierType":"packageUrl","identifier":"pkg:cargo/serde@1.0.197"}] }` + paired Annotation for `waybill:source-files`. |

### Example: binary-tier component (cargo-auditable extract)

`/usr/bin/uv` is a Rust binary; its cargo-auditable section
exposes its dependency tree. Each declared crate emits as a
binary-tier component.

| Format | Wire shape (abridged) |
|---|---|
| CDX 1.6 | `{ "type":"library", "name":"uv", "version":"0.4.27", "purl":"pkg:cargo/uv@0.4.27", "evidence":{"occurrences":[{"location":"usr/bin/uv","additionalContext":"…"}]}, "properties":[{"name":"waybill:binary-class","value":"application"},{"name":"waybill:cargo-auditable-kind","value":"runtime"}] }` |
| SPDX 2.3 | `Package` with the same purl externalRef + the binary-class / cargo-auditable annotations. |
| SPDX 3 | `software_Package` with `software_primaryPurpose: "application"` set from the binary-role classifier. |

## Trivy lesson: path / layer context on package-tier (US2)

This milestone takes an idea from trivy's design: every
package-tier component identified from a rootfs path carries
`waybill:source-files` (the relative rootfs path) and, for image
scans, `waybill:layer-digest` (the OCI layer digest containing
the path). Trivy proves these properties are low-cost /
high-value for forensic / diff / supply-chain queries.

This is a DIFFERENT choice than trivy's per-(package × path)
component duplication. waybill continues to dedupe package-tier
components by their PURL identity; when a package is identified
from multiple paths in a single scan, the paths collapse into the
`waybill:source-files` array (sorted JSON array) and into the
CDX-native `evidence.occurrences[]` list. Same shape as US1's
file-tier `waybill:file-paths` for symmetry.

## Why waybill rejected the alternative designs

Two adjacent industry designs were considered and rejected during
milestone-132 close-out research. Their rationale lives here so
future contributors don't re-litigate.

### Syft model (per-(path × hash) file emission)

Syft emits one file-tier component per (path × hash) tuple — the
same file at two paths shows up as two components. This achieves
5★ Completeness on the sbom-comparison scorecard but pumps SBOM
size up dramatically (27 006 file entries vs ~3 770 package
entries on the milestone-132 audit baseline). waybill chose
per-unique-hash with paths-as-property to preserve the
malware / forensic query surface without the SBOM-bloat cost.

### Trivy model (per-(package × path), no file-tier components)

Trivy emits package-tier components only, but DUPLICATES the
package when it's identified from multiple paths (581 components
on the audit baseline; the same
`@smithy/is-array-buffer@2.2.0` appears twice with different
`FilePath` properties). Trivy bets that "the package is the unit
of analysis"; waybill chose to surface BOTH (package-tier with
`waybill:source-files` + `evidence.occurrences[]` collection,
plus file-tier for unattributed content) so consumers can query
by either.

## Related milestones

- Milestone 038 — Per-file SHA-256 deep-hash. Established the
  hash-based identity that file-tier emission inherits.
- Milestone 104 — Binary-tier component role classification.
  Established the binary-tier-vs-package-tier distinction this
  milestone extends.
- Milestone 130 / 131 — PE/CLR managed-assembly metadata +
  license-coverage backfill. Identified the `dotnet/packs/`
  over-emission risk that motivated FR-005's path-prefix
  exclusion list.
- Milestone 132 — Closeout of milestone 131 SC misses. The
  Completeness 1★ vs 5★ gap surfaced during 132's audit-baseline
  measurement; milestone 133 is the structural response.
- Milestone 133 PRs:
  - US1.A scaffolding (#387)
  - US1.B opt-in MVP (#388)
  - US1.C SPDX parity + default flip (#389)
  - US2.1 source-files defects (#384)
  - US2.2 layer-digest (#385)
  - US2.3 evidence.occurrences[] (#386)
  - US3 transparency annotations (#390)
  - US4 constitution amendment + this doc (TBD)
