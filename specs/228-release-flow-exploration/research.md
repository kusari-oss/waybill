# Research: peer-project longlist + methodology

**Feature**: 228-release-flow-exploration
**Phase**: 0 (research)
**Date**: 2026-08-05

This phase resolves Technical Context unknowns and builds the methodology the writing phase executes. The **writing phase itself** — surveying the projects, filling the tradeoff matrix, drafting the recommendation — is the T-tasks (Phase 3+ of tasks.md). Phase 0 makes those tasks executable by pinning down: what to survey, which axes to score on, how to cite sources, and how to structure the recommendation.

## §A — Peer-project longlist + shortlist selection

### Decision: shortlist of 6 projects across 4 categories

Longlist candidates (per spec FR-002's 5-category framework):

**Category (a) — Rust CLIs similar to waybill's scale**:
- `sharkdp/bat` — mature, moderate release cadence, no multi-track model today; **candidate**
- `BurntSushi/ripgrep` — mature, moderate cadence, single-track; **candidate**
- `extrawurst/gitui` — mid-size, single-track; less-relevant (not multi-track)
- `dandavison/delta` — smaller, single-track; less-relevant
- `XAMPPRocky/tokei` — smaller, single-track; less-relevant

**Category (b) — SBOM-ecosystem tools**:
- `anchore/syft` — mature, multi-arch releases, single-track today but frequent releases; **candidate**
- `aquasecurity/trivy` — mature, multi-track (weekly + patch); **candidate — HIGH SIGNAL** (closest peer profile to waybill)
- `CycloneDX/cyclonedx-cli` — smaller, single-track; less-relevant
- `spdx/tools-golang` — library not CLI; less-relevant

**Category (c) — Fast-moving developer tooling with mature multi-track models**:
- `rust-lang/rust` — the canonical nightly/beta/stable model; **candidate — HIGH SIGNAL** (widely-recognized reference model)
- `denoland/deno` — canary/latest with SemVer; **candidate**
- `oven-sh/bun` — canary/latest, self-versioned via SemVer; less relevant to CLI shape
- `astral-sh/uv` — new but fast-moving, ships pre-releases; **candidate**

**Category (d) — Infrastructure/K8s-ecosystem CLIs**:
- `kubernetes/kubectl` — quarterly minor releases + patch cadence; too large-scale to fit waybill directly
- `argoproj/argo-cd` — quarterly minor + patch; **candidate**
- `fluxcd/flux2` — weekly release cadence; less-relevant
- `cilium/cilium` — quarterly minor + LTS branches; too large-scale

**Category (e) — Language runtimes with LTS models**:
- `nodejs/node` — LTS/current model; **candidate** (as REFERENCE for LTS pattern even though runtime not CLI)
- `python/cpython` — LTS-adjacent (community-driven); less-relevant

**Shortlist of 6** (spans 4 of 5 categories per spec FR-002 minimum of 3):

1. **`rust-lang/rust`** (category c) — canonical nightly/beta/stable reference model; both maintainers and downstream consumers already understand the pattern by shorthand.
2. **`aquasecurity/trivy`** (category b) — closest peer profile (SBOM/vuln tool, moderate contributor pool, mature CI, similar downstream-consumer expectations).
3. **`anchore/syft`** (category b) — sibling SBOM tool; different release cadence than trivy; comparison illuminates SBOM-ecosystem variance.
4. **`sharkdp/bat`** (category a) — small-to-medium OSS Rust CLI, single-track today; shows what waybill's current state looks like scaled-up.
5. **`argoproj/argo-cd`** (category d) — quarterly minor + patch cadence with pre-release tags; K8s-ecosystem convention.
6. **`nodejs/node`** (category e) — LTS/current reference; even/odd version LTS discrimination; the only surveyed project with a formal support-window contract.

**Alternates if any shortlisted project turns out to have insufficient source citations** during writing:
- `astral-sh/uv` (category c backup)
- `denoland/deno` (category c backup)
- `BurntSushi/ripgrep` (category a backup)

### Rationale

The shortlist is chosen to give waybill maintainer + reader (a) the canonical multi-track reference (rust), (b) two direct-peer SBOM tools (trivy + syft), (c) a Rust-CLI-scale reference to sanity-check that waybill isn't over-engineering (bat), (d) a K8s-ecosystem quarterly-cadence reference (argo-cd), and (e) an LTS-model reference for the "what if we wanted LTS eventually" consideration (nodejs). Every shortlisted project has public release automation the writing phase can cite.

### Alternatives considered

- **Shortlist of 3 projects** — would fit under FR-002's ≥5 floor, rejected.
- **Shortlist of 10 projects** — would blow spec SC-007's 800-line ceiling; rejected. Six projects × 100-line detail = 600 lines + shared front matter + recommendation = ~750 lines, fits budget.
- **Include a large enterprise project (kubernetes proper)** — rejected per spec Assumptions §2 ("peer means small-to-medium OSS ... NOT giant enterprise projects with dedicated release-engineering teams").

## §B — Tradeoff axis definitions

### Decision: 6 axes (5 required by FR-003 + 1 clarification-added nightly-cadence-variance)

Per FR-003 the matrix scores each project on ≥5 axes:

1. **Maintainer time cost per release cycle** — measured qualitatively (LOW / MEDIUM / HIGH) as observed from the project's release automation. LOW = fully-automated tag-push triggers release; MEDIUM = single PR + manual tag; HIGH = multi-step ceremony with changelog curation + release-notes drafting.
2. **Downstream-consumer trust signal quality** — measured on a 3-point scale (WEAK / MODERATE / STRONG) based on: does the channel name communicate stability guarantees to the consumer without documentation lookup?
3. **Breaking-change management** — categorical (SemVer strict / SemVer + LTS window / CalVer / hybrid). Notes any project-specific deviations.
4. **Artifact-availability latency** — from merged commit to downloadable artifact for the earliest channel. Measured in wallclock time (minutes/hours/days). Applies to the fastest channel (usually nightly).
5. **SBOM-artifact stability guarantees** — whether the project provides byte-reproducible artifacts across builds (relevant to waybill because it produces SBOMs that must reproduce for compliance auditing). YES / PARTIAL / NO / N/A.
6. **Nightly cadence (verbatim)** — per Q3 clarification: record each peer's actual cadence without normalization (e.g., "per-commit", "1×/day scheduled", "1×/day if changes", "manual only", "N/A — no nightly channel").

### Rationale

The 5 spec-mandated axes plus the nightly-cadence-variance axis together give the maintainer enough dimensions to defend the recommendation against alternatives. Axis 5 (SBOM reproducibility) is waybill-specific — most peer projects don't optimize for it, so answers will lean N/A or PARTIAL; that IS the interesting signal (waybill needs to preserve reproducibility that peers don't need).

### Alternatives considered

- **Add a "consumer feedback loop" axis** (e.g., how does the project collect + integrate feedback per channel?) — rejected as too subjective + hard to source-cite for the writing phase.
- **Add a "security-patch delivery model" axis** — folded into axis 3 (breaking-change management) as a sub-note per project rather than a separate axis. Cleaner matrix.
- **Reduce to 4 axes** — would fall below the FR-003 minimum of 5.

## §C — Source-citation methodology

### Decision: per-project citation checklist with 3 mandatory sources

For each shortlisted project, the writing phase MUST cite at minimum:

1. **Release page URL** — the GitHub releases tab or equivalent, showing the actual tag history the surveyed model produces. Verifies channel names + cadence claims.
2. **Release-triggering workflow YAML** — the `.github/workflows/*.yml` file (or equivalent CI config) that fires the release. Verifies "how a merged commit becomes an artifact" claim. If the workflow isn't public (unlikely for OSS), fall back to an official release-docs URL.
3. **Documentation** — the project's official release-policy documentation (`RELEASING.md`, `CONTRIBUTING.md#release-process`, or dedicated docs section). Verifies stability-guarantee claims + consumer-audience descriptions.

For projects that don't have all 3 (rare), the writing phase notes the gap explicitly rather than fabricating: "release-triggering workflow not public; cadence inferred from release-page tag timestamps".

Per memory `feedback-verify-research-empirical-claims`: every claim about a specific reader/project must be verifiable against actual current-state sources at authoring time. This applies to every one of the 6 shortlisted projects; grep-verify or fetch-verify the cited URL before recording any factual claim.

### Rationale

Three-source triangulation catches drift between "what the project documents" and "what the project actually does". A doc-only citation would let the writer inherit stale documentation; a workflow-YAML-only citation would miss human-driven release ceremonies; a release-page-only citation would miss the cadence and quality-bar policy. Together they let the writing phase produce claims that are verifiable spot-check-by-spot-check per spec SC-006.

### Alternatives considered

- **Single-source citations** — rejected for the drift risk noted above.
- **Interview project maintainers directly** — out of scope + not reproducible; the survey is public-source-only.
- **Rely on aggregator sites (e.g., libraries.io, release-tracker services)** — rejected; those add a source-of-truth intermediary that may itself be stale.

## §D — Recommendation-shape scaffolding

### Decision: recommendation subsection has 5 required components (per FR-005 + Q1 clarification)

The writing phase MUST include in the recommendation:

1. **Channel manifest** — named list of channels (e.g., "nightly | beta | stable" — or whichever set the survey concludes fits waybill). One-line per-channel description of intended audience.
2. **Per-channel cadence** — verbatim per channel (e.g., "nightly = 1×/day scheduled, skipped if no changes"; "beta = every 4 weeks"; "stable = manual, promoted from beta after 2-week burn-in"). Cadence formats can mix per Q3 (verbatim > normalized).
3. **Per-channel tag/version convention** — SemVer pre-release syntax preferred per Q2 compatibility invariant (e.g., `v0.2.0-nightly.20260806`, `v0.2.0-beta.1`, `v0.2.0`).
4. **Per-channel signing decision** — per FR-007, whether each channel gets Sigstore keyless signature (m222 flow) or not. Rationale per decision.
5. **Migration path from `v0.1.0-alpha.70`** — either explicit migration (e.g., "next release cuts as v0.2.0-nightly.20260807; alpha.70 stays as-is") OR explicit "no migration; new model starts at v0.2.0".

Followed by two mandated sibling subsections:

- **Considered-and-rejected**: at least 2 alternative models from the surveyed set with per-alternative "why not for waybill" rationale (per FR-005 + Q1).
- **Future-distribution compatibility**: which of {crates.io, homebrew, cargo-binstall, apt/rpm/dnf} the recommendation has been checked against, and which known conventions from those surfaces the recommendation honors (per FR-012).

### Rationale

The 5 required components together give the follow-up implementation spec a complete starting point — engineer can name every workflow, tag pattern, signing invocation, and migration step from those 5 fields alone. Removing any one would leave the follow-up spec with an ambiguity to resolve.

### Alternatives considered

- **Fold rejected-alternatives into a footnote** — rejected; the rejection rationale is load-bearing for defending the recommendation.
- **Skip migration-path field** — rejected; the follow-up implementation spec would immediately hit "what's the very next release tag" as its first unknown.

## §E — Placement decision + related-docs cross-linking

### Decision: single new file at `docs/design/2026-08-05-release-flow-survey.md`

Placement rationale per plan.md Summary + Project Structure. Not `docs/audits/` (audit = per-target external audit, not internal design doc); not `docs/reference/` (reference = evergreen consumer-facing content); `docs/design/` is the natural fit for point-in-time design decisions.

Verified during Phase 0: **`docs/design/` doesn't exist yet** (baseline check: `ls docs/` shows `audits/`, `reference/`, `architecture/`, `contributing/`, `ecosystems.md`, `index.md`, root-level md files). T-tasks create `docs/design/` as part of the same commit; no explicit "seed the directory" task needed because git preserves empty dirs poorly — the file's mere presence in the dir creates it.

Related-docs cross-linking: if `docs/index.md` hosts a "design docs" section, add a 1-line pointer. If not, no cross-link edit needed (design docs are discoverable via the `docs/design/` directory presence + this doc's link from the eventual `229-release-flow-implementation` spec).

### Rationale

Point-in-time design decisions belong under `docs/design/` — matches OSS convention for ADR-adjacent artifacts. Doesn't clutter `docs/reference/` with content that will age out once the follow-up implementation lands.

### Alternatives considered

- **Place under `specs/228-release-flow-exploration/` and don't publish under `docs/`** — rejected; the deliverable's US3 (downstream consumer) requires it be discoverable to consumers, not buried under `specs/`.
- **Place under `docs/adr/`** — rejected; project doesn't have an existing ADR discipline. Creating a new ADR directory for one document is premature; `docs/design/` is looser and easier to grow into.
- **Place inline within `README.md`** — rejected; too big for README, plus the recommendation will be superseded by implementation docs eventually.

## §F — Line-budget check

Estimated line breakdown for the deliverable (per spec SC-007's 800-line ceiling):

- Front matter (title, intent, TOC) — ~30 lines
- **§1 waybill context** (current state — v0.1.0-alpha.70, CI shape, known blockers, compliance target) — ~50 lines
- **§2 Peer-project survey** (6 projects × ~80 lines each — 3-source citations + project-shape paragraph + channel-model paragraph + "why this fits their project" note) — ~480 lines
- **§3 Tradeoff matrix** (6 rows × 6 axes + prose interpretation) — ~50 lines
- **§4 Recommendation** (channel manifest + per-channel cadence/tag/signing + migration path) — ~80 lines
- **§5 Considered-and-rejected** (2+ alternatives × ~15 lines each) — ~40 lines
- **§6 Future-distribution compatibility** — ~30 lines
- **§7 Risks and open questions** — ~30 lines
- Cross-links + closing — ~10 lines

Total estimate: **~800 lines** — at the SC-007 ceiling. Writing phase must either stay disciplined (per-project summaries shouldn't sprawl) OR the writing phase can compress the per-project sections to ~60 lines each (~360 total for §2) which drops the total to ~680 lines with 120-line headroom.

### Rationale

Six projects is the maximum that fits SC-007's 800-line ceiling with adequate per-project depth. Adding a 7th project would require compressing every existing section OR blowing the budget.

### Alternatives considered

- **Shortlist of 5 projects** — would leave ~100 lines of headroom, but drops one of the 4 categories to only 1 project (thin sample). Six projects preserves category coverage.
- **Shortlist of 7 projects, compressed per-project sections** — feasible but reduces per-project detail below what SC-006 spot-check tolerance allows (harder to source-cite verifiably in a 45-line section).

## §G — Risks + follow-up seeds

Surfaced during Phase 0, filed as issues after merge:

1. **The 229 implementation spec should be spec'd immediately after this merges** — the whole point of this feature is to inform 229; leaving 229 unspec'd for months lets peer-project state drift out from under the survey.
2. **CISA 2026 signing per-channel may need per-channel identity provider decisions** — Sigstore keyless via GHA ambient tokens works for stable-channel release-artifact signing (matches how m222 landed); nightly channels signed by a different identity may need different Sigstore Fulcio account / trust-root configuration. Deferred to 229.
3. **Reproducibility across channels** (FR-007d) — nightly-cadence "per-commit" implies every merged commit produces a different artifact hash even for no-behavior-change commits (e.g., docs-only merges). Whether that's acceptable depends on downstream-consumer needs; the recommendation must call out the reproducibility semantic per channel.
4. **Homebrew formula compatibility** — homebrew doesn't like SemVer pre-release syntax with dashes in some formula types; the future-distribution-compatibility subsection should note this specific known-issue if the recommendation uses pre-release syntax like `v0.2.0-nightly.20260806`.

These seeds live here for the writing phase to reference; they'll be filed as separate GitHub issues after the survey doc merges (matching the m227 follow-up-issue pattern).
