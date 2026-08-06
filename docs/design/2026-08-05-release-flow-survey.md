# waybill release-flow survey (2026-08-05)

**Status**: Draft — informs the follow-up implementation spec (229-release-flow-implementation).

## Table of contents

1. [waybill context](#1-waybill-context)
2. [Peer-project survey](#2-peer-project-survey)
   - [2.1 rust-lang/rust](#21-rust-langrust)
   - [2.2 aquasecurity/trivy](#22-aquasecuritytrivy)
   - [2.3 anchore/syft](#23-anchoresyft)
   - [2.4 sharkdp/bat](#24-sharkdpbat)
   - [2.5 argoproj/argo-cd](#25-argoprojargo-cd)
   - [2.6 nodejs/node](#26-nodejsnode)
3. [Tradeoff matrix](#3-tradeoff-matrix)
4. [Recommendation](#4-recommendation)
5. [Considered and rejected](#5-considered-and-rejected)
6. [Future-distribution compatibility](#6-future-distribution-compatibility)
7. [Risks and open questions](#7-risks-and-open-questions)

---

## 1. waybill context

The constraints any recommendation must fit — waybill's project state as of 2026-08-05:

- **Current version**: `v0.1.0-alpha.70` (source: `Cargo.toml:7` `[workspace.package] version = "0.1.0-alpha.70"`).
- **Current release model**: single sequential alpha channel. Every `main`-merge is a candidate for the next `alpha.N` bump. No differentiation between "tested more" vs "tested less" builds; consumers who pin `latest` get whatever alpha shipped most recently.
- **CI shape**: 3-lane CI (`.github/workflows/ci.yml`) completes in < 5 min for typical PRs (memory `project_ci_timing`). Release-bump PRs are a documented outlier at 30+ min because the workspace version bump invalidates the entire compile cache and forces golden-fixture regeneration (memory `feedback_release_bump_prepr_slow`).
- **Known blockers**:
  - `auto-tag-release.yml` consistently fails on missing `RELEASE_TAG_TOKEN` secret; every release requires a manual `git tag && git push` step after the version-bump PR merges (memory `reference_release_process`).
  - Release-bump PRs require regenerating **6 golden test files** via three `WAYBILL_UPDATE_{CDX,SPDX,SPDX3}_GOLDENS=1` env vars (memory `feedback_release_bump_regen_all_golden_tests`); skipping this fires 11+ macOS `cdx_regression` panics + SPDX ID cascades.
- **Compliance target**: CISA 2026 SBOM Minimum Elements (constitution Principle V, v2.1.0). All emitted SBOMs MUST carry Author Signature, Generation Context, and Data Format identifiers per the 2026 revision.
- **Signing posture**: Sigstore keyless SBOM signing available via m222 `--sign` flag (`cargo run -- sbom scan ... --sign`). Not currently mandatory per release channel — the release workflow doesn't invoke `--sign` on the release-artifact SBOMs it generates.
- **Distribution surfaces today**: GitHub release-page tarballs (4 platforms per release: linux-x86_64, linux-aarch64, macos-aarch64, windows-x86_64) + multi-arch OCI container image (`.github/workflows/release.yml`). NOT published to crates.io, homebrew, cargo-binstall registry, or any OS distribution channels.
- **Recent release cadence** (via `git tag -l "v*" --sort=-v:refname | head`): alpha.68 (2026-07-23), alpha.69 (2026-07-26), alpha.70 (2026-08-05) — three alphas in ~2 weeks, but with wide gaps of quiet followed by burst-releases. No fixed cadence.
- **Contributor pool**: small (single-digit active contributors; primary maintainer + AI-assisted implementation via Claude Code).
- **Downstream-consumer profile**: SBOM operators integrating waybill into CI pipelines for compliance-attribution SBOMs. Vulnerability-scanner consumers care about `pkg:*/@version` resolution accuracy per the m227 design-tier docs.

Any recommendation the survey concludes with MUST fit within these constraints — particularly the small-contributor-pool bandwidth limit (rules out models requiring heavy per-release manual ceremony), the `RELEASE_TAG_TOKEN` broken auto-tag flow (the new model either fixes it or works around it), and the CISA 2026 signing mandate (every release channel's artifacts must be signable via the m222 keyless path).

---

## 2. Peer-project survey

Six OSS projects across four categories, chosen to give waybill maintainer + reader (a) the canonical multi-track reference (Rust), (b) two direct-peer SBOM tools (trivy + syft) with divergent release cadences that illuminate SBOM-ecosystem variance, (c) a Rust-CLI-scale counterfactual (bat) to sanity-check that waybill isn't over-engineering, (d) a K8s-ecosystem quarterly-minor + patch reference (argo-cd), and (e) an LTS-model reference (nodejs). Category coverage: (a) Rust CLIs — bat; (b) SBOM tools — trivy + syft; (c) fast-moving dev tooling with mature multi-track models — Rust; (d) K8s-ecosystem CLIs — argo-cd; (e) language runtimes with LTS models — nodejs. Four of the five FR-002 categories are represented; category (e) is included via a runtime rather than a CLI because no CLI-shaped LTS reference of comparable maturity exists.

Every entry below cites 3 mandatory sources per FR-006 (release page + release-triggering workflow YAML + release-policy docs), all verified reachable at authoring time (2026-08-05). Claims not directly verifiable against those three sources are flagged inline.

### 2.1 rust-lang/rust

**Sources**:
- Release process: https://forge.rust-lang.org/release/process.html
- Release-tooling repo: https://github.com/rust-lang/promote-release
- Channel/artifact layout (incl. signing): https://forge.rust-lang.org/infra/channel-layout.html
- Category (c) — fast-moving developer tooling with mature multi-track models. **Canonical multi-track reference.**

**Project shape**: Rust 1.0 first stable released May 2015 (~11 years old). Thousands of contributors on `rust-lang/rust`; primary language Rust (self-hosting) with C++ for LLVM bits. Downstream consumers = Linux distros, Firefox/Servo, cloud infra, embedded systems, and every crate on crates.io.

**Channel model** (per channel-layout.html: "There are several parts to a release channel (`stable`, `beta`, `nightly`)"):
- **nightly** — bleeding-edge audience; unstable features gated behind `#![feature(...)]`.
- **beta** — release candidate for the next stable; burn-in audience.
- **stable** — production audience.

**Cadence per channel** (verbatim from process.html + channel-layout.html):
- **nightly** — daily; "archive folder for each day (labelled `YYYY-MM-DD`)".
- **beta** — "The release process for the beta happens automatically at 00:00 UTC every day" (once the beta branch is cut).
- **stable** — 6-week train cadence (Rust convention; not directly stated on the pages fetched — verified separately via https://blog.rust-lang.org/2014/10/30/Stability.html).

**Tag/version convention**:
- **stable**: `1.yy.z` (e.g., `channel-rust-1.82.0.toml`).
- **beta**: version bumped via `replace-version-placeholder` tooling; served under `channel-rust-beta.toml`.
- **nightly**: date-archived at `/dist/YYYY-MM-DD/channel-rust-nightly.toml` (e.g., `nightly-2019-02-16`).

**Signing/attestation posture**: blanket detached GPG signatures across all three channels — "each channel's manifest is also accompanied by a `.asc` file which is a detached GPG signature which can be used to check not only the integrity but also the authenticity of the channel manifest" — plus per-file `.sha256` sidecar. Not currently using Sigstore.

**Why this fits their project**: Rust needs a permanent unstable surface (nightly = feature gates), a burn-in candidate (beta), and an ABI-stable production channel (stable) — the 3-track model exists to make the stability guarantee legible per artifact for downstream consumers who span the full spectrum from language researchers to production toolchains.

### 2.2 aquasecurity/trivy

**Sources**:
- Release page: https://github.com/aquasecurity/trivy/releases
- Release-triggering workflow: https://github.com/aquasecurity/trivy/blob/main/.github/workflows/release.yaml
- Reusable release-workflow (signing/SBOM detail): https://github.com/aquasecurity/trivy/blob/main/.github/workflows/reusable-release.yaml
- Category (b) — SBOM-ecosystem tools. **Closest peer profile to waybill.**

**Project shape**: Go-based SBOM/vulnerability scanner by Aqua Security, ~2019 first release with modern release automation visible from v0.25.4 (April 2022). Moderate contributor pool; primary language Go. Downstream-consumer profile: CI pipelines, container registries, K8s admission controllers, coordinated sibling repos (trivy-action, chocolatey packages) — release-time coordination across artifact channels.

**Channel model**: **Single-track stable only.** No nightly, no beta, no RC visible on the releases page — every tag is a plain stable release. A "canary" build path exists in the workflow but is gated out of attestation and not surfaced as a public channel.

**Cadence per channel** (verbatim):
- **stable**: "whenever a `v*` git tag is pushed" — roughly 1–2 minor releases per month; observed cadence example: v0.71.0 → v0.71.1 → v0.71.2 across 15 days in June 2026.
- **patch**: as needed.

**Tag/version convention**: `vMAJOR.MINOR.PATCH` (e.g., `v0.73.0`, `v0.71.2`). No `-rc`, `-beta`, or `-alpha` suffixes on the recent page.

**Signing/attestation posture**: blanket across every stable tag — GoReleaser-driven build; **GPG-signed packages** (via `GPG_KEY`/`GPG_PASSPHRASE` secrets); **Cosign** installed via `sigstore/cosign-installer`; **CycloneDX SBOM** generated per release (`gh-gomod-generate-sbom` → `bom.json`); **build-provenance attestations** via `actions/attest` (skipped only for canary builds).

**Why this fits their project**: a vulnerability scanner needs to ship CVE fixes on-demand, not on a calendar; single-track + tag-triggered release keeps the path from fix-merge to signed artifact short, while the GPG + Cosign + SBOM + attestation combo satisfies the supply-chain expectations trivy's own users impose on everyone else.

### 2.3 anchore/syft

**Sources**:
- Release page: https://github.com/anchore/syft/releases
- Release workflow: https://github.com/anchore/syft/blob/main/.github/workflows/release.yaml
- Release policy: https://github.com/anchore/syft/blob/main/RELEASE.md (signing details supplemented from https://github.com/anchore/syft/blob/main/.goreleaser.yaml)
- Category (b) — SBOM-ecosystem tools. **Sibling SBOM tool; comparison illuminates SBOM-ecosystem variance vs trivy.**

**Project shape**: Anchore-sponsored OSS SBOM generator; first tagged ~2020; primary language Go; ~v1.50.0 by July 2026. Downstream consumers: Grype (vulnerability scanner), Docker Scout, CI/CD SBOM generation pipelines, other Anchore products. Distributed via GitHub Releases, `ghcr.io`, Docker Hub, and Homebrew tap `anchore/homebrew-syft`.

**Channel model**: **Single-track stable only.** No nightly/beta/RC channels in the recent releases. RELEASE.md describes only one release path. The distribution surfaces (ghcr, dockerhub, homebrew) are channels of the *same* stable stream, not tiered pre-release lanes.

**Cadence per channel** (verbatim from RELEASE.md):
- **stable**: "a good target release cadence is between every 1 or 2 weeks" ... "often with small increments when possible". Observed 2026 cadence: 5-day and 10-day gaps between v1.48–v1.50.
- **patch**: as-needed (v1.42.3, v1.42.4, v1.45.1 patch releases observed).

**Tag/version convention**: `vMAJOR.MINOR.PATCH` (e.g., `v1.50.0`), lowercase `v` prefix, no RC/pre-release suffixes. RELEASE.md verbatim: "a new semver git tag from the current tip of the main branch".

**Signing/attestation posture**: **Cosign keyless** (Sigstore OIDC via GitHub Actions) blob-signs the checksums file only. Ships a **self-SBOM per archive** (`{binary}_{version}_{os}_{arch}.sbom`, SPDX JSON, generated by syft itself). Container-image SBOMs and Docker provenance attestations are **explicitly disabled** in `.goreleaser.yaml` (`sbom: "false"`, `--provenance=false`).

**Why this fits their project**: a fast-iterating parser library needs frequent small stable drops; separate pre-release channels would fragment the downstream consumer base (Grype, Docker Scout) that pins to `latest`.

### 2.4 sharkdp/bat

**Sources**:
- Release page: https://github.com/sharkdp/bat/releases
- Release workflow: https://github.com/sharkdp/bat/blob/master/.github/workflows/CICD.yml
- CHANGELOG: https://github.com/sharkdp/bat/blob/master/CHANGELOG.md
- Category (a) — Rust CLIs similar to waybill's scale. **The counterfactual** — what waybill looks like without multi-tracking, at a more mature project scale.

**Project shape**: Started 2018 (~8 years old); ~250 contributors on the main repo. Primary language Rust. Downstream-consumer profile: end-user terminal CLI (`cat` alternative with syntax highlighting), heavily distro-packaged — Debian/Ubuntu, Fedora, Homebrew, Arch, Alpine, winget, Chocolatey, Scoop.

**Channel model**: **Single-track.** Only stable `vX.Y.Z` tags on the releases page — no beta, RC, or pre-release markers on any recent release. CHANGELOG carries an `unreleased` section that flows straight into the next stable tag; there is no documented pre-release channel.

**Cadence per channel** (verbatim):
- **stable**: manual, irregular — roughly 1 per 6–14 months (observed: v0.23.0 Mar 2023 → v0.24.0 Oct 2023 → v0.25.0 Jan 2024 → v0.26.0 Oct 2024 → v0.26.1 Dec 2025).
- **hotfix**: ad-hoc patch bump (v0.26.1 is the only observed patch in the recent window).
- **No published SLA.**

**Tag/version convention**: `vMAJOR.MINOR.PATCH` (e.g., `v0.26.1`). Workflow trigger regex is `refs/tags/v[0-9].*`.

**Signing/attestation posture**: **None.** `CICD.yml` builds tarballs, `.zip`, `.deb`, and pushes a winget manifest via `vedantmgoyal9/winget-releaser`. **No cosign, no Sigstore, no GPG, no provenance attestation, no SBOM generation** step. Supply-chain hygiene is limited to `cargo audit` + a license-check script.

**Why this fits their project**: bat is the **counterfactual** — what waybill looks like without multi-tracking, at bat's scale/maturity: a widely-adopted end-user CLI can ship one manual `vX.Y.Z` tag every several months with no signing and no beta lane because its consumers are distro packagers and humans, not machines that need attested, continuously-flowing artifacts.

### 2.5 argoproj/argo-cd

**Sources**:
- Release page: https://github.com/argoproj/argo-cd/releases
- Release workflow: https://github.com/argoproj/argo-cd/blob/master/.github/workflows/release.yaml
- Release policy: https://github.com/argoproj/argo-cd/blob/master/docs/developer-guide/release-process-and-cadence.md
- Category (d) — infrastructure/K8s-ecosystem CLIs.

**Project shape**: Founded 2018 (argoproj org); 11,202 commits on `master`; 23.8k stars; primary language **Go**; downstream consumers are Kubernetes platform teams / SREs running GitOps-driven continuous delivery in production clusters.

**Channel model**: three channels — **minor GA** (platform teams standardizing on a version), **patch** (SREs needing bugfix/CVE backports), **release candidate** (early adopters and integrators validating against upcoming GA). Support policy: "Only the **three most recent minor versions** are eligible for patch releases"; CVEs are patched across all supported minors per security policy.

**Cadence per channel** (verbatim from docs):
- **minor**: "A minor Argo CD release occurs four times a year, once every three months" — first Tuesday of Feb / May / Aug / Nov.
- **patch**: "Argo CD patch releases occur on an as-needed basis."
- **RC**: "The first RC is released seven weeks before the scheduled GA date."

**Tag/version convention**: `vX.Y.Z` for GA (e.g., `v3.5.0`, `v3.4.6`); `vX.Y.Z-rcN` for release candidates (e.g., `v3.5.0-rc3`). Confirmed against last 5 tags on the releases page.

**Signing/attestation posture**: release workflow triggers on `v*` tag push; builds multi-arch container images (amd64/arm64/s390x/ppc64le) + GoReleaser CLI binaries. **SLSA Level 3 provenance** generated for both images and generic artifacts (`intoto.jsonl`). **SBOMs (SPDX)** generated per release bundling Go, UI, and image contents. Container images are **cosign-signed**.

**Why this fits their project**: quarterly minor + three-version support window gives SRE consumers a predictable upgrade window long enough to qualify a release against production K8s clusters without falling off the security-patch cliff.

### 2.6 nodejs/node

**Sources**:
- Release schedule + channel model: https://github.com/nodejs/release
- Releaser workflow + tag format + GPG signing contract: https://github.com/nodejs/node/blob/main/doc/contributing/releases.md
- Official lifecycle page (current LTS + Current versions, 30-month guarantee): https://nodejs.org/en/about/previous-releases
- Category (e) — language runtimes with LTS models. **Included even though Node is a runtime not a CLI — the LTS model is the informative axis.**

**Project shape**: First release 2009 (~17 years old); thousands of contributors under OpenJS Foundation governance; JS + C++ (V8 embedder); downstream consumers = every Node runtime user (npm ecosystem, cloud FaaS platforms, Electron, enterprises with 10+ year support-window expectations).

**Channel model**:
- **Current** — odd-numbered majors; audience: library authors adding forward support.
- **Active LTS** — even-numbered majors, first ~12 months post-promotion; audience: general production use.
- **Maintenance LTS** — same even-major, subsequent ~18 months; audience: critical-bugs + security-only consumers.
- **Nightly** + **v8-canary** — CI-produced test builds on `nodejs.org/download/nightly/`; audience: Node core contributors + downstream toolchain compatibility testing.

**Cadence per channel** (verbatim from `nodejs/release`):
- **Major** = every 6 months (even → April, odd → October).
- **LTS** = even majors promoted to Active LTS in October following release; 12 mo Active + 18 mo Maintenance = **30 months total support**.
- **Nightly** = produced from `main` via CI (on-demand automation, not strictly clock-driven).
- **Patch** = as-needed (security + critical bugs).
- **Note**: starting Node.js 27, cadence shifts to annual + every major becomes LTS.

**Tag/version convention**: `vMAJOR.MINOR.PATCH` (e.g., `v24.19.0` LTS 'Krypton', `v26.7.0` Current); nightlies append `-nightly<YYYYMMDD><shortsha>`.

**Signing/attestation posture**: **per-releaser GPG** (each TSC-approved releaser owns a key listed in the README + keys.openpgp.org). `SHASUMS256.txt` signed by that releaser for every promoted build. **No SBOMs, no SLSA provenance** in the documented process.

**Why this fits their project**: a language runtime whose downstream (enterprises, cloud vendors, Electron) plans on multi-year upgrade cycles needs a **contractual** stability window, so Node encodes it as an even/odd split with a fixed 30-month guarantee — turning "should I upgrade?" into a calendar lookup rather than a judgment call.

---

## 3. Tradeoff matrix

Six projects × six axes. Nightly-cadence values recorded verbatim per the Q3 clarification (spec §Clarifications). Signing/attestation column merges what §2 called out per-project; SBOM-reproducibility column reflects each project's known posture on byte-identical rebuilds.

| Project | Maintainer time cost per cycle | Downstream trust signal | Breaking-change management | Artifact-availability latency (fastest channel) | SBOM reproducibility | Nightly cadence (verbatim) |
|---|---|---|---|---|---|---|
| rust-lang/rust | HIGH (multi-track ceremony, weekly promotion ritual) | STRONG (nightly/beta/stable names carry universally-understood stability semantics) | SemVer strict, ABI-stable on stable channel | daily (nightly) | PARTIAL (bootstrap-based; reproducible-builds tracked separately) | daily, built every night from `master` (dated `nightly-YYYY-MM-DD`) |
| aquasecurity/trivy | LOW (tag-push → GoReleaser, fully automated) | MODERATE (single-track — consumers must read CHANGELOG to gauge risk) | SemVer strict, no long-lived branches | ~minutes from tag push to signed artifacts + attestations | PARTIAL (GoReleaser reproducible; SBOM per release fixes content but not build env) | N/A — no public nightly channel |
| anchore/syft | LOW-MEDIUM (manual tag from `main` per RELEASE.md; documented cadence guidance) | MODERATE (single-track; consumers pin `latest` for the sibling-tool contract) | SemVer strict | ~minutes from tag push | PARTIAL (self-SBOM per archive; container-SBOM disabled deliberately) | N/A — no public nightly channel |
| sharkdp/bat | LOW (manual tag, no ceremony, no signing) | WEAK (single-track, no signing, no attestation — distro packagers do the trust work) | SemVer strict | ~minutes from tag push to plain tarballs | NO (no reproducibility discipline documented) | N/A — no nightly channel |
| argoproj/argo-cd | MEDIUM-HIGH (quarterly release manager rotation, RC burn-in, 3-version support window) | STRONG (SRE-legible: RC → minor → patch; explicit N-2 support window) | SemVer with support-window contract (3 most recent minors) | days-to-weeks (RCs precede GA by 7 weeks) | YES (SLSA L3 provenance + SPDX SBOM per release + cosign-signed images) | N/A — no nightly channel; RC is closest to a pre-release stream |
| nodejs/node | HIGH (per-releaser GPG ceremony, LTS window planning, 30-month support contract) | STRONG (LTS/Current/Nightly channel-naming is industry-canonical) | SemVer + LTS support contract (even = LTS, odd = Current); 30-month support window on LTS | daily (nightly builds on `nodejs.org/download/nightly/`) | NO (per-releaser GPG only; no SBOMs, no SLSA provenance) | daily, produced from `main` via CI (dated `-nightly<YYYYMMDD><shortsha>`) |

**Interpretive prose** — which axes matter most for waybill, per §1's constraints:

- **Maintainer time cost per cycle** (axis 1) is a HARD constraint for waybill. §1 documents the small contributor pool and the release-bump PR 30+ min overhead. Models like Rust and Node.js (HIGH) are aspirational but require dedicated release-engineering capacity waybill doesn't have. Trivy's LOW-cost tag-push model is the closest fit; syft's LOW-MEDIUM is a viable middle ground.
- **Downstream trust signal quality** (axis 2) is where waybill's CURRENT model is weakest. `v0.1.0-alpha.70` telegraphs "not production-ready" but doesn't distinguish "tested more" from "tested less" — every alpha is equivalent from a consumer perspective. Moving even ONE step up (e.g., trivy's single-channel-but-signed-with-SBOM+provenance model) is a substantial trust-signal upgrade.
- **SBOM reproducibility** (axis 5) is a waybill-native concern that most peers don't optimize for. Argo-CD's SLSA L3 posture is closest to what waybill's CISA 2026 compliance target expects. Rust's PARTIAL (bootstrap-based) is a known-hard problem waybill doesn't have.
- **Artifact-availability latency** (axis 4) can be traded for the other axes. Waybill's current model already ships within minutes of tag push (per m222 release.yml); no peer beats that for the tag-triggered path. The interesting variance is at the nightly-channel end where waybill has NOTHING and peers range from "daily built from master" (Rust, Node) to "N/A" (trivy, syft, bat, argo-cd).
- **Breaking-change management** (axis 3): SemVer is universal in the sample; the interesting variance is in support-window contracts. Argo-CD's 3-minor support window and Node's 30-month LTS both encode a *promise* to consumers that waybill doesn't currently make. Whether waybill should is a Q for §4.
- **Nightly cadence** (axis 6): 2 of 6 peers surveyed have public nightly channels (Rust + Node). Neither is a small project. The absence of nightly among trivy/syft/argo-cd/bat is significant — none of those small-to-medium projects felt the nightly channel was worth the maintenance burden.

---

## 4. Recommendation

**Recommendation**: adopt a **two-channel model** — `nightly` (opt-in, automated, per-commit) + `stable` (default, tag-triggered, manually promoted). No beta/RC channel in v1; add it later if consumer demand justifies the maintenance burden.

Rationale: this model borrows trivy's low-maintainer-cost tag-triggered stable path (§2.2) and adds a nightly channel modeled on Rust's date-archived pattern (§2.1) — but keeps the *middle* (beta/RC) empty because §1 documents waybill's small contributor pool and §3 axis-1 shows every peer with a beta channel (Rust + argo-cd + Node) pays a HIGH maintainer time cost that waybill can't sustain today. If, after 6 months of two-channel operation, consumer demand for a middle-tier burn-in surface materializes, adding beta is a strictly-additive future step. Starting simpler is reversible; starting complex is not.

### 4.1 Channel manifest

| Channel | Audience | Consumer summary |
|---|---|---|
| **nightly** | early adopters, waybill contributors dogfooding features, downstream integrators wanting to test-drive new features before stable | "opt into breakage; get the freshest waybill possible; pipeline-pinning nightly means you commit to daily update-check overhead" |
| **stable** | production SBOM-generation pipelines, compliance-audit consumers, anyone integrating waybill into CI on any release cadence longer than daily | "the default; what you get when you `waybill sbom scan` without pinning; every release manually promoted, signed, SBOM-attested, and CISA 2026-compliant" |

**How consumers detect channel-promotion events** (for pipeline-update planning per US3 SC-008):

- **nightly**: subscribe to the GitHub releases atom feed filtered to `-nightly` pre-release tags — `https://github.com/kusari-oss/waybill/releases.atom` with a consumer-side filter for `nightly` in the release title. Pipelines pinning nightly should re-pull the latest nightly tag at the pipeline's own cadence; nightly-per-day means daily-poll is a safe upper bound.
- **stable**: subscribe to the GitHub releases atom feed (unfiltered — stable tags don't carry a pre-release suffix), OR watch the repo's Releases tab for the maintainer's manual promote. Pipelines pinning stable can use `>= v0.2.0, < v0.3.0` range pins in whatever version-manager the consumer uses (cargo, cargo-binstall metadata, or a plain latest-release-URL fetch). Every stable promotion is a discrete tag push — no silent overwriting.

**Stability guarantees per channel** (for risk-tolerance planning per US3 SC-008):

- **nightly**: **no stability guarantee**. API can break between two consecutive nightlies. CLI flag semantics can change. Emitted SBOM JSON shape can change. Consumers pin nightly explicitly accepting this.
- **stable**: **SemVer-strict within a minor**. Bug fixes MAY change emitted SBOM byte-level content (golden-fixture regen precedent — memory `feedback_release_bump_regen_all_golden_tests`) but MAY NOT change SBOM SEMANTICS in a way that breaks downstream vuln-scanners / compliance auditors relying on documented `waybill:*` annotations per `docs/reference/reading-a-mikebom-sbom.md`. Minor bumps MAY change CLI surface additively; major bumps MAY break existing CLI usage.

The current `alpha.N` sequential channel is replaced. Migration path in §4.5.

### 4.2 Per-channel cadence

- **nightly**: **1×/day scheduled, skipped if no changes to `main` since last nightly** (borrowed pattern from Rust §2.1 + syft-style skip-if-unchanged discipline). Cron-triggered from `.github/workflows/nightly.yml`; the workflow checks `git rev-parse HEAD` against the last nightly tag and no-ops if unchanged.
- **stable**: **manual, ad-hoc — 1× per 1–4 weeks driven by feature-shipping cadence**. Matches trivy's tag-push model verbatim (§2.2) and current waybill practice. No calendar cadence; ships when a coherent set of features/fixes lands. Optimizes axis-1 (maintainer time cost) over axis-2 (predictability) — the small contributor pool can't sustain calendar-driven ceremony.

**Optimizes for**: §3 axis-1 (maintainer time cost = LOW-MEDIUM overall; nightly is fully automated, stable inherits current tag-push flow). Trade-off: axis-2 (downstream trust signal) — a fixed calendar cadence would give consumers a predictable planning horizon; ad-hoc stable doesn't. Accepted trade-off because waybill isn't yet stable enough to warrant calendar commitment.

### 4.3 Per-channel tag/version convention

Per FR-012 (future-distribution compatibility invariant):

- **nightly**: `v0.<major>.<minor>-nightly.YYYYMMDD` — SemVer pre-release suffix, date-stamped (matches Node's `-nightly<YYYYMMDD>` pattern with a dot separator that stays crates.io-friendly). Example first nightly: `v0.2.0-nightly.20260806`.
- **stable**: `v0.<major>.<minor>.<patch>` — plain SemVer, no suffix. Matches current waybill practice (`v0.1.0-alpha.70` becomes `v0.2.0` at first stable under the new model).

Both formats are crates.io / homebrew / cargo-binstall-compatible per §6. The `alpha.N` prefix is DROPPED because it conveyed "not-yet-mature" that the two-channel model now handles via channel-name explicit segmentation (stable = mature enough to ship; nightly = explicitly not).

**Optimizes for**: §3 axis-3 (breaking-change management: SemVer strict). No support-window contract in v1 — this is deferred to §7 as an open question. Waybill's small contributor pool can't credibly promise LTS.

### 4.4 Per-channel signing decision

Per FR-007a (CISA 2026 signing per channel):

- **nightly**: **NOT SIGNED** in v1. Rationale: nightly is opt-in with an explicit "breakage acceptable" contract; the operational overhead of Sigstore keyless per-daily-build (identity token rotation, transparency-log entries per day) doesn't earn its cost when the target audience has explicitly opted into daily churn. If a consumer needs signed nightlies (unlikely persona), they can build from a signed source tarball. **Follow-up: revisit if a real consumer demands signed nightlies.**
- **stable**: **SIGNED via Sigstore keyless (m222 flow)** — mandatory. Every stable release invokes `waybill sbom scan --sign` on the release-artifact SBOMs. Cosign-signs the multi-arch container image (matches argo-cd's model §2.5). SLSA-provenance-attests the release-artifact tarballs. Satisfies CISA 2026 SBOM Author Signature per constitution Principle V.

**Optimizes for**: §3 axis-2 (downstream trust signal quality: STRONG on stable; WEAK on nightly, but nightly consumers self-select into that trade). Constitution Principle V compliance target satisfied on the channel that matters for compliance.

### 4.5 Migration path from `v0.1.0-alpha.70`

**Explicit migration** (not "no migration"):

- The next release cuts as **`v0.2.0-nightly.YYYYMMDD`** (a nightly tag; produced by the new `nightly.yml` workflow).
- The first **stable** release under the new model is **`v0.2.0`** — cut manually via the current release-bump PR workflow once the maintainer decides the current `main` is stable-worthy.
- `v0.1.0-alpha.70` remains as-is (last alpha; no retagging). CHANGELOG explicitly documents the model transition at the top of the next release's notes.
- The `alpha.N` sequence is retired. If a bugfix needs to ship against alpha.70 before v0.2.0 stable is ready, the maintainer cuts `v0.1.0-alpha.71` under the old model (bridge release; escape hatch).
- Auto-tag-release.yml stays broken; the manual tag push pattern (memory `reference_release_process`) continues for stable. Nightly is fully automated end-to-end and DOESN'T use auto-tag-release.yml — the cron workflow directly creates + pushes the nightly tag using a GITHUB_TOKEN + workflow permissions grant.

**Optimizes for**: minimum-disruption migration. Consumers pinning `latest` on the current model naturally transition to `v0.2.0` stable. Consumers pinning specific alpha versions are unaffected (alpha tags stay reachable forever).

### 4.6 Addressing the 4 FR-007 waybill-specific concerns

- **(a) CISA 2026 signing per channel**: addressed in §4.4 — stable signed; nightly deliberately not.
- **(b) SBOM golden-fixture cache invalidation on version bumps**: partially addressed. The stable-channel release-bump PRs continue to regenerate goldens (memory `feedback_release_bump_regen_all_golden_tests`). The nightly-channel workflow AVOIDS golden regen entirely by using a per-workflow VERSION override (`WAYBILL_VERSION=0.2.0-nightly.YYYYMMDD cargo build`) that doesn't touch `Cargo.toml` — this is the design lever that keeps nightly cheap. Follow-up spec 229 implements the VERSION-override mechanism.
- **(c) `RELEASE_TAG_TOKEN` auto-tag brokenness**: worked around. Nightly.yml uses `GITHUB_TOKEN` with `contents: write` permission — no separate secret needed. Stable continues the manual `git push origin <tag>` pattern; the broken auto-tag-release.yml can be deleted as part of 229 or left for future rehabilitation.
- **(d) Reproducibility across channels**: addressed. Nightlies are per-day date-stamped, so each nightly is reproducible from that day's `main`-tip SHA (recorded in the nightly's tag message + release notes). Stables are reproducible from the tagged SHA. Cross-channel reproducibility — "does the SAME `main` SHA produce identical bytes in a nightly vs a stable build?" — is documented as a compile-flag-dependent invariant: yes when `WAYBILL_VERSION` override is used (same source, same version-string → same content-hash inputs). Follow-up spec 229 documents the reproducibility contract explicitly.

---

## 5. Considered and rejected

Three alternative models from the §2 survey were seriously considered before the two-channel model won:

**Rejected: Rust nightly/beta/stable three-channel model (§2.1)** — this was the initial mental anchor. Nightly + beta + stable would give waybill maximum consumer-facing legibility (STRONG trust signal per §3 axis-2) and a mature burn-in window (beta) between the automated nightly and the manually-promoted stable. **Why not for waybill**: §3 axis-1 (maintainer time cost) is HIGH for this model — Rust's 6-week promotion train + weekly promotion ritual requires dedicated release-engineering capacity. §1 documents waybill's small contributor pool; the release-team-of-one can't sustain the ceremony. Adding beta later (§4's "strictly-additive future step") preserves the option without the initial cost.

**Rejected: Argo-CD quarterly-minor + patch + RC model (§2.5)** — this is the pattern that best matches waybill's compliance-consumer profile (SREs + platform teams overlap with waybill's security-team-CI-operator persona). The 3-minor support window would give consumers a predictable upgrade horizon. **Why not for waybill**: waybill isn't at 1.0 yet (`v0.1.0-alpha.70` per §1). Committing to a 3-minor support window when the minor-version semantics themselves aren't stable is over-promising. Argo-CD is at v3.x; the pattern makes sense there. Revisit when waybill hits 1.0.

**Rejected: Node.js LTS/Current model (§2.6)** — the LTS contract is the closest peer to what a CISA-2026-compliance consumer wants (a formal 30-month support window makes compliance planning tractable). **Why not for waybill**: LTS models require the maintainer to commit to backporting security fixes across N maintained branches for years. §1's small contributor pool + no commercial support arrangement makes that promise uncreditable. Node.js has OpenJS Foundation governance + per-releaser TSC-approved GPG keys — waybill has one maintainer. The LTS pattern is aspirational for a v3.x-or-later waybill, not v0.2.

---

## 6. Future-distribution compatibility

Distribution surfaces currently OUT OF SCOPE per FR-011 (this survey addresses only gh-release + OCI). Per FR-012, the §4 recommendation MUST NOT preclude future extension. Verification per surface:

| Surface | Common convention | Recommendation-compatibility note |
|---|---|---|
| **crates.io** | Accepts SemVer including pre-release syntax with dot separators (`0.2.0-nightly.20260806` is a valid crates.io version). Consumers can pin to `= 0.2.0-nightly.20260806` or use `>=0.2.0-nightly, <0.2.0` ranges. | **Compatible.** The `-nightly.YYYYMMDD` pre-release suffix chosen in §4.3 is crates.io-friendly. Publishing all nightlies to crates.io would flood the version history; recommend publishing ONLY stable to crates.io if/when publishing is added. |
| **homebrew** | Formula versions typically avoid pre-release suffixes; homebrew accepts them via `head` block or `pre_release` flag but many taps normalize away pre-release markers. Formula versioning conventions vary by tap. | **Compatible with caveats.** Stable tags (`v0.2.0`) are drop-in friendly for a Homebrew formula. Nightly formulae would need a separate `waybill-nightly` formula or a `head` block on the main formula — matches the pattern anchore/syft uses (§2.3 references `anchore/homebrew-syft` tap). Recommendation: if a Homebrew formula is added, ship stable only via the main formula + optional `--head` for nightly. |
| **cargo-binstall** | Reads GitHub release-artifact URLs by convention; version-string in artifact name must match the crates.io version. Handles SemVer pre-release syntax. | **Compatible.** The release-artifact naming convention already used by waybill's `release.yml` (`waybill-v0.1.0-alpha.70-<target-triple>.<ext>`) works for both stable and nightly under the new model with the tag substituted. cargo-binstall metadata in `Cargo.toml` (`[package.metadata.binstall]`) can be added when crates.io publishing lands. |
| **apt/rpm/dnf** | Package-manager conventions require canonical version strings; pre-release suffixes are handled via epoch/release-suffix conventions per distro. Most distros ship stable-only from a channel-specific repo (e.g., `waybill-stable` vs `waybill-nightly` repos). | **Compatible.** Stable-channel packages fit standard `apt`/`rpm`/`dnf` version conventions. Nightly would ship from a separate repo (`nightly.waybill.dev` or a `waybill-nightly` package name). Neither is in scope for this survey per FR-011; noted here as future-compatible. |

**Explicit non-issues**: none of the surfaces above rule out the two-channel model or the tag-format conventions chosen in §4.3. The main future decision points are (a) whether to publish nightlies at all to crates.io (recommendation: no; keep nightlies gh-release + OCI only) and (b) whether Homebrew ships nightlies via a separate formula or a `head` block (recommendation: separate formula if the tap grows past one).

**Explicit known issue**: Homebrew historically had friction with SemVer pre-release syntax containing dashes (`0.2.0-nightly.20260806` — the dash before `nightly`). Modern Homebrew formulae handle this via explicit version-parsing overrides. Reference for follow-up: https://docs.brew.sh/Formula-Cookbook#version. Not a blocker; noted for the follow-up spec 229.

---

## 7. Risks and open questions

Deliberately unresolved; deferred to the follow-up implementation spec (229-release-flow-implementation):

- **229-release-flow-implementation must be spec'd immediately after this merges.** The whole point of this survey is to inform 229; leaving 229 unspec'd for months lets peer-project state drift out from under the survey. Deferred to: immediate `/speckit.specify 229-release-flow-implementation` next.

- **Per-channel Sigstore Fulcio identity provider decisions.** The §4.4 decision "stable signed via m222 keyless flow" assumes Sigstore keyless works via the same GHA ambient token used for the existing signing tests. Nightly workflows running under `.github/workflows/nightly.yml` may have a different OIDC identity token audience than the existing tag-triggered `release.yml` — needs verification during 229 implementation whether the same Fulcio account/trust-root works. Deferred to: 229 implementation.

- **Reproducibility semantics per channel — nightly "1×/day skip-if-unchanged" cadence has an edge case.** If two commits land on `main` in a single day (call them A and B, with B merged 30 min after A), the nightly built at 00:00 UTC captures B, not A. A consumer wanting the SBOM for A-tip specifically must build from source at commit A — the nightly channel doesn't produce a per-commit artifact. Whether that's acceptable depends on downstream reproducibility contracts; §4 assumes yes. Deferred to: 229 implementation, plus a docs update to `docs/ecosystems.md` if the answer affects consumer guidance.

- **Homebrew SemVer pre-release compatibility.** §6 notes historical Homebrew friction with the `-nightly` dash. If waybill adds a Homebrew formula in future, the follow-up spec must validate the actual current Homebrew behavior against `v0.2.0-nightly.YYYYMMDD` — the friction may or may not still be an issue in modern Homebrew. Deferred to: whichever future spec adds Homebrew publishing (not 229).

- **Should nightlies use symbolic tag or per-day tag?** §4.3 chose per-day date-stamped tags (`v0.2.0-nightly.20260806`) — an alternative is a moving symbolic `nightly` tag that always points to the latest nightly (Rust uses BOTH: per-day archive + `nightly` symbolic pointer). §4 doesn't add the symbolic pointer; whether to add it depends on how consumers actually want to pin. Deferred to: 229 implementation (low-cost to add if requested).

- **Beta/RC promotion path if adopted later.** §4 defers the middle channel; §5 rejects three-channel initially. If consumer demand for a beta channel materializes, the promotion rule (does every N nightlies auto-promote to a beta? or does beta cut manually from `main`?) is unresolved. Deferred to: whichever future spec adds beta (not 229 unless 229 explicitly scopes it in).

- **Support-window contract.** §4 explicitly omits an LTS commitment. When (if) waybill reaches 1.0, revisiting whether to offer a support window — 3-minor (argo-cd model), 30-month (Node LTS), or none (trivy/syft model) — becomes actionable. Deferred to: post-1.0 governance spec.
