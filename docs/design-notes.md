# mikebom design notes

Running reference for architectural decisions, tradeoffs, and known
limitations across ecosystems. Intended as a pickup point for future
sessions — skim the ecosystem-status table first, drill into the
sections that matter for the task at hand.

---

## Ecosystem coverage

The authoritative per-ecosystem coverage matrix — OS-db vs. lockfile vs.
manifest, dep-graph completeness, and known limitations — lives in
[`docs/ecosystems.md`](./ecosystems.md). This document captures the
cross-cutting architectural decisions that span ecosystems; go there
first if you're asking "can mikebom read X" or "what does it do with Y".

---

## Dep-graph resolution strategy (Maven)

Maven is the most complex — transitive versions can live in parent
POMs' `<dependencyManagement>` or be supplied by BOM imports. The
scanner layers sources in this order:

1. **Scanned project `pom.xml`** — direct deps, declared versions.
2. **JAR-embedded `META-INF/maven/<g>/<a>/pom.xml`** — identity from
   pom.properties; edges from the embedded pom.xml. Works for
   deployed containers. Fat/shaded JARs yield one
   `EmbeddedMavenMeta` per vendored artifact.
3. **`~/.m2/repository/` cache walker** (BFS) — for each observed
   coord, fetch its cached `.pom`, extract `<dependencies>`, recurse.
4. **Parent-POM chain** (`build_effective_pom` in `maven.rs`) —
   merges `<properties>` and `<dependencyManagement>` up the
   `<parent>` chain. Required for guava (parent POM declares
   `jsr305`, `checker-qual`, etc. versions) and jackson-databind
   (`${jackson.version.core}` resolved in parent). BOM imports
   (`<type>pom</type><scope>import</scope>`) flattened into the
   effective `dependencyManagement`. Memoized, cycle-guarded.
5. **deps.dev `:dependencies` endpoint** (`deps_dev_graph.rs`) —
   online fallback. Fills shaded-transitive gaps + cold-cache gaps.
   Tagged `source_type = "declared-not-cached"` distinct from
   locally-observed coords.
6. **Empty edges** — final graceful degradation.

### deps.dev policy (critical)

- deps.dev is authoritative for **edge topology** ("A depends on B").
- deps.dev is **not** authoritative for **versions**. Local `.m2`
  wins. When deps.dev reports `foo@1.0` and local has `foo@1.5`, the
  emitted edge target is `foo@1.5` — what's actually on disk.
- When a deps.dev-reported coord has no local version at all, it's
  emitted as a new component tagged `source_type = "declared-not-cached"`.
- Offline mode (`--offline`) skips the entire deps.dev pass.
- Concurrency capped at 8 in-flight requests (`tokio::task::JoinSet`).

### Why deps.dev is only wired for Maven

Other ecosystems have local signals that are complete:
- **Cargo**: `Cargo.lock` encodes the full tree.
- **Go source**: `go.sum` + module cache reconstructs the tree.
- **Ruby**: `Gemfile.lock` indent-6 lines encode transitives.
- **npm**: lockfiles encode tree.

deps.dev could be wired for these later (e.g. for Go binaries where
BuildInfo doesn't encode edges), but isn't today.

---

## Source-type markers glossary

The `mikebom:source-type` property on each CycloneDX component
distinguishes how a coord was discovered:

| Value | Meaning |
|---|---|
| `workspace` | Declared in the scanned project's own manifest (pom.xml, Cargo.toml, etc.). Highest trust — the user directly wrote this dep. |
| `transitive` | BFS-discovered via local cache / JAR walk. Strong trust — the coord is on-disk locally, its own manifest says it declares these deps. |
| `declared-not-cached` | deps.dev says this coord is part of the declared tree, but it's not present locally at any version. Lower trust — may not actually be installed. |
| `analyzed` | JAR walker emitted this from `META-INF/maven/.../pom.properties`. Strong trust — the JAR is on disk. |
| `git`, `path`, `workspace`, `local` | Cargo/Gem source-kind markers for non-registry packages. |

`mikebom:sbom-tier` is a separate axis (`source` / `analyzed` / `deployed` / `design` / `build`) — see `mikebom-common/src/resolution.rs` for the full ladder.

---

## Known limitations / sharp edges

### Maven
- **`<exclusions>`** not parsed. If a project excludes a transitive via `<exclusions>` in its pom, mikebom still emits the excluded coord as a dep.
- **Version ranges** (`[1.0,2.0)`) not resolved. Maven picks a specific version at build time; mikebom treats the range string as-is.
- **`<profiles>`** ignored. Profile-conditional deps never emit.
- **Plugin-section deps** (`<build><plugins>`) ignored — not runtime deps.
- **POM-less JARs** (older Gradle outputs, OSGi bundles) can't be inspected via `META-INF/maven/` — coord + deps invisible.
- **Same artifactId across groups** — `scan_fs/mod.rs::normalize_dep_name` keys edges on `(ecosystem, name)` only, so two unrelated artifacts both named `commons` in different groups would conflate. Pre-existing; not made worse by any recent work.
- **Compositions-level transparency** — currently the `maven` composition is marked `complete` whenever any source-tier Maven coord is seen. Should probably downgrade to `incomplete_first_party_only` when any BFS cache-miss or deps.dev failure occurred during the scan. Deferred.

### Go
- **Binary scans have no edges.** `runtime/debug.BuildInfo` encodes module list but not module→module relationships. Source-tree scans get the graph via the milestone 055 transitive-edges resolver (4-step ladder).
- **Scratch / distroless images with a single Go binary** produce a flat component list. That's the accurate answer — the binary doesn't know the graph.
- **Private module proxies / `vendor/` directory component extraction** out of scope.

#### Go transitive-edge resolver (milestone 055)

Source-tree scans use a 4-step ladder to obtain each `(module, version)` pair's `go.mod` (and thus the module's outgoing `dependsOn` edges). The ladder is invoked once per Go project root per scan, in `scan_fs::package_db::golang::graph_resolver::GraphResolver::resolve`.

| Step | Source | Available when |
|------|--------|----------------|
| 1 | `go mod graph` subprocess | `go` is on PATH AND `--offline` not set |
| 2 | `$GOMODCACHE` walk (`cache/download/<escaped-mod>/@v/<ver>.mod`) | mikebom finds at least one cache root via `$GOMODCACHE` / `$GOPATH/pkg/mod` / `$HOME/go/pkg/mod` / rootfs scan |
| 3 | HTTP fetch from `$GOPROXY` (default `proxy.golang.org`) | `--offline` not set AND `$GOPROXY != off` AND module not matched by `$GOPRIVATE` |
| 4 | Empty fallthrough | always — emits the component with empty `depends`, increments `LadderSummary::missing_count` |

Edges are intersected with `go.sum` AFTER all steps complete. The intersection is path-keyed (not exact `(path, version)` keyed) — a `go.mod` may declare `require X v1.0.0` but `go.sum` has `X v2.0.0` because workspace MVS bumped it; the resolver rewrites the edge target to the installed version. This matches `go mod graph`'s output and avoids dropping edges purely because of declared-vs-installed version drift.

Per-scan operational visibility: `tracing::info!` summary line at scan end lists the per-step counts: `go transitive edges: ladder=[graph:N1, cache:N2, proxy:N3, missing:N4]`. Useful in CI logs to see at a glance which step contributed which edges.

Sync, not async: the resolver is invoked from a sync chain (`scan_path` → `read_all` → `golang::read`). `reqwest::blocking::Client` (workspace `blocking` feature) is constructed and dropped on a dedicated `std::thread::spawn`'d worker so it never crosses an async runtime boundary. Concurrency for proxy fetches uses a 16-way `std::thread` worker pool with bounded `mpsc` queue (`parallel_fetch` in `graph_resolver.rs`).

`--offline` plumbing is currently bridged via the `MIKEBOM_OFFLINE` env var (set in `main.rs` based on `cli.offline`). Future cleanup: thread an `offline: bool` parameter through `scan_path` → `read_all` → `golang::read` to remove the env-var bridge.

Out of scope for milestone 055: `go.work` workspaces (multi-module), `vendor/` directory resolution, deps.dev as a data source, source-VCS (`GOPROXY=direct`) fallback, per-fetched-`.mod` SHA-256 verification against `go.sum`'s h1 hashes.

See `specs/055-go-transitive-edges/` for the full design + capability matrix.

### RPM
- **Berkeley DB rpmdb** (`/var/lib/rpm/Packages` pre-RHEL 8) is detected but not parsed. Diagnostic logged, zero rpm components emitted.
- **rpmdb.sqlite size cap** is 200 MB — defense-in-depth. Real rpmdbs are ~5 MB.
- **Pure-Rust SQLite reader** only handles leaf-table + interior-table pages; overflow pages refused. RHEL rpmdbs don't use overflow pages in practice.

### Ruby
- Only `--include-dev` gating is on gems under `test` scope in the declaration tree; bundler's full scope semantics not modeled.
- **Gemspec walker** (added 2026-04-20 for sbom-conformance bug 3) parses name + version from `specifications/*.gemspec` files via a line-scanner for `s.name = "..."` / `s.version = "..."`. Interpolated versions (`"#{FOO_VERSION}"`) produce garbage strings — downstream PURL construction will typically reject them. In practice, gemspec versions are always literal strings so this is a theoretical edge case.

### Binary scanner
- **Version-string scanner is gated on `skip_file_level_and_linkage`** (added 2026-04-20 for conformance bug 6a). Claimed binaries no longer emit `pkg:generic/<library>@<version>` from the curated scanner. Trade-off: static-library version detection inside claimed binaries (e.g. statically-linked OpenSSL in a dpkg-owned binary) is lost. Accepted because the FP flood from self-identifying claimed binaries (curl reporting libcurl from /usr/bin/curl) was the larger correctness problem.
- **Linkage aggregator probes standard library dirs** (added 2026-04-20 for conformance bug 6b) via `add_with_claim_check`. Sonames resolving to a claimed library path (e.g. libc.so.6 → /lib/x86_64-linux-gnu/libc.so.6 owned by libc6 deb) are skipped.
- **ELF-note-package emission is claim-gated + OS-context-aware** (added 2026-04-20 for conformance bug 1). Previously unconditional — a claimed Fedora binary would emit both `pkg:rpm/fedora/<subpackage>@<ver>` (from rpmdb) AND a ghost `pkg:rpm/rpm/<source-package>@<ver>` (from the ELF `.note.package` section). Now the ELF-note emission is gated on `skip_file_level_and_linkage` (drops ghosts for claimed binaries). For unclaimed binaries, the signature of `note_package_to_entry` takes the scan's `/etc/os-release` `ID` and `VERSION_ID` — precedence is `note.distro` > os-release ID > hardcoded type default (rpm/debian/alpine). When VERSION_ID is known, a `distro=<vendor>-<version>` qualifier is appended. Trade-off: for claimed binaries we lose the ELF note's source-package identity; recovery is via rpm's `SOURCERPM` header if needed.
- **Curated version-string scanner is a 7-library list** (OpenSSL/BoringSSL/zlib/SQLite/curl/PCRE/PCRE2). Binaries installed outside the package manager without matching patterns emit file-level only (hash-only PURL). Extending the list is case-by-case; see backlog item #12.

### OS-release reader
- **Rootfs-aware fallback** (added 2026-04-20 for conformance bug 1): tries `<rootfs>/etc/os-release` first, falls back to `<rootfs>/usr/lib/os-release`. Fixes Ubuntu images where /etc/os-release is a relative symlink that can dangle after container-layer tar extraction.

### PURL canonicalization
- **Qualifiers sorted alphabetically** (added 2026-04-20): `Purl::new` re-canonicalizes the qualifier section so `?epoch=1&arch=x86_64&distro=fedora-40` becomes `?arch=x86_64&distro=fedora-40&epoch=1`. Required by purl-spec `docs/how-to-build.md` ("Sort this list of qualifier strings lexicographically"). Affects every ecosystem uniformly. Already-sorted inputs pass through unchanged (preserves caller-side `encode_purl_segment` work).
- **RPM `epoch=0` omitted** (added 2026-04-20): treats `Some(0)` as semantically "no epoch" and drops the qualifier. RPM treats absent and 0 as equivalent for version comparison; `rpm -qa` default display omits; purl-spec rpm example never shows `epoch=0`. Reverses the milestone-005 round-trip-`rpm -qa` decision (see `specs/005-purl-and-scope-alignment/research.md` for the trade-off).

### CycloneDX 1.6 serialization
- **`evidence.identity` is an array** (added 2026-04-20 for sbomqs parse failure): the single-object form was deprecated in CDX 1.5→1.6. Every component emits `identity: [{...}]` with exactly one identity object.
- **`evidence.identity[].tools` is never emitted**: per CDX 1.6 that field must contain bom-refs to items declared in the BOM (metadata/tools/services/formulation). mikebom's previous payload (TLS connection IDs + deps.dev markers) are not tools and don't exist elsewhere in the BOM. Both now land on the component as properties `mikebom:source-connection-ids` (comma-joined) and `mikebom:deps-dev-match` (`<system>:<name>@<version>`). The `pkg:generic/...` provenance semantics are preserved, just in the CDX-conformant location.
- **License shape**: components emit `{"license": {"id": "<SPDX-id>"}}` for single-identifier licenses (via `SpdxExpression::as_spdx_id`) and `{"expression": "<expr>"}` for compound expressions. Required for sbomqs's `comp_with_valid_licenses` check.
- **Component hashes from manifests**: npm's `package-lock.json::integrity` (sha256/sha384/sha512) and Cargo.lock's `checksum` (sha256) flow through `PackageDbEntry.hashes` → `ResolvedComponent.hashes` → `components[].hashes[]`. Other ecosystems (gem/maven/pypi/go) defer for now — see TODO.
- **`metadata.component` carries synthetic `purl` + `cpe`**: scan subjects emit `pkg:generic/<name>@<version>` and `cpe:2.3:a:mikebom:<name>:<version>:*:*:*:*:*:*:*`. Required for sbomqs schema validity (the validator rejects empty cpe/purl on metadata.component even though the spec doesn't require them).
- **`metadata.authors`, `metadata.supplier`, `metadata.licenses` (CC0-1.0)**: hardcoded SBOM-producer identity + dataLicense.
- **Trace-integrity counters on `metadata.properties`**: `mikebom:trace-integrity-{ring-buffer-overflows,events-dropped,uprobe-attach-failures,kprobe-attach-failures}` instead of attached to a composition (CDX 1.6 compositions schema sets `additionalProperties: false`).
- **Compositions emit both `assemblies` and `dependencies`** for each `complete` ecosystem record. Plus a separate dep-completeness composition listing the primary's bom-ref under `dependencies` when no integrity issues — needed for sbomqs's `comp_with_dependencies` to credit the primary.
- **Primary-dependency fallback in `build_dependencies`**: when the scanned project's root entry was filtered out (npm path_key=="", cargo source=None) and no explicit edges connect the metadata.component to anything, synthesize edges from the primary to every "root" component (those nothing else depends on). Without this, sbomqs reports "no dependency graph present" even when transitive edges are populated.

### sbomqs scoring baseline (2026-04-20, post-CD pass)
After the ClearlyDefined enrichment integration, source-scan SBOMs reach **8.8/10 (Grade B)** on npm fixtures, **7.0–7.8 (C)** on cargo / gem / polyglot, **6.1 (D)** on RPM image scans (Integrity 0/10 still — rpmdb has no per-package hashes mikebom can use). Remaining deferred work (separate milestone):
- `comp_with_strong_checksums` for gem/maven/pypi/go/rpm (need ecosystem-specific hash sources)
- `comp_no_deprecated_licenses` / `comp_no_restrictive_licenses` (the spdx crate has the data; needs threading through)
- `comp_with_supplier` (needs walking node_modules / .m2 cache for author info; lockfiles alone don't carry it)
- `comp_with_source_code` (needs VCS URL extraction per ecosystem)
- `sbom_signature` (needs key/signing infra)
- `sbom_completeness_declared` for gem (currently lockfile gem composition isn't tagged complete)

### ClearlyDefined enrichment (added 2026-04-20)
- Post-scan enricher mirroring the `deps.dev` pattern. Lives at `mikebom-cli/src/enrich/clearly_defined_{client,coord,source}.rs`.
- Queries `https://api.clearlydefined.io/definitions/{type}/{provider}/{ns}/{name}/{rev}` per supported component (npm, cargo, gem, pypi, maven, golang). CD's `licensed.declared` becomes mikebom's `acknowledgement: "concluded"` license entry.
- Honors the existing `--offline` flag (no HTTP when set).
- In-memory cache (per-scan, not persistent). 5s timeout per request.
- Sequential per-component (matches deps.dev). Bounded concurrency deferred until profiling shows it matters.
- Unsupported ecosystems (deb / apk / rpm / generic / alpm) skipped silently.
- When CD has no entry for a package, no concluded entry emitted (declared remains). `NOASSERTION` is intentionally never emitted — sbomqs's `ValidateLicenseText` rejects it, so it would add cost without unlocking score.

### General
- **Same-artifact-different-group edge conflation** (see Maven note).
- **`#[deny(clippy::unwrap_used)]` at crate root** — production code cannot use `.unwrap()`. Test modules opt back in via `#[cfg_attr(test, allow(clippy::unwrap_used))]`.

---

## Testing layout

| Fixture type | Where | Shape |
|---|---|---|
| Unit tests | Inline in each `mikebom-cli/src/scan_fs/package_db/*.rs` | Synthetic via `tempfile::tempdir()`; helpers like `write_cached_pom`, `write_jar` |
| Integration tests | `mikebom-cli/tests/scan_<ecosystem>.rs` | Shell out to the compiled binary via `CARGO_BIN_EXE_mikebom`; parse resulting JSON SBOM |
| Real fixtures | `tests/fixtures/<ecosystem>/` | Real go.mod/go.sum + real Go binaries, real Gemfile.lock, hand-crafted rpmdb.sqlite (via Python sqlite3), synthetic JARs |
| Cache-warm tests | Synthetic `<rootfs>/root/.m2/repository/...` inside tempdirs | Avoids dependency on user's host `~/.m2` |
| Online tests | Unit tests involving deps.dev are unit-tested only for name-formatting / URL construction; no HTTP roundtrips in CI | Integration tests that would need network are gated behind env-present checks |

Full-suite regression: `cargo test --workspace` — 871 passing, 0 failed as of per-ecosystem-hashes pass (2026-04-20). Baseline was 585 at milestone 003.

---

## Key code landmarks

### Maven (most complex)
- `mikebom-cli/src/scan_fs/package_db/maven.rs`
  - `parse_pom_xml` — XML traversal; captures self/parent coords, properties, dependencies, dependencyManagement
  - `EffectivePom`, `build_effective_pom` — parent-chain walker with memo + cycle guard
  - `resolve_dep_version`, `resolve_dep_group` — use effective POM for placeholder resolution
  - `bfs_transitive_poms` — BFS over M2 cache driven from direct-dep seeds
  - `walk_jar_maven_meta` — JAR-embedded pom walker
  - `MavenRepoCache::discover` — probes `$HOME/.m2`, `<rootfs>/root/.m2`, etc.

### deps.dev enrichment
- `mikebom-cli/src/enrich/deps_dev_client.rs` — HTTP client; `get_dependency_graph` hits `:dependencies` endpoint
- `mikebom-cli/src/enrich/deps_dev_system.rs` — PURL-ecosystem→system mapping + Maven-aware `deps_dev_package_name`
- `mikebom-cli/src/enrich/deps_dev_graph.rs` — post-scan enricher; substitutes local versions, tags declared-not-cached
- `mikebom-cli/src/enrich/depsdev_source.rs` — existing license enricher (now using the Maven-aware name format)

### Go
- `mikebom-cli/src/scan_fs/package_db/golang.rs`
  - `GoModCache::discover` — cache-root discovery for source scans
  - `build_entries_from_go_module` + `cache_lookup_depends` — walks `<cache>/@v/*.mod` files
  - `escape_module_path` — capital letters → `!x` for cache path lookup
- `mikebom-cli/src/scan_fs/package_db/go_binary.rs`
  - `decode_buildinfo` — reads inline-format BuildInfo from Go 1.18+ binaries
  - `detect_is_go` — section lookup via `object` crate, fallback memmem for stripped binaries

### Cache / SQLite
- `mikebom-cli/src/scan_fs/package_db/rpmdb_sqlite/` — pure-Rust SQLite subset reader

### Orchestration
- `mikebom-cli/src/cli/scan_cmd.rs` — wires scan_fs → enrichment → SBOM serialization
- `mikebom-cli/src/scan_fs/mod.rs` — `scan_path` entry, relationship resolution + dangling-target filter

---

## Deferred backlog

Ordered rough priority (highest-value first):

1. **Maven `<exclusions>`** — needed for correctness when projects deliberately exclude transitives.
2. **Maven version ranges** — `[1.0,2.0)` resolution; low-priority since published artifacts rarely use ranges.
3. **Parent-POM inheritance for ancestral `<parent>` chains** — basic case works; deeply nested parents (e.g. Spring's hierarchy) may still produce unresolved placeholders if a grandparent's properties aren't found. Verify in practice.
4. **Compositions degradation** — downgrade ecosystem composition from `complete` to `incomplete_first_party_only` when cache-miss or deps.dev-miss occurred. Requires threading a miss counter through scan_fs.
5. **JAR-embedded pom.xml for Maven transitive edges in container scans** — when a shaded JAR is the only artifact and deps.dev is offline, we currently emit the top-level coord with empty edges. Could fall back to reading the shade plugin's dependency-reduced-pom.xml if present.
6. **Go: deps.dev fallback for binary scans** — `runtime/debug.BuildInfo` emits coords but no edges. deps.dev could fill in the graph. Trade-off: network dependency for a scan mode that today is fully offline.
7. **npm scoped names** — deps.dev formatter now handles `@scope/name`; dep-graph enricher only wired for Maven. Could extend if npm lockfile scans ever need supplementation.
8. **POM-less JARs** (OSGi bundles, older Gradle artifacts) — would need OSGi manifest (`Import-Package`, `Require-Bundle`) parsing.
9. **Same-artifactId-different-groupId edge conflation** — pre-existing. Fix would require keying edges on `(ecosystem, namespace, name)` not just `(ecosystem, name)`.
10. **Multiple cached versions of the same `(g, a)` in `~/.m2`** — the JAR walker's `coord_index` currently keeps the first-observed version. Good enough for most cases; a project-specific resolution would require reading the project's pom + running Maven's "nearest wins" algorithm.
11. **Go source-tree scope** — investigate switching from go.sum-driven to `go.mod Require`-driven component enumeration for Go 1.17+ sources. Would align with trivy's default behavior (syft default uses `packages.Load` which is even more inclusive). Full context in `docs/research/go-binary-scope.md`.
12. **Binary-scanner jq detection** — `version_strings.rs` has a curated 11-library scanner as of milestone 026 (OpenSSL/BoringSSL/zlib/SQLite/curl/PCRE/PCRE2/GnuTLS/LibreSSL/LLVM/OpenJDK). Unmanaged binaries like a curl'd `/usr/local/bin/jq` emit only as `pkg:generic/jq?file-sha256=...` (hash, no version). Options: (a) add jq-specific pattern to the curated list — doesn't scale; (b) generic version-string heuristic (`<name>-<ver>` / `<name> version <ver>`) — high FP surface; (c) investigate trivy's `binaries` analyzer and port the subset that has low FP risk.

### Deferred: curated version-string scanner — hard cohort (milestone 026.x)

Three libraries deferred from milestone 026 (which shipped the easy-4 cohort: GnuTLS, LibreSSL, LLVM, OpenJDK) because they don't have clean self-identifying strings in the read-only string region (`.rodata` / `__TEXT,__cstring` / `.rdata`). Each needs a different detection mechanism than the curated string scanner:

- **glibc** — version markers (`GLIBC_X.Y`) live in the ELF `.gnu.version_r` section (symbol-version table), NOT in `.rodata`. Detection requires walking `.gnu.version_r` entries via `object::read::elf::*` primitives (similar to how milestone 023's `extract_runpath_entries` walks `.dynamic`) and aggregating the maximum GLIBC version across all symbol references. Different mechanism than the string scanner — would emit `mikebom:glibc-required-version` (or similar) on the file-level component, not a separate `pkg:generic/glibc@X.Y` component. ELF-only signal.

- **musl** — typically doesn't self-identify in compiled output. Some bundled-musl binaries embed a `musl libc (x86_64)` banner via `__libc_get_version()` calls, but that path is rarely exercised in static binaries. Research milestone needed to find a reliable signature (control-set: try Alpine Linux's `/usr/bin/busybox`, `/usr/bin/curl`, etc.) or conclude there isn't one and document the gap.

- **V8** — version strings live in stack-trace formatting code (e.g. `v8::internal::Version::GetString()` callers) and tend to be non-deterministic across builds + dependent on V8 build flags. Research milestone needed to find a reliable string-region signature; may end up needing an inline-data scan of V8's snapshot blob rather than a string scan. Control-set: try a Node.js binary, a Chromium content-shell, a `deno` binary — all embed V8 in different ways.

Discoverable via `grep TODO\(milestone-026.x\) mikebom-cli/src/scan_fs/binary/version_strings.rs`.

### Deferred: sbomqs score lift

Tracked separately because each item has its own design depth. Current source-scan baseline is 7.0–8.8/10 depending on fixture (post-CD enrichment, 2026-04-20).

13. **CDX `comp_no_deprecated_licenses` + `comp_no_restrictive_licenses`** — sbomqs reads these off `concluded_licenses[]`. The `spdx` crate exposes `is_deprecated()` and OSI/copyleft classifications; need to thread that through `SpdxExpression` (e.g. `as_spdx_id_info() -> Option<{id, deprecated, restrictive}>`) so the CDX serializer can emit `properties` flagging each. ~6.4% in Licensing for npm/cargo fixtures.
14. **Component supplier extraction** — npm `package.json::author.name`, cargo `Cargo.toml::package.authors[0]`, maven `pom.xml::organization`. Lockfile scans currently miss these because lockfiles don't carry author info; adding a node_modules / .m2 walk for the supplier field would unlock `comp_with_supplier` (2.2%). Heuristic for npm scoped packages: treat `@scope` as supplier when `author` absent.
15. **Component VCS URL externalReferences** — emit `externalReferences[{type: "vcs", url: ...}]` from each ecosystem's manifest (cargo `repository`, npm `repository.url`, maven `<scm>`). Unlocks `comp_with_source_code` (2.2%). Most ecosystems have this in the manifest so it's mostly extraction work.
16. **SBOM signature** (`sbom_signature` 1.8%) — sign the emitted CDX BOM in-place (CycloneDX defines a `signature` block). Needs key management story (CLI flag for key path? KMS?). Separate from this effort.
17. ~~**Per-ecosystem manifest hashes** — gem/maven/pypi/go currently emit no per-component hashes.~~ **PARTIALLY DONE 2026-04-20**: maven sidecar (`.jar.sha512` > `.sha256` > `.sha1`) wired into `MavenRepoCache::read_artifact_hash` for both BFS-discovered transitives and direct deps. PyPI `requirements.txt --hash=alg:hex` flags wired through to `PackageDbEntry.hashes`. Remaining: (a) Maven-direct SHA-256 computation when `~/.m2` has the JAR but no SHA-256 sidecar (Maven Central mostly has SHA-1 only — sbomqs penalizes for `comp_with_strong_checksums`); (b) gem CHECKSUMS in bundler 2.5+ when adoption stabilizes; (c) Go: `go.sum` H1 hashes are Merkle trie roots (NOT file SHA-256), would need a custom CDX hash type or to hash the cached `<v>.zip` from `$GOMODCACHE/cache/download/`.
18. **ClearlyDefined ecosystem expansion — deb (priority)** — current scope is npm/cargo/gem/pypi/maven/golang. The deb arm is the highest-value addition: when a container scan strips `/usr/share/doc/<pkg>/copyright` (common minimization practice), mikebom emits zero licenses even when `dpkg/status` is intact. CD's `deb` type pulls license data from Debian's upstream copyright-file server and would fill that gap. Shape: add a `"deb"` arm to `enrich/clearly_defined_coord.rs::build_cd_coord` (type=`deb`, provider=`debian`, namespace=`-`, name=`<pkg>`, revision=`<version>`). Works for both debian and ubuntu since ubuntu packages reuse Debian coords in CD. Other CD types (`composer`, `pod`, `conda`, `nuget`) are separate follow-ups; apk / rpm coverage in CD is thin and not worth the mapping work yet.
21. **Debian sources.debian.org copyright API (fallback)** — alternative to #18 for deb when CD returns a miss (CD doesn't curate every debian-unstable or backport version). `https://sources.debian.org/copyright/api/package/<name>/<version>/` returns structured copyright data parsed from upstream `debian/copyright`. More work than CD integration (new HTTP client, no existing pattern to copy) but covers versions CD misses. Only worth doing after #18 ships and we measure the actual miss rate on real fixtures; CD probably covers >90% of Debian stable / Ubuntu LTS packages that production scans encounter.
19. **ClearlyDefined bounded concurrency** — current implementation is sequential per-component (matches `deps.dev`). For scans of 100+ components this can be 10–30 seconds. Concrete optimization: `tokio::task::JoinSet` with 8 in-flight + reqwest connection pool reuse. Deferred until profiling shows it dominates scan time.
20. **ClearlyDefined harvest endpoint** — CD has `/notices`, `/curations`, search APIs that could enrich provenance further (license texts, attributions, copyright statements). Out of scope for this milestone but unlock more sbomqs categories if added.

---

## Output formats (milestone 010)

SPDX 2.3 is a peer of CycloneDX 1.6 — not a second-class alternative:
the two formats share a single scan pass, produce byte-identical content
from identical input (modulo each spec's mandatory-volatile fields), and
every piece of CDX-emitted data has a documented target in SPDX 2.3
(native field or `annotations[]` envelope), cross-referenced via
`docs/reference/sbom-format-mapping.md`. A third format —
`spdx-3-json-experimental` — opts into a minimal SPDX 3.0.1 JSON-LD
stub for npm components; it exists so adding full SPDX 3 emission in a
future milestone is incremental rather than a rewrite. CVE/advisory data
that the scanner discovers (currently no scanner populates
`AdvisoryRef`, but the plumbing is live) rides alongside SPDX 2.3 as an
OpenVEX 0.2.0 sidecar, referenced from the SPDX document via
`externalDocumentRefs` with a SHA-256 of the sidecar bytes. The CLI
surface: `--format` accepts a comma-separated list, `--output` accepts
either bare `<path>` (legacy single-format) or repeated `<fmt>=<path>`
(per-format); `openvex` is a reserved pseudo-format key for the
sidecar's path override (cannot be requested via `--format`). The
canonical cross-format contract is
`docs/reference/sbom-format-mapping.md`; CI guards it against drift
(`sbom_format_mapping_coverage.rs`) so any new `mikebom:*` property
added anywhere in the scan pipeline breaks the build until it's
mapped.

## Per-component generic annotation bag (milestone 023)

`PackageDbEntry` and `ResolvedComponent` carry an
`extra_annotations: BTreeMap<String, serde_json::Value>` field. Each
entry is emitted at SBOM-generation time as a `mikebom:<key>`
annotation across all three formats — a CycloneDX `properties[]`
entry, a SPDX 2.3 `annotations[]` envelope, and a SPDX 3
graph-element `Annotation`. The emission code in
`generate/cyclonedx/builder.rs`, `generate/spdx/annotations.rs`, and
`generate/spdx/v3_annotations.rs` iterates the bag deterministically
(`BTreeMap`'s sorted-by-key order is what byte-identity goldens
depend on).

**Why a bag and not typed fields:** `PackageDbEntry` has 30+
struct-literal init sites and no `Default` impl (`Purl` and
`SpdxExpression` don't have meaningful defaults), so each new typed
field forces 30+ manual `field: None,` additions per milestone. The
bag amortizes that cost across all future per-binary-metadata
milestones — a new key is one `entry.extra_annotations.insert(...)`
call.

**Consumers shipped (6 — pattern fully validated):**
- Milestone 023 — ELF identity (`mikebom:elf-build-id`,
  `mikebom:elf-runpath`, `mikebom:elf-debuglink`) populated in
  `binary/entry.rs::make_file_level_component` →
  `build_elf_identity_annotations`.
- Milestone 024 — Mach-O identity (`mikebom:macho-uuid`,
  `mikebom:macho-rpath`, `mikebom:macho-min-os`) populated in
  `binary/entry.rs::build_macho_identity_annotations`. Reads LC_UUID
  / LC_RPATH / LC_BUILD_VERSION (or LC_VERSION_MIN_*) via byte-level
  load-command parsing in `binary/macho.rs`. Fat / universal
  binaries report from the first slice's bytes.
- Milestone 025 — Go BuildInfo VCS metadata
  (`mikebom:go-vcs-revision`, `mikebom:go-vcs-time`,
  `mikebom:go-vcs-modified`) populated in
  `package_db/go_binary.rs::build_vcs_annotations` on the main-module
  entry only (deps don't carry VCS info).
- Milestone 028 — PE identity (`mikebom:pe-pdb-id`,
  `mikebom:pe-machine`, `mikebom:pe-subsystem`) populated in
  `binary/entry.rs::build_pe_identity_annotations`. Reads the
  CodeView Type-2 record + IMAGE_FILE_HEADER + IMAGE_OPTIONAL_HEADER
  via `object` 0.36's typed PE accessors in `binary/pe.rs`. PE32 vs
  PE32+ auto-dispatched by `IMAGE_OPTIONAL_HEADER.Magic`.
- Milestone 029 — cargo-auditable cross-link
  (`mikebom:detected-cargo-auditable = true`) populated in
  `binary/entry.rs::build_cargo_auditable_cross_link` when the binary
  carries a parsed `.dep-v0` manifest. Per-crate
  `pkg:cargo/<name>@<version>` components flow through a separate
  `cargo_auditable_packages_to_entries` helper (regular
  `PackageDbEntry` channel, not the bag), with the cross-link
  annotation letting consumers find them without scanning every
  component. Optional bag annotations
  `mikebom:cargo-auditable-source` and `mikebom:cargo-auditable-kind`
  emit on the per-crate components when they carry non-default
  values (non-registry source / non-runtime kind).
- Milestone 030 — Mach-O codesign metadata
  (`mikebom:macho-codesign-identifier`,
  `mikebom:macho-codesign-flags`, `mikebom:macho-codesign-team-id`)
  added to the existing `build_macho_identity_annotations` helper.
  Reads `LC_CODE_SIGNATURE` (cmd `0x1D`) → big-endian SuperBlob
  (`0xfade0cc0`) → CodeDirectory blob (`0xfade0c02`) via byte-level
  parsing. Identifier is universal; flags decoded from
  `CodeDirectory.flags` u32 bitfield (sorted JSON array, unknown
  bits emit as `unknown-0x<hex>`); team-id only present on
  CodeDirectory v2.0.5+ for developer-signed binaries (system
  binaries have `TeamIdentifier=not set`). Same first-slice
  convention as 024 for fat binaries.

The four binary-format / runtime helpers
(`build_elf_identity_annotations`,
`build_macho_identity_annotations`, `build_pe_identity_annotations`,
`build_cargo_auditable_cross_link`) are merged by the unified
`build_binary_identity_annotations` → each format's bag-emission
contract stays co-located with its source struct fields. The
Mach-O helper now owns six fields (the original three identity
signals from 024 + the three codesign signals from 030).

**Spec discipline:** typed fields stay typed. The bag is for NEW
per-binary metadata; existing fields like `binary_class`,
`evidence_kind`, `is_dev` don't migrate. If duplicate keys are
inserted (a typed field's `mikebom:*` name and a bag entry with the
same name), the parity matrix' `holistic_parity` test catches the
double-emit at PR time.

**Bag amortization receipts:** six consecutive consumers (023, 024,
025, 028, 029, 030) shipped with **zero diff** in `package_db/`,
`mikebom-common/`, `cli/`, `resolve/`, `generate/`, and unrelated
binary-format modules. The 30+ `PackageDbEntry`-init sites are
untouched by per-binary-metadata work.

Future milestones inheriting the bag without schema churn: 027
container layer attribution (`mikebom:layer-id`), the milestone-026.x
hard-cohort version-string detectors (glibc / musl / V8 — when
research clears the technical blockers documented in the deferred
backlog), Mach-O entitlements XML extraction (CSMAGIC_EMBEDDED_ENTITLEMENTS,
deferred from 030), CMS PKCS#7 cert-chain decoding (the leaf-cert
subject CN, signing time — likely unified with PE Authenticode
since both need ASN.1 DER parsing), PE DllCharacteristics security
flags (deferred from 028), and PE Authenticode signing detection
(deferred from 028 for the dep-cost reason mentioned above).

## Relevant specs

- `specs/001-build-trace-pipeline/` — original eBPF build-trace mode
- `specs/002-python-npm-ecosystem/` — Python + npm ecosystem expansion
- `specs/003-multi-ecosystem-expansion/` — Go / RPM / Maven / Cargo / Gem + foundational work
- `specs/010-spdx-output-support/` — SPDX 2.3 + SPDX 3.0.1-experimental + OpenVEX sidecar + dual-format data-placement map
- `specs/023-elf-binary-identity/` — ELF NT_GNU_BUILD_ID + RPATH/RUNPATH + .gnu_debuglink + per-component annotation bag
- `specs/024-macho-binary-identity/` — Mach-O LC_UUID + LC_RPATH + min-OS via the bag (2nd consumer)
- `specs/025-go-vcs-metadata/` — Go BuildInfo VCS metadata via the bag (3rd consumer)
- `specs/028-pe-binary-identity/` — PE CodeView pdb-id + machine + subsystem via the bag (4th consumer; binary trifecta complete)
- `specs/029-cargo-auditable-extraction/` — cargo-auditable `.dep-v0` manifest extraction → per-crate `pkg:cargo` components + cross-link annotation via the bag (5th consumer)
- `specs/030-macho-codesign-metadata/` — Mach-O codesign identifier + flags + team-ID via the bag (6th consumer)
- `.specify/memory/constitution.md` — 12-principle constitution; notable constraints: no C dependencies, no `.unwrap()` in production, generation context always stamped, packageurl-python conformance

---

## Session journal pointers

Major work milestones (for context in future sessions):

- Foundational phase (T001–T014): workspace deps, module stubs, clippy gate, pure-Rust SQLite scaffolding.
- US1 Go source + binary: `go.mod`/`go.sum` parser, BuildInfo inline decoder, module-path escaping for cache walker.
- US2 RPM: pure-Rust SQLite reader (page/record/schema), vendor ID→PURL slug mapping.
- US3 Maven: pom.xml parser with property resolution, JAR walker with embedded pom.xml support.
- US4 Cargo: v3/v4 lockfile parser, v1/v2 refusal, source-kind classification.
- US5 Gem: Gemfile.lock section parser with indent-6 transitive edge capture.
- Post-US work (what the user called "get better results"):
  - Ruby transitive edges via indent-6 parsing.
  - Go transitive tree via module cache `@v/*.mod` walker.
  - Maven M2 cache BFS walker.
  - Maven JAR-embedded pom.xml walker (non-shaded).
  - Maven parent-POM chain resolution (`EffectivePom` with `<properties>` + `<dependencyManagement>` inheritance + BOM imports).
  - Maven deps.dev `:dependencies` fallback (edge-authoritative, version-deferential).

Detailed per-feature decisions and per-commit rationale are captured in
[`CHANGELOG.md`](../CHANGELOG.md). This document intentionally stays at
the architectural-decision level; if you're looking for "what changed
in version X", read the CHANGELOG first — specifically the
`[0.1.0-alpha.3]` entries for feature-006 (SBOMit compliance suite),
the trace-mode reclassification, the artifact-vs-manifest scope gate,
and the dual-identity Maven coord emission.

## Go vs other ecosystems: the main-module asymmetry (milestone 053)

Milestone 053 introduced a **synthetic main-module component** for Go
workspace roots. Other ecosystems (cargo, npm, maven, pip, gem) do
NOT get an equivalent component. The asymmetry is deliberate.

**The Go-specific problem.** A `go.mod`'s `require` block is the only
on-disk authoritative source of direct-dependency edges from the
project to its immediate deps. Transitive edges live in each upstream
module's own `go.mod` inside the local `GOMODCACHE`. On a fresh-clone
repo with empty cache, mikebom had no `from` node for `require`-based
direct edges — they were silently dropped (issue #102). The fix:
emit a synthetic main-module component, attach the direct-require
edges to it.

**Why other ecosystems don't need this.** Their lockfiles encode
edges directly between named packages: cargo's `[[package]]
dependencies = [...]`, npm's `package-lock.json::packages`, maven's
`<dependencies>`, pip / poetry lockfile sections, gem's
`Gemfile.lock` GEM blocks. All resolve against components already in
the scan WITHOUT needing a synthetic root component. The project's
own root identity is captured in CDX `metadata.component` (synthetic
placeholder) and SPDX `documentDescribes`.

**Native-field placement (Principle V).** Per Constitution v1.4.0
the Go main-module is emitted via each format's standards-native
"BOM subject" construct — CDX `metadata.component` with `type:
"application"` (NOT a sibling in `components[]`); SPDX 2.3
`primaryPackagePurpose: "APPLICATION"` plus `documentDescribes`;
SPDX 3.0.1 `software_primaryPurpose: "application"`. The
`mikebom:component-role: main-module` (C40) annotation is
supplementary signal layered on top. Matches Trivy's pattern.

**Constraint to maintain.** When adding a new ecosystem reader, DO
NOT add a synthetic main-module component without going through the
same design pass milestone 053 did. The Go synthetic-root pattern is
a Go-specific solution to a Go-specific edge-encoding problem; it is
NOT a general "every ecosystem needs a project-itself component"
template. Per-ecosystem main-modules for project-identification /
vuln-intersection / sbomqs-uniformity reasons are tracked separately
in issue #104; not in scope for milestone 053.

## Filesystem walking pattern (milestone 054)

Every `fn walk*` (or `fn walk_dir`) function in
`mikebom-cli/src/scan_fs/` MUST detect symlink loops and bound
recursion. Two valid protection mechanisms; pick whichever fits the
walker's structure:

1. **Canonicalize-keyed visited set + max-depth backstop**
   (mandatory for walkers that follow symlinks via `path.is_dir()`,
   which dereferences). Reference implementations:
   `package_db/golang.rs::walk_for_go_roots`,
   `package_db/project_roots.rs::walk_for_project_roots`. Pattern:

   ```rust
   const MAX_WALK_DEPTH: usize = 16;  // or tighter with justification
   let mut visited: HashSet<PathBuf> = HashSet::new();
   walk(root, 0, &mut visited, &mut out);

   fn walk(dir: &Path, depth: usize, visited: &mut HashSet<PathBuf>,
           out: &mut Vec<...>) {
       if depth >= MAX_WALK_DEPTH { return; }
       let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
       if !visited.insert(key) {
           tracing::debug!(path = %dir.display(), "walker: cycle/visited skip");
           return;
       }
       // ... read_dir + recurse with `walk(&p, depth + 1, visited, out)` ...
   }
   ```

2. **lstat-equivalent file-type check** (acceptable for walkers that
   intentionally don't follow symlinks — `entry.file_type()` does
   NOT dereference). Reference implementations:
   `scan_fs/walker.rs::walk`, `package_db/maven.rs::walk_m2_jars`,
   `package_db/maven_sidecar.rs::walk`,
   `package_db/npm/walk.rs::walk_node_modules`. These walkers MUST
   carry an inline `// SAFETY (milestone-054 walker audit):` comment
   naming the lstat-skip mechanism so a future audit grep confirms
   they're protected. Pattern:

   ```rust
   for entry in read_dir.flatten() {
       let ft = entry.file_type().ok().filter(|ft| ft.is_dir());
       // ft is None for symlinked-dirs (lstat doesn't deref) — loop
       // physically can't follow symlinks. Continue with the file.
   }
   ```

**Audit gate.** PR-review time check:
`grep -rn "fn walk" mikebom-cli/src/scan_fs/` MUST find every match
either using mechanism 1 (visible `HashSet<PathBuf>` parameter) OR
mechanism 2 (inline `// SAFETY:` comment). Any walker matching
neither is a blocking review finding. The milestone-054 audit
seeded this rubric; future contributors keep it satisfied.

**Per-walker depth limits**. Default ceiling is 16 (deeper than any
realistic monorepo's natural nesting). Walkers MAY use tighter
bounds (cargo: 6, gem: 6, go_binary: 10, maven: 6) when the
ecosystem's structural conventions justify it — but MUST carry an
inline justification comment naming the specific reason (e.g.,
"Cargo workspaces are shallow by convention"). Per milestone-054
FR-003.

**Future migration**. Issue #108 tracks the migration from per-
walker hand-rolled visited-set logic to a single shared `safe_walk`
helper. Until then the pattern above is the canonical reference for
new ecosystem readers — copy it from the closest existing walker,
not from a `walkdir` crate dependency (mikebom's Cargo.toml has a
deliberate minimal-dependency posture).
