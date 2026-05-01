# Changelog

All notable changes to mikebom are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project
adheres to [Semantic Versioning](https://semver.org/) once it exits
`0.1.x` alpha.

## [Unreleased]

(Nothing yet. Land changes here, then cut a release per the
`release.yml` workflow trigger documented in
`docs/contributing/release.md`.)

## [0.1.0-alpha.9] — 2026-05-01

A small targeted release covering one user-facing fix shipped
since alpha.8 (~1 day later): milestone 049's correction of the
Go source-tree component scope. Resolves an audit-grounded gap
where `mikebom sbom scan --path` on a Go project emitted only
the project's directly-imported modules (collapsing legitimate
transitive prod deps into the dropped-as-test-only bucket).

### Changed

- **Go source-tree scans now emit the full go.sum closure by
  default** (milestone 049). Previously the source-tree filter
  dropped every entry not directly imported by this project's
  non-`_test.go` files, collapsing legitimate transitive prod
  deps (e.g., aws-sdk internals, gin's middleware chain) into
  the test-only bucket. Audit on `apigatewayv2/config` showed
  6 components emitted vs. 55 in trivy / 56 in syft. The new
  default emits every `go.sum` entry as a component (matches
  trivy/syft) and only TAGS the small subset proven test-only
  by source-walking the project's `_test.go` imports. Test-only
  deps carry the existing `mikebom:dev-dependency = true`
  annotation when `--include-dev` is set; default-mode drops
  them (mirrors npm/Poetry/Pipfile semantics). No new flag,
  no new annotation, no new catalog row. CDX + SPDX 2.3 +
  SPDX 3 outputs all carry the new emission via existing
  parity wiring.

  Scope: Go-only. cargo / gem / maven test-tagging extension
  tracked as milestone 050 (see specs/049-go-source-scope/).

## [0.1.0-alpha.8] — 2026-04-30

A small targeted release covering one user-facing feature
shipped since alpha.7 (~1 hour after alpha.7): the
`mikebom:component-role` annotation surfacing
filesystem-position-classified component roles in CDX + SPDX 2.3
+ SPDX 3 outputs. Audit-grounded — addresses 3 false-positive
Maven build-tool JARs surfaced in the alpha.7 polyglot-builder-
image conformance run.

- **Build-tool and language-runtime components are now
  explicitly tagged** in every output format. Maven's own
  internals at `/usr/share/maven/lib/`, JDK system-installed
  JARs at `/usr/lib/jvm/*/lib/`, system Python packages at
  `/usr/lib/python*/site-packages/` and `dist-packages/`, and
  comparable build-tool / language-runtime paths now carry
  `mikebom:component-role = "build-tool"` or
  `mikebom:component-role = "language-runtime"`. Downstream
  consumers (vulnerability scanners, license auditors,
  conformance ground-truths) can filter on the annotation
  without re-implementing the path-heuristic.

### Added

- **`mikebom:component-role` annotation** (048). Components
  whose `evidence.occurrences[]` paths match a curated
  filesystem heuristic now carry a `mikebom:component-role`
  annotation classifying them as `build-tool` (under
  `/usr/share/maven/lib/`, `/usr/share/gradle/lib/`, `/opt/sbt/`)
  or `language-runtime` (under `/usr/lib/jvm/*/lib/`,
  `/usr/lib/node_modules/`, `/usr/lib/python*/site-packages/`,
  `/usr/lib/python*/dist-packages/`). Three-state semantics:
  components without a heuristic match get NO annotation —
  absence does NOT mean "definitely application code", it
  means the heuristic didn't classify. Emitted symmetrically
  across CDX `properties[]`, SPDX 2.3 `packages[].annotations[]`,
  and SPDX 3 top-level `annotations[]` (catalog row C40,
  SymmetricEqual). Lets downstream consumers (vulnerability
  scanners, license auditors, conformance suites) filter
  build-tooling and platform runtime libraries from
  application-deps reporting without mikebom dropping any
  component from the SBOM.

## [0.1.0-alpha.7] — 2026-04-30

A small docs + SPDX-parity release. Two days after alpha.6, with
a focus on closing user-facing gaps surfaced during alpha.6
adoption: SPDX consumers' scope context, README staleness, and
CI flake hardening so the next milestone lands cleanly.

- **SPDX-side document-level scope hint** (047). SPDX 2.3 +
  SPDX 3 outputs now self-describe scope at the document level,
  closing the parity gap with CDX's `metadata.lifecycles[]`.
  Closes the user-reported "is mikebom undercounting?"
  conversational ambiguity by making scope explicit in every
  format.
- **README post-alpha.6 docs refresh** (046). Closes 10 audited
  drift items in user-facing docs: stale version pin,
  `--image-src` flag missing from CLI reference, registry-first
  framing for `--image` (default is now docker-daemon-first),
  internal-milestone-number jargon leaking into user docs,
  `--include-legacy-rpmdb` "deferred" framing for shipped
  behavior.
- **CI test-suite flake hardening** (045 + 044-followon).
  Diagnosed two genuine flake patterns from a 60-run audit:
  macOS-runner perf-test variance (now median-of-5 sampling)
  and a timestamp-race on byte-identity tests (now pinned via
  `MIKEBOM_FIXED_TIMESTAMP` in subprocess-spawning helpers).
  Plus a new gated end-to-end integration test for the
  docker-daemon image source. Test-only — no production
  behavior change.

### Added

- **SPDX-side document-level scope hint** (047). SPDX 2.3
  `creationInfo.comment` and SPDX 3 `SpdxDocument.comment` now
  carry a free-text scope summary naming the scope mode
  (artifact vs manifest, derived from `--include-declared-deps`),
  the observed lifecycle phases (mirroring CDX
  `metadata.lifecycles[]`), and a pointer to the per-component
  `mikebom:sbom-tier` annotation for finer-grained detail. SPDX
  consumers reading metadata-only now get the same scope
  context CDX consumers already had via
  `metadata.lifecycles[]`. CDX output unchanged.
- **README "What kind of SBOM does mikebom emit?" section**
  (047). New top-level section between "Why" and "Install"
  explaining mikebom's two scope axes (document-level
  artifact-vs-manifest mode + per-component lifecycle tier),
  how each format self-describes its scope, and how mikebom's
  default scopes map to industry / NTIA-style terminology — so
  operators comparing component counts to trivy / syft can see
  the question being asked rather than wonder whether mikebom
  is undercounting.
- **End-to-end docker-daemon integration test** (044
  follow-on). Gated on `docker --version` + `docker info`
  succeeding; pulls `alpine:3.19`, runs `mikebom sbom scan
  --image alpine:3.19 --image-src docker`, asserts the SBOM
  was produced via the docker-daemon path and contains ≥5
  components. Skips cleanly on CI lanes without docker
  (macOS-latest).

### Changed

- **README + `docs/user-guide/cli-reference.md` reflect
  post-alpha.6 reality** (046). Status pin updated to alpha.6;
  `--image-src docker,remote` flag documented; `--image`
  description updated to describe docker-daemon-first default;
  `--include-legacy-rpmdb` description rewritten to drop
  "deferred until that code lands" framing for long-shipped
  BDB rpmdb reading; OCI-cache flag rows cross-link to the
  `OCI layer caching` section. Also drops internal milestone
  numbers from user-facing docs (CHANGELOG and design-notes
  retain them as appropriate).

### Fixed

- **macOS perf-test flake** (045). `dual_format_perf` and
  `triple_format_perf` failed intermittently on macos-latest
  CI runners (observed 9.0% / 14.4% / 19.9% reduction vs the
  25% gate, while local distribution sits around 50%). Bumped
  median-of-3 → median-of-5 sampling — cuts the median's
  variance by ≈40% so macOS CPU contention spikes don't push
  the measurement below the gate. CI gate (25%) and spec
  target (30%) unchanged.
- **SPDX byte-identity test flake** (045). Three byte-identity
  tests (`spdx_3_alias_bytes_are_byte_identical_to_stable_
  identifier`, `scenario_7_alias_bytes_are_byte_identical`,
  `scenario_8_mikebom_no_deprecation_notice_env_suppresses_
  stderr_warning`) compared raw bytes across two sequential
  subprocess invocations. When the two invocations straddled
  a second-boundary, `creationInfo.created` diverged at
  second precision, surfacing as a CI flake on unrelated
  branches. Pinned `MIKEBOM_FIXED_TIMESTAMP` in the two
  subprocess-spawning helpers (the env var was added in
  milestone 011 specifically for this case but the helpers
  weren't using it).

## [0.1.0-alpha.6] — 2026-04-29

A small, focused release: makes `mikebom sbom scan --image <ref>`
behave the way users coming from trivy and syft expect, and
unblocks AWS ECR pulls that were previously failing on a Basic
auth challenge.

- **Docker daemon as a default image source.** When `--image
  <ref>` is an OCI reference, mikebom now checks the local docker
  daemon's cache first and falls back to a registry pull only on
  miss. Matches trivy's `--image-src` and syft's auto-detection
  convention. The new `--image-src docker,remote` flag (default
  in that order) controls the resolution sequence; pass
  `--image-src remote` to force a fresh registry fetch.
- **AWS ECR support for the registry path.** The OCI-pull's
  401-retry now handles `Basic` auth challenges in addition to
  `Bearer`, applying cached `~/.docker/config.json` credentials
  directly. ECR's `aws ecr get-login-password | docker login`
  flow now works end-to-end with `--image-src remote`.

Together these resolve the reported case where an ECR image was
already cached locally and `docker login`'d, but mikebom errored
out with `WWW-Authenticate is not a Bearer challenge: Basic ...`.

### Added

- **`Basic` auth challenge support for the OCI registry pull** (044
  commit 2). The 401-retry path now accepts both `Bearer` (existing
  Docker Hub / GHCR / gcr.io flow) and `Basic` (AWS ECR's flavor)
  `WWW-Authenticate` challenges. For `Basic`, mikebom applies the
  cached docker-config credentials directly on the original request
  — no token-realm round-trip. Resolves the previous
  `WWW-Authenticate is not a Bearer challenge: Basic ...` error
  on `mikebom sbom scan --image <ecr-ref> --image-src remote`. The
  `~/.docker/config.json` lookup is unchanged (already supported
  `auths.<host>.auth`, `credHelpers`, `credsStore` since milestone
  034); only the challenge parser was Bearer-only.
- **Local docker daemon as a default image source** (044 commit 1).
  `mikebom sbom scan --image <ref>` now consults the local docker
  daemon before reaching for a registry pull, matching trivy and
  syft conventions. New `--image-src docker,remote` flag controls
  the source-resolution order; default is `docker,remote`. Force a
  fresh registry fetch with `--image-src remote`. Docker source
  shells out to `docker image inspect` + `docker save`, so the
  user's existing `DOCKER_HOST` / contexts are honored. Resolves
  the case where an ECR image is already cached locally but the
  registry pull is failing (e.g. on a Basic-auth challenge).

## [0.1.0-alpha.5] — 2026-04-29

Cuts a new pre-release covering everything merged since
alpha.3 (the alpha.4 tag was a CHANGELOG-less mechanical
bump). Ships milestones 010, 023–030, and 034–042 together.
Highlights:

- **Container per-file evidence trilogy** (037 → 040 → 041):
  deb, apk, and rpm components all carry populated
  `evidence.occurrences[]` blocks now, plus matching
  upstream-cross-ref checksums (`md5` / `sha1` /
  `rpm_filedigest`) in `additionalContext`.
- **Direct OCI registry image scanning** (034 → 036):
  `mikebom sbom scan --image alpine:3.19` now pulls from
  registries directly, including authenticated private pulls
  via the standard Docker keychain, cross-arch selection via
  `--image-platform`, and SHA-256-content-addressed disk
  caching for fast repeat scans.
- **Distroless / chainguard / Bazel-built minimal-image
  coverage** (037 → 038): the per-package
  `/var/lib/dpkg/status.d/` layout and its `.md5sums`
  companion files are now read; deb minimal images go from
  zero components to a full SBOM with per-file evidence.
- **Mach-O binary identity + codesign + Go VCS metadata**
  (024 → 025 → 030): macOS and Apple-platform binaries now
  emit `LC_UUID`, `LC_RPATH`, codesign identifier / flags /
  team-id, and Go-binary VCS commit-SHA + build-time
  metadata.
- **Maven sidecar Debian layout** (042): in addition to
  Fedora's `/usr/share/maven-poms/`, mikebom now reads
  Debian's `/usr/share/maven-repo/` GAV-tree layout, so
  `lib*-java`-installed JARs surface as
  `pkg:maven/<group>/<artifact>@<version>` PURLs.
- **Two cross-ref-symmetry milestones** (040 US2 and 041)
  bring apk and rpm to parity with deb's longstanding `md5`
  cross-ref carrier on per-file occurrences.

Detailed entries below.

### Added
- **Milestone 042 — Post-041 small follow-ons.** Two unrelated
  legacy-deferral items closed:
  - **US1 (housekeeping)**: dropped a stale comment in
    `binary/predicates.rs:88` that named rpm file-list
    extraction from HeaderBlob `BASENAMES` / `DIRNAMES` /
    `DIRINDEXES` as "deferred to a follow-on milestone." That
    work shipped in milestone 040 US3; the comment now
    accurately credits 040 US3 as the authoritative claim
    source and explains the directory-heuristic's role as a
    defense-in-depth fallback for corrupt / partial rpmdb cases.
  - **US2 (Maven sidecar Debian layout)**: extends
    `maven_sidecar.rs` with a parallel `DebianSidecarIndex`
    that walks `/usr/share/maven-repo/` (the GAV-tree layout
    populated by Debian's `maven-repo-helper` during
    `apt-get install lib*-java`). Debian-shaped Java images
    that previously emerged as `pkg:generic/<filename>` PURLs
    now resolve to `pkg:maven/<group>/<artifact>@<version>` —
    matching the milestone-007 Fedora-side coverage.
    Implementation introduces a small `SidecarIndex` trait so
    `resolve_coords` works generically over either layout.
    Fedora wins on basename collision (FR-005). Alpine
    layouts remain out of scope (Alpine ships no documented
    system-wide maven repo convention).
  - 6 new inline tests for the Debian sidecar reader; 27
    byte-identity goldens regen with zero diff (no fixture
    contains `/usr/share/maven-repo/` content).
- **Milestone 041 — Rpm FILEDIGESTS cross-reference.** Closes
  the milestone-040 Q1 deferral. Every populated rpm
  `evidence.occurrences[]` entry's `additionalContext` JSON-
  string now carries `rpm_filedigest` alongside the existing
  `sha256`, in algorithm-prefixed form (e.g.
  `"sha256:abc..."` for modern rpm packages,
  `"md5:def..."` for legacy ones). The algorithm matches the
  package's `FILEDIGESTALGO` value (or defaults to MD5 when
  absent per the rpm spec). Brings rpm to full cross-ref
  symmetry with deb (`md5`, since milestone 037) and apk
  (`sha1`, since milestone 040 US2).
  Verified end-to-end against `fedora:40`: 6938 of 6966
  total file occurrences carry the cross-ref (99.6%; the
  28 remainder are non-regular files whose `FILEDIGESTS`
  entry is empty by rpm-spec convention). Sample value
  `rpm_filedigest = "sha256:7544bd..."` matches the
  mikebom-observed `sha256` for the same file — the
  integrity-check arrow goes both ways. New
  `rpm_file_digest: Option<String>` field on
  `mikebom_common::resolution::FileOccurrence` (additive,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`).
  No new top-level dependencies. 27-fixture goldens regen
  with zero diff. See `specs/041-rpm-filedigests/spec.md`.
- **Milestone 040 — Package-DB follow-ons (trifecta).** Three
  sequenced follow-on items closing coverage and hygiene gaps
  after milestones 037 / 038 / 039:
  - **US1 (housekeeping)**: dropped a stale "deferred to
    milestone 031.y" framing in `oci_pull/mod.rs::host_oci_arch`
    that named `--image-platform` as deferred. The flag shipped
    in milestone 035 (PR #72); the error message now positively
    references it with an example invocation.
  - **US2 (apk SHA-1 cross-ref)**: extends milestone 039's apk
    per-file evidence with the apk-provided SHA-1 from each `Z:`
    line in `/lib/apk/db/installed`. Surfaced as `sha1` in the
    per-occurrence `additionalContext` JSON-string alongside the
    mikebom-computed `sha256`. Mirrors deb's `md5` cross-ref
    contract from milestone 037. New `ApkFileEntry` struct in
    apk.rs, new optional `apk_sha1: Option<String>` field on
    `mikebom_common::resolution::FileOccurrence` (additive,
    `#[serde(default, skip_serializing_if = "Option::is_none")]`).
    Verified end-to-end against `alpine:3.19`.
  - **US3 (rpm per-file deep-hash)**: completes the OS-package
    per-file-evidence trilogy. rpm-based images (fedora,
    almalinux, rocky, centos:stream, redhat/*) now produce
    populated `evidence.occurrences[]` blocks at parity with
    deb (037/038) and apk (039). New
    `rpm::read_file_lists(rootfs)` exposes the per-package
    file-list map decoded from the rpmdb HeaderBlob's
    `BASENAMES` / `DIRNAMES` / `DIRINDEXES` triple via the
    existing `iter_rpmdb` helper; new `hash_rpm_package_files`
    + `hash_rpm_db_only` mirror the apk pattern; new `is_rpm`
    branch in `scan_fs/mod.rs::read_all`. Verified end-to-end:
    `fedora:40` produces 147 components with 6966 total file
    occurrences (was 0). Per the milestone-040 Q1
    clarification, rpm FILEDIGESTS cross-ref is OUT of scope
    and deferred to a separate follow-on milestone — rpm-side
    `additionalContext` carries SHA-256 only.
  - No new top-level Cargo dependencies. 27 byte-identity
    goldens regen with zero diff (the goldens use
    `--no-deep-hash` so they're insulated from the deep-hash
    path by design).
  - See `specs/040-pkg-db-followups/spec.md`.
- **Milestone 039 — Per-file evidence for apk components
  (alpine + chainguard apko + Wolfi).** Closes the asymmetry
  surfaced during milestone 038's recon (#75): apk-based images
  now produce per-file `evidence.occurrences[]` blocks at the
  same quality as deb-based images. Implementation mirrors the
  dpkg deep-hash path: a new `apk::read_file_lists` extracts
  per-package paths from the `F:` (directory) and `R:` (regular
  file) lines that the apk installed-db carries inline; a new
  `hash_apk_package_files` walks those paths, opens each file,
  and SHA-256s the content (same 256 MB cap as the dpkg path).
  A parallel `--no-deep-hash` fast path
  (`hash_apk_db_only`) hashes the package's stanza bytes
  in-place. Verified end-to-end:
  `alpine:3.19` produces 79 file occurrences across 15
  components (was 0); `cgr.dev/chainguard/static:latest`
  produces 1217 occurrences across 3 components (was 0). 27
  byte-identity goldens regen with zero diff (those goldens use
  `--no-deep-hash` so they're insulated from the deep-hash path
  by design). Apk-side `additionalContext` carries SHA-256 only;
  the apk-provided SHA-1 (`Z:` lines) is a future extension. No
  new top-level dependencies. Closes #75. See
  `specs/039-apk-deep-hash/spec.md`.
- **Milestone 038 — Per-file evidence for distroless /
  Bazel-built minimal-image deb scans.** Closes the deferred
  milestone-037 item: distroless deb images
  (`gcr.io/distroless/*`, rules-distroless, similar Bazel-built
  minimal images) now produce populated
  `evidence.occurrences[]` blocks with per-file paths and
  SHA-256 + MD5 hashes — matching the evidence quality
  full-fat-image scans have produced since the early
  milestones. Implementation: extended
  `file_hashes.rs::read_info_file{,_bytes}` lookup chain to
  fall back to `var/lib/dpkg/status.d/<pkg>.<ext>` after the
  legacy `info/` paths, and synthesized the path list from the
  second column of `<pkg>.md5sums` when `<pkg>.list` is absent.
  Stanzas in this layout legitimately omit the `Status:` field
  (no dpkg daemon manages install state in the image), so a
  relaxed parse path was added that treats the stanza file's
  existence as the installation marker; strict filtering is
  preserved for the legacy `status` file source. Verified
  end-to-end: `gcr.io/distroless/static-debian12:latest` now
  produces 4 components with 938 total file occurrences (was
  0). 27 byte-identity goldens regen with zero diff.
  Out-of-scope concurrent finding: apk per-file evidence is
  empty for both `alpine:3.19` and chainguard apko/wolfi
  images — mikebom's `file_hashes.rs` is dpkg-only. Filed as
  follow-on issue
  [#75](https://github.com/kusari-sandbox/mikebom/issues/75)
  for a future milestone. See
  `specs/038-minimal-image-deep-hash/spec.md`.
- **Milestone 037 — distroless / chainguard / Bazel minimal-image
  dpkg coverage.** mikebom now reads per-package metadata from
  `/var/lib/dpkg/status.d/<pkgname>` files in addition to the
  legacy single-file `/var/lib/dpkg/status`. Closes the
  milestone-031-surfaced gap where mikebom reported 0 deb
  components for `gcr.io/distroless/static-debian12:latest` and
  similar minimal images that ship per-package metadata files
  instead of the monolithic dpkg-daemon-managed `status` file.
  Same coverage syft and trivy already provided. Filtering uses
  parse-success-or-skip so companion files (`<pkg>.md5sums`,
  `.conffiles`, etc.) naturally drop out without breaking on
  package names that contain dots (`python3.11`). When both
  layouts are present (defensive — never seen in practice), the
  `status.d/` source wins. No new dependencies, no SBOM-shape
  changes, no parity-catalog impact. Closes #64. See
  `specs/037-dpkg-status-d/spec.md`.
- **Milestone 036 (031.z) — On-disk cache for pulled OCI image blobs.**
  Repeat-scans of the same image now skip the network fetch and
  read from a SHA-256-content-addressed cache on disk, completing
  in seconds rather than tens of seconds for non-trivial images.
  The cache lives at `$MIKEBOM_OCI_CACHE_DIR` →
  `$XDG_CACHE_HOME/mikebom/oci-layers` →
  `$HOME/Library/Caches/mikebom/oci-layers` (macOS) →
  `$HOME/.cache/mikebom/oci-layers` (fallback). Default size cap
  10 GB with mtime-based LRU eviction; configurable via
  `--oci-cache-size <bytes>` or `MIKEBOM_OCI_CACHE_SIZE=<bytes>`.
  Disable with `--no-oci-cache` or `MIKEBOM_OCI_CACHE=0`. Every
  cache read re-verifies SHA-256 against the digest, so silent
  corruption is detected and recovered (drop entry + re-fetch).
  Atomic-rename writes (tempfile + persist) keep concurrent scans
  safe. Best-effort posture: any IO failure (read-only fs, missing
  $HOME) falls through to network-only behavior; scans complete
  either way. Manifests are NOT cached (floating tags like
  `:latest` need to re-fetch). No new dependencies. Closes #68.
  See `specs/036-oci-layer-cache/spec.md` and the new
  ["OCI layer caching"](docs/user-guide/cli-reference.md#oci-layer-caching)
  section in the user guide.
- **Milestone 035 (031.y) — `--image-platform` CLI flag for cross-arch
  image scans.** New `mikebom sbom scan --image <ref>
  --image-platform linux/<arch>[/<variant>]` selects a specific
  platform from a multi-arch image index instead of auto-resolving
  to `linux/<host-arch>`. Common shapes: `linux/amd64`,
  `linux/arm64`, `linux/arm/v7`, `linux/386`, `linux/ppc64le`,
  `linux/s390x`. The variant segment is honoured for indexes that
  carry it (e.g. arm v6 vs v7 vs arm64 v8). Closes the macOS-arm64
  dev / Linux-x86_64 CI workflow gap that previously required
  `docker pull --platform <X> && docker save` to scan a non-host
  image. Registry-only — passing `--image-platform` alongside a
  pre-extracted tarball errors clearly. Non-`linux` OS values
  reject with an explanation that mikebom's package-DB readers are
  linux-rootfs-shaped. No SBOM-shape changes (the byte-identity
  goldens regen produces zero diff). Closes #67. See
  `specs/035-image-platform-flag/spec.md` and the new flag row in
  `docs/user-guide/cli-reference.md`.
- **Milestone 034 (031.x) — Authenticated OCI registry pulls.**
  `mikebom sbom scan --image <ref>` now supports private registries
  via the standard Docker keychain — the same `~/.docker/config.json`
  (or `$DOCKER_CONFIG/config.json`) that `docker pull` uses. All four
  documented credential sources resolve in Docker's documented
  precedence order: per-registry `credHelpers` > registry-wide
  `credsStore` > direct `auths.<reg>.auth` (base64 user:password) >
  `auths.<reg>.identitytoken`. Credential helpers are invoked as
  subprocesses (`docker-credential-<helper> get`) per the published
  protocol — covers ECR (`docker-credential-ecr-login`), Google
  Artifact Registry (`docker-credential-gcloud`), macOS keychain,
  Windows credential store, GNOME Secret Service, and `pass`. When
  credentials resolve, they're sent as Basic auth on the
  bearer-token realm GET; the resulting bearer token authorizes the
  manifest + blob fetches. Anonymous fallback is preserved: no
  config.json + public image works exactly as it did in milestone
  031. Credentials never leak to stdout, stderr, `--verbose` output,
  or `RUST_LOG=debug` traces — `Credential::Debug` redacts both
  fields and the helper subprocess's stderr is dropped to /dev/null.
  No new top-level dependencies; the
  `no_c_dependencies_in_oci_registry_feature_tree` regression test
  still passes. See `specs/034-authenticated-registry-pulls/spec.md`
  and the new ["Authenticating to private registries"](docs/user-guide/cli-reference.md#authenticating-to-private-registries)
  section in the user guide. Closes #66.
- **Milestone 030 — Mach-O codesign metadata.** Every Mach-O scan
  now extracts three identity-flavored signals from the
  `LC_CODE_SIGNATURE` (cmd `0x1D`) SuperBlob's CodeDirectory blob:
  `mikebom:macho-codesign-identifier` (e.g. `com.apple.ls` —
  universal across Apple-signed binaries),
  `mikebom:macho-codesign-flags` (JSON array decoded from
  `CodeDirectory.flags` — `hardened-runtime`, `library-validation`,
  `adhoc`, etc.; unrecognized bits emit as `unknown-0x<hex>`), and
  `mikebom:macho-codesign-team-id` (10-char Apple Team ID for
  developer-signed binaries; absent for Apple-system signatures
  whose `TeamIdentifier=not set` and for ad-hoc signatures). This
  is what `codesign -dvv` reads. Fat / universal binaries report
  from the first slice (matching milestone 024's convention).
  **Sixth amortization-proof consumer of the milestone-023
  `extra_annotations` bag** (after 023/024/025/028/029 — 026 was
  a coverage-breadth milestone that didn't touch the bag). No new
  crate dependencies. CMS PKCS#7 cert-chain decoding (which would
  extract the leaf-cert subject CN, signing time, intermediate
  cert hashes — requires ASN.1 DER parsing) and entitlements XML
  extraction explicitly deferred to a follow-on milestone (likely
  unified with PE Authenticode, which has the same DER-parsing
  requirement). See `specs/030-macho-codesign-metadata/spec.md`
  and catalog rows C37/C38/C39 in
  `docs/reference/sbom-format-mapping.md`.
- **Milestone 029 — cargo-auditable extraction.** Extracts the
  zlib-compressed JSON manifest from Rust binaries' `.dep-v0` linker
  section ([cargo-auditable](https://github.com/rust-secure-code/cargo-auditable)
  format) and surfaces the full build-time crate dependency closure as
  per-crate `pkg:cargo/<name>@<version>` components with
  `evidence-kind = "cargo-auditable"`, `confidence = "high"`,
  `parent_purl` cross-linking back to the file-level binary, and
  index-based `dependencies` resolved into `depends` edges. The binary
  itself gains a `mikebom:detected-cargo-auditable = true` cross-link
  annotation (Rust analog of milestone 005's `mikebom:detected-go =
  true`). Cargo wrappers in Debian Trixie+, Fedora 40+, Alpine Edge,
  and the official Rust container images auto-enable the embedding —
  so most Rust binaries built in those environments now surface their
  full statically-linked crate closure without source access. Cross-
  format: ELF / Mach-O / PE. Optional bag annotations
  `mikebom:cargo-auditable-source` (non-registry sources) and
  `mikebom:cargo-auditable-kind` (non-runtime kinds) preserve
  manifest detail. **Fifth amortization-proof consumer of the
  milestone-023 `extra_annotations` bag** (after 023/024/025/028 —
  026 was a coverage-breadth milestone that didn't touch the bag).
  No new crate dependencies — `flate2` and `serde_json` were already
  in the workspace. See `specs/029-cargo-auditable-extraction/spec.md`
  and catalog row C36 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 026 — curated version-string scanner expansion (easy-4
  cohort).** Extends `version_strings.rs`'s curated scanner from 7 to
  **11 self-identifying native libraries**. Four new detectors with
  clean self-identifying signatures in the binary's read-only string
  region:
  - **GnuTLS** (`GnuTLS X.Y.Z`) — common in curl-with-GnuTLS, wget,
    GnuPG, GNU-stack tools.
  - **LibreSSL** (`LibreSSL X.Y.Z`) — macOS system tools (system curl
    was LibreSSL-backed for years), OpenBSD-derived utilities.
  - **LLVM** (`LLVM version X.Y.Z`) — strict prefix; bare `LLVM ` is
    too noisy (matches `LLVM ERROR:`, `LLVM IR ...` etc.).
  - **OpenJDK** — two-scheme parser handling both modern JEP 322
    (`21.0.1+12`) and legacy Java 8 (`8u362-b09`).

  Each match emits a `pkg:generic/<library>@<version>` component with
  `mikebom:evidence-kind = "embedded-version-string"` and
  `mikebom:confidence = "heuristic"`, flowing through the existing
  `version_match_to_entry` machinery (no downstream wiring change).
  9 new inline tests cover positive + negative cases per library
  plus a `libressl_distinct_from_openssl` cross-validation test.

  Three additional libraries from the original wishlist (glibc, musl,
  V8) are deferred to a 026.x research-and-attempt follow-on because
  they don't have clean self-identifying strings in `string_region` —
  glibc's `GLIBC_X.Y` lives in the `.gnu.version_r` ELF section, musl
  rarely self-identifies in compiled output, and V8's version strings
  are buried in stack-trace formatting code. Tracked via
  `TODO(milestone-026.x)` in `version_strings.rs` and the
  "Deferred backlog" section of `docs/design-notes.md`. See
  `specs/026-version-string-library-expansion/spec.md`.

  Note: this milestone is **not** a `extra_annotations` bag consumer —
  it produces new components rather than annotations on existing
  components. The bag-amortization streak from 023/024/025/028 stays
  at four; 026 is purely scanner coverage breadth.
- **Milestone 028 — PE binary identity.** Every Windows-binary scan
  now surfaces three identity signals via `object` 0.36's typed PE
  accessors: `mikebom:pe-pdb-id` (the `<guid-hex-lowercase>:<age>`
  pair from the CodeView Type-2 record in `IMAGE_DIRECTORY_ENTRY_DEBUG`
  — the canonical PE binary identity used by Microsoft Symbol Server,
  Mozilla / Chromium symbol stores, WinDbg, drmingw; analog of
  Linux's NT_GNU_BUILD_ID and macOS's LC_UUID), `mikebom:pe-machine`
  (lowercase `IMAGE_FILE_HEADER.Machine` — `amd64` / `i386` /
  `arm64` / `armnt` / `ia64` / `riscv32` / `riscv64` / `unknown`),
  and `mikebom:pe-subsystem` (lowercase
  `IMAGE_OPTIONAL_HEADER.Subsystem` — `console` / `windows-gui` /
  `efi-application` / `native` / etc., with `WINDOWS_CUI` rendering
  as `console` per Microsoft toolchain idiom). PE32 vs PE32+
  bit-width is auto-dispatched by reading
  `IMAGE_OPTIONAL_HEADER.Magic` (`0x10B` vs `0x20B`). With ELF (023)
  and Mach-O (024) already shipping, this completes the binary-
  identity trifecta — every compiled binary mikebom scans now
  carries cross-platform identity in the SBOM. Surfaced via the
  milestone-023 generic annotation bag — the **fourth** amortization-
  proof consumer, with zero churn in `package_db/`, `mikebom-common/`,
  `cli/`, `resolve/`, `generate/`, `elf.rs`, or `macho.rs`. See
  `specs/028-pe-binary-identity/spec.md` and catalog rows
  C33/C34/C35 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 024 — Mach-O binary identity.** Every macOS-binary
  scan now surfaces three identity signals from byte-level Mach-O
  load-command parsing: `mikebom:macho-uuid` (16-byte LC_UUID
  hex-encoded lowercase — the macOS analog of NT_GNU_BUILD_ID; used
  by `dwarfdump`, `xcrun symbolicatecrash`, the macOS crash reporter,
  and every `*.dSYM` bundle for symbol matching),
  `mikebom:macho-rpath` (LC_RPATH paths in declaration order, dedup'd
  — `@executable_path` / `@loader_path` / `@rpath` recorded raw,
  runtime-context-dependent expansion deferred to consumers), and
  `mikebom:macho-min-os` (`<platform>:<version>` shape — e.g.
  `macos:14.0`, `ios:17.5` — preferring `LC_BUILD_VERSION`, falling
  back to `LC_VERSION_MIN_MACOSX` / `LC_VERSION_MIN_IPHONEOS` /
  `LC_VERSION_MIN_TVOS` / `LC_VERSION_MIN_WATCHOS`). Fat / universal
  Mach-O binaries report from the FIRST slice's bytes (per-slice
  arch-divergence is uncommon in practice; consumers needing it can
  fall back to `otool -l <slice>`). SC-002 verified on the macOS CI
  lane: `/bin/ls` scan emits a non-empty 32-lowercase-hex
  `mikebom:macho-uuid` and a non-empty `<platform>:<version>`
  `mikebom:macho-min-os` — both universal on every supported macOS
  version. Surfaced via the milestone-023 generic annotation bag,
  with zero PackageDbEntry-init churn (the bag's amortization
  payoff). 3 atomic commits; see `specs/024-macho-binary-identity/spec.md`
  and catalog rows C30/C31/C32 in `docs/reference/sbom-format-mapping.md`.
- **Milestone 025 — Go BuildInfo VCS metadata.** Every Go-binary scan
  now surfaces the source-tree VCS state recorded at build time. The
  main-module entry (`pkg:golang/<module>@<version>`) gains three new
  annotations across CDX / SPDX 2.3 / SPDX 3:
  `mikebom:go-vcs-revision` (commit SHA from `vcs.revision`),
  `mikebom:go-vcs-time` (RFC 3339 commit timestamp from `vcs.time`),
  `mikebom:go-vcs-modified` (dirty-tree boolean from `vcs.modified`,
  preserved as the literal string `"true"` / `"false"` matching Go's
  wire format). The data was already present in BuildInfo's vers_info
  blob; pre-025 the parser read only the first line (Go version) and
  discarded the rest. Dep modules don't carry VCS info — it's a
  main-module concern. Surfaced via the milestone-023 generic
  annotation bag, with zero PackageDbEntry-init churn or generate/
  plumbing changes (the bag's amortization payoff). 4 atomic commits;
  see `specs/025-go-vcs-metadata/spec.md` and catalog rows C27/C28/C29
  in `docs/reference/sbom-format-mapping.md`.
- **Milestone 023 — ELF binary identity + per-component generic
  annotation bag.** Two cohorts in one milestone. (a) ELF identity:
  every Linux-binary scan now surfaces `NT_GNU_BUILD_ID` (the
  canonical Linux binary-identity hash used by `eu-unstrip`,
  `coredumpctl`, `debuginfod`, `*-dbgsym` packaging), `DT_RPATH` /
  `DT_RUNPATH` (embedded library search paths the dynamic loader
  consults — `$ORIGIN` etc. recorded raw), and `.gnu_debuglink`
  (pointer to the stripped-debug sibling file). Three new annotations
  on the file-level binary component: `mikebom:elf-build-id`,
  `mikebom:elf-runpath`, `mikebom:elf-debuglink`. SC-002 is satisfied
  on Linux CI: `/bin/ls` scan emits a non-empty hex build-id (every
  modern distro stamps build-ids by default). (b) Per-component
  annotation bag: `extra_annotations: BTreeMap<String, Value>` on
  `PackageDbEntry` and `ResolvedComponent` provides a generic per-
  component annotation channel that future per-binary-metadata
  milestones (024 Mach-O LC_UUID, 026 version-string library
  expansion, 027 container layer attribution) can populate without
  per-field schema migration. Determinism is preserved by `BTreeMap`
  iteration order. Catalog rows C24/C25/C26.

- **Milestone 010 — SPDX 2.3 output + OpenVEX sidecar + SPDX 3.0.1
  experimental stub.** SPDX 2.3 JSON is now a peer of CycloneDX across
  all 9 supported ecosystems. A single `mikebom sbom scan` invocation
  can emit both formats from one pass over the target; the new
  `--format` flag accepts a comma-separated list and is repeatable,
  and `--output` accepts either a bare path (single-format, legacy)
  or repeated `<fmt>=<path>` (per-format). Every data element that
  CDX emits has a documented target in SPDX — native field where the
  spec has one, `annotations[]` entry with a `mikebom-annotation/v1`
  JSON envelope for the rest; the full map is at
  `docs/reference/sbom-format-mapping.md`. When a scan produces
  advisory data, SPDX 2.3 emission co-emits a companion OpenVEX 0.2.0
  JSON sidecar referenced from the SPDX document via
  `externalDocumentRefs` with a SHA-256 of the sidecar bytes;
  `--output openvex=<path>` retargets it (legal only alongside an
  SPDX format). A third, opt-in format `spdx-3-json-experimental`
  emits a minimal SPDX 3.0.1 JSON-LD document for npm components —
  clearly labeled `[EXPERIMENTAL]` in `--help`, in error messages,
  and in the document's own `CreationInfo.comment`. Typing bare
  `spdx-3-json` offers a did-you-mean hint. No behavior change for
  users who don't request SPDX output: CycloneDX emission is
  byte-identical to the pre-milestone baseline, guarded by pinned
  golden fixtures and a dedicated regression test.
  See `specs/010-spdx-output-support/spec.md` for the full
  requirement list and `docs/reference/sbom-format-mapping.md` for
  the cross-format data-placement contract.
- **Feature 009 refinement — bytecode-presence gating for Maven
  shade-relocation.** Shade-relocation entries are now emitted only when
  an ancestor's bytecode is verifiably present in the enclosing JAR
  (either at its original group path or at a shade-relocated path whose
  leaf matches a distinctive artifact-id fragment). Apache's
  `maven-dependency-plugin` emits `META-INF/DEPENDENCIES` into any JAR
  it is configured on, not only shade fat-jars, so the pre-gating
  emission path reported ancestors as "present in" JARs whose bytecode
  was never relocated there. New unit + integration tests exercise
  every disposition. See `specs/009-maven-shade-deps/spec.md` FR-002b.

### Changed
- **`oci-registry` Cargo feature is now on by default.** Direct
  registry pulls (`mikebom sbom scan --image alpine:3.19`) work
  out of the box on a stock `cargo install mikebom` — matches
  syft / trivy UX without requiring `--features oci-registry`.
  The post-milestone-032 substrate (`oci-spec` types-only +
  workspace `reqwest 0.12` + mikebom-owned thin HTTP client) is
  small enough + durable enough that the milestone-031 default-off
  framing no longer pays for itself. Users embedding mikebom in a
  context that needs a minimal-deps build can opt out via
  `cargo install mikebom --no-default-features`; the local
  `--path <dir>` and `--image <foo.tar>` paths still work in that
  configuration. The dep-audit guardrail
  (`no_c_dependencies_in_oci_registry_feature_tree` regression
  test) continues to enforce zero new C-bound transitive deps in
  the now-default tree.

### Removed
- **`mikebom sbom compare` subcommand** and the `demos/` directory.
  The head-to-head comparison story is now owned by a separate test
  suite outside this repo; keeping the in-tree version invited drift
  between the two. Any workflow that depended on `sbom compare`
  should move to the external suite.

## [0.1.0-alpha.3] — 2026-04-23

### Added
- **Feature 009 US1 — shade-relocation ancestor emission.** When a JAR
  contains `META-INF/DEPENDENCIES`, mikebom emits one nested
  `pkg:maven/...` component per declared ancestor, nested under the
  enclosing JAR's primary coord and tagged with
  `mikebom:shade-relocation = true`. Ancestor licenses are parsed from
  the adjacent `License:` lines. Classifier-bearing coords preserve
  `?classifier=<value>` in the PURL. Self-references are dropped
  (`com.example:outer` cannot shade itself). Commit `cdf29b0`.
- **Feature 008 US3 — Maven target/-dir path heuristic** for
  suppressing `target/`-staged development artifacts from image scans.
  Commit `701ea50` (#14).
- **Feature 008 US2 — cache-ZIP Go component filter.** Emissions from
  Go module-cache ZIPs are cross-checked against the linked binary's
  `runtime/debug.BuildInfo`, suppressing ZIPs that never made it into
  the shipped binary. Commit `db6fbab` (#13).
- **Feature 007 US1 — Fedora sidecar POM reading.** JARs installed by
  `dnf` that have stripped embedded `META-INF/maven/` metadata now
  fall back to `/usr/share/maven-poms/` sidecar POMs (JPP-prefixed
  and plain). Commit `a06b7ff` (#8).
- **Feature 007 US2+US3 — Go test-scope and main-module filters.**
  go.sum and BuildInfo emissions are filtered against non-`_test.go`
  import closure and against the primary module's self-coord. Commit
  `b06eda8` (#10).
- **Feature 007 US4 — Main-Class executable-JAR self-reference
  suppression.** JARs whose `META-INF/MANIFEST.MF` names a `Main-Class`
  no longer re-emit their own primary coord as a generic-binary
  `pkg:generic/...` entry. Commit `89a334f` (#11).
- **Feature 006 US5 — SBOM enrichment (`mikebom sbom enrich`).**
  RFC 6902 JSON Patch applier with per-patch provenance recorded as
  `mikebom:enrichment-patch[N]` properties on the BOM metadata. Replaces
  a previously stubbed bail.
- **Feature 006 US4 — in-toto policy layouts (`mikebom policy init`
  and `mikebom sbom verify --layout`).** Single-step functionary-keyed
  layouts. Multi-step deferred.
- **Feature 006 US3 — real artifact subjects.** Attestation subjects
  are resolved via a 5-stage resolver (operator override → artifact-dir
  walk → suffix match → magic-byte detect for ELF / Mach-O / PE →
  synthetic fallback).
- **Feature 006 US2 — DSSE signing + verification** via `sigstore-rs`
  0.10 (pinned below 0.13 to stay off `aws-lc-rs` per Constitution
  Principle I). `mikebom sbom verify` replaces the never-shipped `sbom
  validate` stub; exit contract: 0 pass / 1 crypto / 2 envelope /
  3 layout.
- **Feature 006 foundation — DSSE verify MVP + witness-v0.1 emission.**
  `mikebom trace run` emits in-toto statements compatible with
  `go-witness` / `sbomit generate`.
- **ClearlyDefined license enrichment.** Post-scan enricher querying
  `api.clearlydefined.io` for `npm`, `cargo`, `gem`, `pypi`, `maven`,
  `golang` components. CD's `licensed.declared` becomes an
  `acknowledgement: "concluded"` license entry. `--offline` disables.
- **Per-ecosystem manifest hashes.** Maven sidecar hashes
  (`.jar.sha512` > `.sha256` > `.sha1`) and PyPI `requirements.txt
  --hash=alg:hex` flags now thread through to `components[].hashes[]`.
- **`metadata.component` carries synthetic `purl` + `cpe`** for sbomqs
  schema validity (`pkg:generic/<name>@<version>` +
  `cpe:2.3:a:mikebom:<name>:<version>:...`).
- **`--include-legacy-rpmdb` flag** (feature 004 US4) enables reading
  legacy Berkeley-DB `/var/lib/rpm/Packages` on pre-RHEL-8 /
  CentOS-7 / Amazon-Linux-2 rootfs. Off by default; also configurable
  via `MIKEBOM_INCLUDE_LEGACY_RPMDB=1`.

### Changed
- **`mikebom trace` reclassified as experimental.** Primary SBOM
  surface is now `mikebom sbom scan`. Trace-mode output format
  (witness-v0.1 + DSSE envelope) remains stable; the capture pipeline
  itself is opt-in, Linux-only (kernel ≥ 5.8), and adds 2–3× wall-clock
  overhead on syscall-heavy builds. Commit `45da74d`.
- **Artifact vs. manifest SBOM scope** is now explicit.
  `sbom scan --image` defaults to artifact scope (on-disk presence
  required). `sbom scan --path` defaults to manifest scope (declared
  deps included). `--include-declared-deps` is the explicit override.
  Gated in three Maven emission paths: deps.dev graph enricher,
  pom.xml direct-dep loop, and the `.m2` BFS cache-miss branch.
- **Dual-identity Maven coords.** JARs at `/usr/share/java/*` owned by
  an OS package-db reader (RPM / dpkg / apk) now emit both identities:
  the `pkg:rpm/...` NEVRA (for distro CVE feeds) and the
  `pkg:maven/<g>/<a>@<v>` GAV (for Maven Central advisories). The
  Maven coord is tagged `mikebom:co-owned-by = rpm` (or equivalent);
  `archive_sha256` is dropped since the archive bytes belong to the
  owning OS component. Pre-fix, the Maven coord was skipped entirely
  under a claim-based heuristic, which cost 53 polyglot GT matches.
- **CycloneDX 1.6 conformance pass.** `evidence.identity` is now an
  array (single-object form deprecated in 1.5→1.6);
  `evidence.identity[].tools` is no longer emitted (the previous
  payload wasn't `tools` by the spec's definition); `mikebom:
  source-connection-ids` + `mikebom:deps-dev-match` now land on the
  component as properties. License shape emits
  `{"license": {"id": "<SPDX-id>"}}` for simple IDs and
  `{"expression": "..."}` for compound expressions.
- **PURL canonicalization.** Qualifiers are now sorted
  lexicographically per purl-spec. `+` is percent-encoded across
  every ecosystem. RPM `epoch=0` is dropped (semantically equivalent
  to no epoch; `rpm -qa` omits it).
- **Compositions emit both `assemblies` and `dependencies`** for each
  `complete` ecosystem record, plus a dep-completeness composition so
  sbomqs's `comp_with_dependencies` credits the primary component.
- **Primary-dependency fallback.** When the scanned project's root
  entry was filtered out (npm `path_key == ""`, cargo `source = None`)
  mikebom now synthesizes edges from the primary metadata.component to
  every orphan root. Without this, sbomqs reported "no dependency
  graph present" even when transitives were populated.
- **OS-release reader** prefers `<rootfs>/etc/os-release`, falls back
  to `<rootfs>/usr/lib/os-release` — fixes Ubuntu images where
  `/etc/os-release` is a relative symlink that dangles after
  tar-extraction.
- **Binary-scanner version-string scanner gated on
  `skip_file_level_and_linkage`** to suppress claimed-binary
  self-identification (curl reporting libcurl from `/usr/bin/curl`).
  Trade-off: static-library version detection inside claimed binaries
  is lost; see `docs/design-notes.md`.

### Fixed
- **Pre-PR verification gate** (Constitution v1.2.1). CI runs
  `cargo +stable clippy --workspace --all-targets` and
  `cargo +stable test --workspace`; skipping either locally before
  opening a PR now yields a reject cycle. Commit `6ec1cf3` (#9).
- **Cross-source deduplication + scan-target filter.** Resolves
  duplicate emissions when the same coord surfaces via multiple
  readers (e.g. Maven JAR walker + `.m2` cache + deps.dev). Commit
  `5c98ed2` (#3).
- **Go `go.sum` vs. BuildInfo divergence.** `go.sum` emissions are
  filtered against the companion binary's BuildInfo so dev-only
  transitives don't surface as runtime components. Commit `5b38b98`
  (#7).
- **Go component name alignment** across the source-tree and binary
  emission paths. Commit `ffa7d9f` (#6).
- **Maven version-aware artifact-presence gate** (M6). Commit
  `b4a9041` (#5).
- **Fat-jar heuristic gated on `co_owned_by.is_none()`** to avoid
  double-reporting. Commit `cb7f14e` (#4).
- **ELF-note ghost emissions.** Previously unconditional — a claimed
  Fedora binary emitted both `pkg:rpm/fedora/<subpackage>` (from
  rpmdb) and a ghost `pkg:rpm/rpm/<source-package>` (from the ELF
  `.note.package` section). Now gated on
  `skip_file_level_and_linkage`; unclaimed binaries respect a
  precedence `note.distro > os-release ID > hardcoded default`.
  Commit `3e5ab91`.
- **Cargo workspace-root false positive.** Commit `3e5ab91`.
- **`declared-not-cached` components dropped from `components[]` by
  default.** They remain in the dependency graph as references but are
  no longer materialized as standalone components. Commit `7688ddb`.
- **sbom-conformance findings + CDX 1.6 evidence serialization.**
  Commit `3cd55e3`.

## [0.1.0-alpha.2] and earlier

Earlier alpha milestones landed as a bootstrap commit
(`b0f31c1 feat: bootstrap mikebom + milestones 001-005`) and ship the
foundational work below. CHANGELOG entries below are a roll-up, not a
per-release breakdown.

### Feature 005 — PURL & scope alignment
- Distro qualifier shape standardized as `distro=<ID>-<VERSION_ID>`
  (matches packageurl-python reference tests); codename-required
  claims dropped from docs + tests.
- npm internals scoping: image scans include
  `node_modules/npm/node_modules/**`; path scans exclude.
  Always-on; not user-gated.
- RPM version-string normalization for canonical round-trip.

### Feature 004 — RPM binary SBOMs
- Standalone `.rpm` file scanning (feature 004 US1/US2).
- Generic binary reader for ELF / Mach-O / PE: linkage
  (`DT_NEEDED`, `LC_LOAD_DYLIB`, PE `IMPORT`) plus embedded
  version-string scanning for a curated 7-library list
  (OpenSSL / BoringSSL / zlib / SQLite / curl / PCRE / PCRE2).
- Legacy Berkeley-DB rpmdb parsing gated behind
  `--include-legacy-rpmdb` (feature 004 US4). Default-off.

### Feature 003 — multi-ecosystem expansion
- Go source + binary readers (`go.mod`, `go.sum`, module cache,
  `runtime/debug.BuildInfo` inline format).
- RPM rpmdb.sqlite pure-Rust reader (page/record/schema).
- Maven pom.xml parser with `<properties>` + `<dependencyManagement>`
  + BOM import resolution (`EffectivePom`, cycle-guarded memo).
- Cargo v3/v4 lockfile parser; v1/v2 refused.
- Gem `Gemfile.lock` indent-6 parser; `specifications/*.gemspec`
  walker catches Ruby stdlib/default gems invisible to Gemfile.lock.

### Feature 002 — Python + npm
- Python venv `dist-info/METADATA` reader; `poetry.lock`,
  `Pipfile.lock`, `requirements.txt` support with dev/prod
  distinction.
- npm `package-lock.json` v2/v3 + `pnpm-lock.yaml` + `node_modules/`
  tree walker. v1 lockfiles refused.

### Feature 001 — build-trace pipeline (experimental)
- eBPF capture of syscall + network events during a build. Requires
  CAP_BPF + CAP_PERFMON and Linux kernel ≥ 5.8. Produces in-toto
  attestations bound to the build event.

---

[Unreleased]: https://github.com/mlieberman85/mikebom/compare/v0.1.0-alpha.3...HEAD
[0.1.0-alpha.3]: https://github.com/mlieberman85/mikebom/releases/tag/v0.1.0-alpha.3
