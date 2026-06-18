# Annotation schema contract — milestone 129

Four new `mikebom:*` annotation keys, all component-scope, all `SymmetricEqual` across
CDX + SPDX 2.3 + SPDX 3 per the existing milestone-128 parity-extraction convention. Catalogued into
`docs/reference/sbom-format-mapping.md` as C-rows C87..C90 (final numbering subject to in-flight
milestone collisions; tasks.md will pin the exact range at implementation time).

## C87: `mikebom:assembly-version-informational`

**Scope**: component

**Value**: the verbatim string from the .NET assembly's `AssemblyInformationalVersionAttribute` custom
attribute. Example: `"8.0.127-servicing.26230.7+sha.a1b2c3d"`.

**Emitted by**: PE/CLR managed-assembly reader (US1, FR-010).

**Principle V audit**:

- CDX 1.6 native equivalent? `version` is single-valued. **No.**
- SPDX 2.3 native equivalent? `Package.versionInfo` is single-valued. **No.**
- SPDX 3 native equivalent? `software_packageVersion` is single-valued. **No.**

**Verdict**: Valid parity-bridging extension per the Principle V "finer-grained information the standard
does not express" carve-out. The single native version field is consumed by mikebom's PURL-version
selection ladder (`InformationalVersion → FileVersion → AssemblyVersion`); the other two versions
remain available for audit / forensics via the three `mikebom:assembly-version-*` annotations.

## C88: `mikebom:assembly-version-file`

**Scope**: component

**Value**: the verbatim string from the .NET assembly's `AssemblyFileVersionAttribute` custom attribute.
Example: `"8.0.127.26230"` (4-tuple build-bearing version; what Windows Explorer's "File version"
property displays).

**Emitted by**: PE/CLR managed-assembly reader (US1, FR-010).

**Principle V audit**: same as C87. **Valid parity-bridging extension.**

## C89: `mikebom:assembly-version-runtime`

**Scope**: component

**Value**: the rendered 4-tuple from the assembly's `Assembly` metadata-table row's
`MajorVersion`/`MinorVersion`/`BuildNumber`/`RevisionNumber` columns. Example: `"8.0.127.0"`. This is
the CLR-binding-relevant version (what the runtime uses for assembly-binding decisions).

**Emitted by**: PE/CLR managed-assembly reader (US1, FR-010).

**Principle V audit**: same as C87. **Valid parity-bridging extension.**

## C90: `mikebom:image-presence`

**Scope**: component

**Values**: `"installed"` (default; assembly file is present in the rootfs at the declared path),
`"declared-not-installed"` (the `.deps.json` declares the package but the corresponding assembly file
is not present in the rootfs).

**Emitted by**: `.deps.json` reader (US1, edge-case handling).

**Principle V audit**:

- CDX 1.6 native equivalent? `compositions[].aggregate` (`complete` / `incomplete`) is **document-scope**,
  not per-component. **No per-component equivalent.**
- SPDX 2.3 native equivalent? `Package.filesAnalyzed` controls per-package file-analysis depth, not
  declaration vs installation status. **No.**
- SPDX 3 native equivalent? **No.**

**Verdict**: Valid parity-bridging extension per the Principle V carve-out. Useful for vulnerability
review of containerized .NET applications where the `.deps.json` is the authoritative declaration but
the actual installed-state may differ (e.g. `dotnet publish --self-contained false` ships `.deps.json`
without bundling the framework assemblies).

## C91: `mikebom:cargo-source-mechanism`

**Scope**: component

**Values**: `"local-path"` (the crate is a path-dependency declared via `[dependencies] foo = { path = "..." }`
in the consuming crate's `Cargo.toml`; the cargo-auditable wire field is `source: "local"`).

**Emitted by**: cargo-auditable binary reader (US2, FR-018) — emitted ONLY when the cargo-auditable
`source` field is `"local"`. For `crates-io` / `git` / `unknown` sources, the annotation is omitted
(absence implies non-local, the common case).

**Principle V audit**:

- CDX 1.6 native equivalent? `pkg:cargo/...` PURLs don't encode a path-vs-registry distinction. The
  CDX `evidence.identity[].technique` field could in principle carry "filename" vs "manifest" but is
  scoped to evidence-of-identity (how the SBOM tool detected the component), not to the upstream
  declaration mechanism (whether the maintainer wrote a path dep or a registry dep). **No native
  equivalent.**
- SPDX 2.3 native equivalent? `Package.downloadLocation = "NOASSERTION"` is the only conventional
  signal for a path-dep, and it's lossy (also fires for `unknown` sources). **No precise native
  equivalent.**
- SPDX 3 native equivalent? Same as SPDX 2.3. **No.**

**Verdict**: Valid parity-bridging extension per the Principle V "finer-grained information the
standard does not express" carve-out. Useful for downstream tools that want to suppress path-dep
entries from external-dependency lists (e.g. vendored crates inside a workspace) without losing the
audit trail that they were present in the build.

## Cross-cutting catalog wiring

Each of the four C-rows MUST be:

1. Catalogued in `docs/reference/sbom-format-mapping.md` with the audit narrative above.
2. Registered as a `cdx_anno!`, `spdx23_anno!`, and `spdx3_anno!` entry in
   `mikebom-cli/src/parity/extractors/{cdx,spdx2,spdx3}.rs`.
3. Registered as a `ParityExtractor` slice entry in `mikebom-cli/src/parity/extractors/mod.rs` with
   matching `use` imports.
4. Covered by the existing `extractors_table_is_sorted_by_row_id` + `every_catalog_row_has_an_extractor`
   shape tests (no new test added; the existing tests fail if any new row breaks invariants).
