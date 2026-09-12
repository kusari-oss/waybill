# Licenses

waybill tracks **two separate** license fields per component: what the
package author declared, and what a curated external analyzer concluded.
Both are emitted in the CycloneDX output with distinct `acknowledgement`
values so downstream tools can trust the right source for their use case.

**Key files:**

- `waybill-common/src/resolution.rs` — `ResolvedComponent.licenses` and
  `ResolvedComponent.concluded_licenses`.
- `waybill-common/src/types/license.rs` — `SpdxExpression`, SPDX
  canonicalization via the `spdx` crate.
- `waybill-cli/src/enrich/clearly_defined_source.rs` — concluded-license
  enricher.
- `waybill-cli/src/enrich/depsdev_source.rs` — declared-license enricher.

## The two-bucket model

```rust
pub struct ResolvedComponent {
    /// Licenses asserted by the package author in their manifest
    /// (npm package.json, Cargo.toml, etc.) or by the OS package
    /// metadata (dpkg copyright, rpm header). Maps to CycloneDX
    /// `licenses[]` with `acknowledgement: "declared"`.
    pub licenses: Vec<SpdxExpression>,

    /// Licenses determined through external analysis — currently
    /// ClearlyDefined.io's curated `licensed.declared` field. Maps to
    /// CycloneDX `licenses[]` with `acknowledgement: "concluded"`.
    pub concluded_licenses: Vec<SpdxExpression>,
    ...
}
```

Both serialize into CycloneDX `components[].licenses[]` with the
`acknowledgement` field distinguishing them. They may overlap when both
sources agree — the serializer emits each side once.

### Why two buckets

- **Declared** is what the package author claims. It's cheap, universally
  available when a manifest is present, and sometimes wrong (author typos,
  outdated declarations, free-form text that doesn't canonicalize to SPDX).
- **Concluded** is what an external curator (ClearlyDefined) determined
  through its own analysis pass. It's slower, requires network access, and
  isn't available for every package — but when present it's the
  highest-trust signal.

A consumer doing compliance review cares about concluded first, declared
second. A consumer doing vulnerability matching cares about neither — they
want the CPE. Keeping both lets each consumer pick.

## Source precedence

Licenses can come from three places, at three phases of the pipeline:

1. **Scan-time manifest parsing** (scan stage, `scan_fs/package_db/*.rs`).
   Populates `licenses[]` from:
   - dpkg `/usr/share/doc/<pkg>/copyright` — DEP-5 structured form,
     standalone `License:` stanzas (common-licenses references), modern
     `SPDX-License-Identifier:` tag, and a multi-line recogniser for the
     canonical FSF license-grant prose that packages like
     `debian-archive-keyring`, `libcrypt1`, `libsemanage2`, `libgcc-s1`
     ship verbatim.
   - rpm header `License` field.
   - Cargo.toml `license`.
   - npm `package.json` `license`.
   - gemspec `s.license=` / `s.licenses=`.
   - Maven POM `<licenses>`.
   - PyPI wheel `METADATA` `License:` / `License-Expression:` headers.
2. **deps.dev enrichment** (`enrich/depsdev_source.rs`). Populates
   `licenses[]` — same bucket as scan-time — with deps.dev's reported SPDX
   license. This fills the gap when the local manifest has no license
   field (e.g. PyPI wheels that only carry trove classifiers, gems whose
   gemspec has no license).
3. **ClearlyDefined enrichment** (`enrich/clearly_defined_source.rs`).
   Populates `concluded_licenses[]` from CD's `licensed.declared` field,
   which is itself the output of CD's automated curation.

deps.dev and ClearlyDefined populate **different buckets**. They are not in
tension — deps.dev is a stand-in for the author's declaration when the
local manifest didn't carry one; ClearlyDefined is a separate curated
judgment.

## SPDX canonicalization

Every license expression that lands in either bucket passes through the
`spdx` crate. Free-form strings never reach the CycloneDX output — the
serializer only emits valid SPDX expressions.

Non-canonical inputs are logged at `warn` level and dropped. This includes:

- Free-form license text ("Licensed under the BSD").
- Proprietary license names that don't map to SPDX.
- `NOASSERTION` — explicitly **never** emitted. sbomqs's
  `ValidateLicenseText` rejects `NOASSERTION`, so emitting it would cost
  score without any benefit. When a package truly has no determinable
  license, the component emits no `licenses[]` entry at all.

## CycloneDX shape

Single-identifier licenses emit as:

```json
"licenses": [{ "license": { "id": "MIT", "acknowledgement": "declared" } }]
```

Compound expressions emit as:

```json
"licenses": [{ "expression": "(MIT OR Apache-2.0)", "acknowledgement": "concluded" }]
```

The `acknowledgement` field takes either `"declared"` or `"concluded"`.
sbomqs's `comp_with_valid_licenses` requires a valid SPDX expression in
either shape.

A component that has both declared and concluded licenses for the same
expression emits two entries — the serializer doesn't dedupe across
acknowledgement types because they mean different things.

## Coverage in practice

- **deb / rpm**: declared licenses from DEP-5 / rpm header are the primary
  source; ClearlyDefined doesn't cover deb/apk/rpm well today. See
  [the sbomqs deferred list](licenses.md#deferred-sbomqs-score-lift) for the planned
  ClearlyDefined deb arm (priority next).
- **apk**: apk's installed DB doesn't carry copyright pointers like dpkg
  does, so apk components still ship with empty `licenses[]`.
- **npm / cargo / gem / pypi / maven / golang**: declared licenses come
  from manifests; deps.dev backfills missing ones; ClearlyDefined
  contributes concluded licenses. This is where the
  [sbomqs score lift to 8.8/10 on
  npm](../architecture/overview.md#sbomqs-scoring-baseline-2026-04-20-post-cd-pass)
  came from.

## Known limitations

- **License expression canonicalization is best-effort.** The `spdx` crate
  is strict; some legitimate expressions (compound with operators in
  non-standard order, e.g.) may be dropped where a more permissive parser
  would accept.
- **Deprecated-license flagging** (sbomqs `comp_no_deprecated_licenses`)
  and restrictive-license flagging are not yet emitted. The `spdx` crate
  has the data via `is_deprecated()` and OSI/copyleft classifications —
  threading that through `SpdxExpression` into CycloneDX properties is a
  deferred backlog item.
- **Supplier extraction** (sbomqs `comp_with_supplier`) isn't done yet.
  Lockfiles don't carry author info; adding `node_modules/` / `.m2`
  walks for supplier would unlock another ~2% of the sbomqs score. See
  [the sbomqs deferred list](licenses.md#deferred-sbomqs-score-lift).

### Deferred: sbomqs score lift

Tracked separately because each item has its own design depth. Current source-scan baseline is 7.0–8.8/10 depending on fixture (post-CD enrichment, 2026-04-20).

13. **CDX `comp_no_deprecated_licenses` + `comp_no_restrictive_licenses`** — sbomqs reads these off `concluded_licenses[]`. The `spdx` crate exposes `is_deprecated()` and OSI/copyleft classifications; need to thread that through `SpdxExpression` (e.g. `as_spdx_id_info() -> Option<{id, deprecated, restrictive}>`) so the CDX serializer can emit `properties` flagging each. ~6.4% in Licensing for npm/cargo fixtures.
14. **Component supplier extraction** — npm `package.json::author.name`, cargo `Cargo.toml::package.authors[0]`, maven `pom.xml::organization`. Lockfile scans currently miss these because lockfiles don't carry author info; adding a node_modules / .m2 walk for the supplier field would unlock `comp_with_supplier` (2.2%). Heuristic for npm scoped packages: treat `@scope` as supplier when `author` absent.
15. **Component VCS URL externalReferences** — emit `externalReferences[{type: "vcs", url: ...}]` from each ecosystem's manifest (cargo `repository`, npm `repository.url`, maven `<scm>`). Unlocks `comp_with_source_code` (2.2%). Most ecosystems have this in the manifest so it's mostly extraction work.
16. **SBOM signature** (`sbom_signature` 1.8%) — sign the emitted CDX BOM in-place (CycloneDX defines a `signature` block). Needs key management story (CLI flag for key path? KMS?). Separate from this effort.
17. ~~**Per-ecosystem manifest hashes** — gem/maven/pypi/go currently emit no per-component hashes.~~ **PARTIALLY DONE 2026-04-20**: maven sidecar (`.jar.sha512` > `.sha256` > `.sha1`) wired into `MavenRepoCache::read_artifact_hash` for both BFS-discovered transitives and direct deps. PyPI `requirements.txt --hash=alg:hex` flags wired through to `PackageDbEntry.hashes`. Remaining: (a) Maven-direct SHA-256 computation when `~/.m2` has the JAR but no SHA-256 sidecar (Maven Central mostly has SHA-1 only — sbomqs penalizes for `comp_with_strong_checksums`); (b) gem CHECKSUMS in bundler 2.5+ when adoption stabilizes; (c) Go: `go.sum` H1 hashes are Merkle trie roots (NOT file SHA-256), would need a custom CDX hash type or to hash the cached `<v>.zip` from `$GOMODCACHE/cache/download/`.
18. **ClearlyDefined ecosystem expansion — deb (priority)** — current scope is npm/cargo/gem/pypi/maven/golang. The deb arm is the highest-value addition: when a container scan strips `/usr/share/doc/<pkg>/copyright` (common minimization practice), waybill emits zero licenses even when `dpkg/status` is intact. CD's `deb` type pulls license data from Debian's upstream copyright-file server and would fill that gap. Shape: add a `"deb"` arm to `enrich/clearly_defined_coord.rs::build_cd_coord` (type=`deb`, provider=`debian`, namespace=`-`, name=`<pkg>`, revision=`<version>`). Works for both debian and ubuntu since ubuntu packages reuse Debian coords in CD. Other CD types (`composer`, `pod`, `conda`, `nuget`) are separate follow-ups; apk / rpm coverage in CD is thin and not worth the mapping work yet.
21. **Debian sources.debian.org copyright API (fallback)** — alternative to #18 for deb when CD returns a miss (CD doesn't curate every debian-unstable or backport version). `https://sources.debian.org/copyright/api/package/<name>/<version>/` returns structured copyright data parsed from upstream `debian/copyright`. More work than CD integration (new HTTP client, no existing pattern to copy) but covers versions CD misses. Only worth doing after #18 ships and we measure the actual miss rate on real fixtures; CD probably covers >90% of Debian stable / Ubuntu LTS packages that production scans encounter.
19. **ClearlyDefined bounded concurrency** — current implementation is sequential per-component (matches `deps.dev`). For scans of 100+ components this can be 10–30 seconds. Concrete optimization: `tokio::task::JoinSet` with 8 in-flight + reqwest connection pool reuse. Deferred until profiling shows it dominates scan time.
20. **ClearlyDefined harvest endpoint** — CD has `/notices`, `/curations`, search APIs that could enrich provenance further (license texts, attributions, copyright statements). Out of scope for this milestone but unlock more sbomqs categories if added.

---

*Moved here from `docs/architecture/overview.md` when that document was retired (#827). It lives with the doc that referenced it, so the indirection is gone rather than relocated.*

> **Staleness warning.** This list was written 2026-04-20 and at least two entries have
> since shipped: `sbom_signature` (milestone 777) and `comp_with_source_code` (milestone
> 776). Treat every entry as needing confirmation before acting on it. It is reproduced
> here so the references that pointed at it still resolve, not because it is current.
