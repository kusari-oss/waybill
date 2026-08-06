# Feature Specification: Survey peer-project release flows + recommendation for waybill's multi-track release strategy

**Feature Branch**: `228-release-flow-exploration`
**Created**: 2026-08-05
**Status**: Draft
**Input**: User description: "I want a new release flow, I want to now have something like nightly releases, stable releases, beta releases, etc. I first want to explore though how other projects handle versioning in a good way."

## Clarifications

### Session 2026-08-05

- Q: Should the deliverable end with a single decisive recommendation, 2–3 finalists, or a menu of viable models? → A: Single decisive recommendation (survey concludes with ONE preferred model, justified against alternatives).
- Q: What distribution-channel scope should the survey cover? → A: In-scope: gh-release + OCI only (matches waybill's current distribution surface). Recommendation MUST NOT preclude future extension to crates.io / homebrew / cargo-binstall / apt|rpm|dnf repos — channel names and tag/version conventions must remain compatible with those downstream distribution paths so a future expansion doesn't require a breaking-change release-model rework.
- Q: When comparing peer projects' nightly cadences, should the survey enforce one common definition, document per-project variance verbatim, or skip nightly entirely? → A: Document per-project variance verbatim (record each peer's actual cadence: "per-commit", "1×/day", "1×/day if changes", etc.). The tradeoff matrix carries a per-cadence column; the recommendation picks a specific cadence for waybill and justifies against the observed variance.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Maintainer explores peer-project release-track patterns before committing to one (Priority: P1)

Today waybill ships a single `v0.1.0-alpha.N` sequential track — every commit landing on `main` becomes a candidate for the next alpha bump. The user (project maintainer) wants to differentiate release channels — nightly, beta, stable, etc. — but **explicitly wants to survey how similar projects have solved this before picking an approach**. The primary deliverable of this feature is a written survey + tradeoff analysis, NOT a shipped release flow. Implementation of whichever approach gets chosen is a separate follow-up.

**Why this priority**: The user explicitly asked to "first explore though how other projects handle versioning in a good way." Committing to a specific channel model (Rust's nightly/beta/stable, Node's LTS/current, Kubernetes' per-feature-gate, etc.) without surveying the space risks copying a pattern that fits somebody else's project stage and shape but not waybill's. Getting the survey right is what makes the eventual implementation defensible.

**Independent Test**: A reader unfamiliar with waybill's release history can, using only the survey deliverable, (a) name at least 4 distinct release-track patterns used by peer OSS projects, (b) understand the tradeoffs each pattern imposes on maintainers vs consumers, (c) explain why the survey's recommendation fits waybill specifically and not, say, a much larger or much smaller project.

**Acceptance Scenarios**:

1. **Given** a maintainer preparing to design waybill's next release model, **When** they open the survey deliverable, **Then** they find a concrete comparison of at least 5 peer projects covering nightly, pre-release, and stable-channel patterns — with each project's project shape (age, contributor count, release cadence, downstream-consumer profile) noted so the reader can judge fit.
2. **Given** a maintainer deciding between competing release models, **When** they read the survey's tradeoff matrix, **Then** they can identify which axes matter most for waybill (e.g., maintainer time cost, downstream trust signal, SBOM-artifact stability guarantees) and see how each surveyed pattern scores on each axis.
3. **Given** a maintainer evaluating the survey's recommendation, **When** they read the rationale, **Then** they see explicit ties from waybill's specific context (current CI cadence, contributor size, downstream-consumer expectations, existing `alpha.N` sequential model, `RELEASE_TAG_TOKEN` auto-tag brokenness) to the recommendation's design choices.

---

### User Story 2 - Maintainer has a single decisive recommendation with success criteria for a follow-up implementation (Priority: P2)

The survey (US1) informs **one** decisive recommendation. The recommendation must be specific enough that a follow-up implementation spec can turn it into concrete work — naming release channels, cadences, tagging conventions, artifact-versioning schemes, and CI-workflow shape without ambiguity. The recommendation IS a design decision; the survey is the input that justifies it. Alternatives explicitly-considered-and-rejected are acknowledged in the recommendation with brief rationale, but only one model advances to the follow-up implementation spec.

**Why this priority**: A survey without a recommendation is a research paper; a recommendation without a survey is guesswork. Both together compose into an executable design.

**Independent Test**: An engineer opening the recommendation section, without reading the survey, can write a full implementation plan (channel names, tag-format regex, workflow-yaml sketch, changelog policy) — because the recommendation is specific enough to be actionable.

**Acceptance Scenarios**:

1. **Given** the survey concludes with a recommendation, **When** an engineer opens the recommendation, **Then** they see a named set of release channels (e.g., `nightly`, `beta`, `stable` — or whatever the survey concludes fits waybill), a per-channel cadence and quality bar, a per-channel tagging convention, and a per-channel consumer-audience statement.
2. **Given** the recommendation names N channels, **When** an engineer traces how a single commit flows through those channels from merge → downstream artifact availability, **Then** the recommendation answers every question that flow raises (e.g., does every merge become a nightly? do nightlies get promoted to beta on some cadence? does beta promote to stable on manual approval?).

---

### User Story 3 - Downstream consumer (SBOM operator) can predict release-channel semantics before adopting waybill (Priority: P3)

Downstream consumers of waybill-emitted SBOMs need to know which release channel to pin their pipelines against. A CI pipeline pinning to `nightly` accepts breaking-change risk daily; a pipeline pinning to `stable` gets predictable behavior but slower feature delivery. The recommendation must document this consumer-facing semantic so operators can pick correctly.

**Why this priority**: This is a smaller audience than the two maintainer-facing stories above, but it's the reason the differentiation exists in the first place. A release model that maintainers understand but consumers can't act on defeats the purpose.

**Independent Test**: A first-time waybill adopter can, from the recommendation's consumer-facing section alone, decide which channel their SBOM-generation pipeline should track.

**Acceptance Scenarios**:

1. **Given** a security-team operator evaluating waybill for production SBOM generation, **When** they read the recommendation's per-channel consumer-audience statement, **Then** they can decide (a) which channel fits their risk tolerance, (b) what stability guarantees they can rely on from that channel, and (c) how to detect channel-promotion events for pipeline-update planning.
2. **Given** a fast-moving research team wanting the latest waybill features, **When** they read the recommendation, **Then** they understand which channel to track and what the tradeoffs are (breakage risk, changelog volatility) compared to a slower channel.

---

### Edge Cases

- **Zero-project survey**: what if the maintainer's project-scale filter (small OSS Rust CLI, similar CI cadence) yields fewer than 5 peer projects to survey? The survey should broaden the filter transparently rather than pretending it found more than it did.
- **Survey disagreement with recommendation**: if the survey's aggregate signal points to Model A but the recommendation picks Model B for waybill-specific reasons, the recommendation MUST explicitly name the divergence and justify it — not paper over it.
- **Recommendation contradicts waybill's existing conventions**: e.g., if the survey recommends CalVer (calendar versioning) but waybill has been on SemVer-derived `alpha.N` for a year, the recommendation must explicitly address the migration path (or explicitly recommend against migrating).
- **Channel semantics conflict with existing tooling**: e.g., cargo-binstall, dependabot, homebrew formulae all have opinions about pre-release version syntax. The recommendation should note which downstream-tooling assumptions the channel model touches.
- **Auto-tag workflow already broken**: the current `RELEASE_TAG_TOKEN` failure (see memory `reference_release_process`) affects how any new release flow gets triggered. The recommendation should call out whether the new flow reuses the broken workflow or replaces it.
- **CISA 2026 compliance interaction**: waybill's compliance target (constitution Principle V) mandates SBOM Author Signature via Sigstore keyless. Multi-channel releases need per-channel signing decisions — does nightly get signed? does beta? The recommendation must address.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The deliverable MUST include a survey of at least 5 peer OSS projects' release-track models. Each surveyed project's entry MUST include: project name + link, current release channel model (channels named, cadence per channel, tag/version convention per channel), project shape (age, approximate contributor count, primary language, downstream-consumer profile), and a one-sentence "why this fits their project" note.

- **FR-002**: Surveyed projects MUST span at least three of the following categories to avoid single-model bias: (a) Rust CLIs similar to waybill's size (bat, ripgrep, gitui, delta, tokei); (b) SBOM-ecosystem tools (syft, trivy, cyclonedx-cli, spdx-tools); (c) fast-moving developer tooling with mature multi-track models (Rust itself, deno, bun); (d) infrastructure/K8s-ecosystem CLIs (kubectl, argocd, flux, cilium); (e) language runtimes with LTS models (Node.js, Python). The survey MUST justify each project's inclusion.

- **FR-003**: The deliverable MUST include a tradeoff matrix scoring each surveyed model on at least these 5 axes: (a) maintainer time cost per release cycle, (b) downstream-consumer trust signal quality, (c) breaking-change management (SemVer / CalVer / hybrid), (d) artifact-availability latency (how fast does a merged commit become downloadable in each channel), (e) SBOM-artifact stability guarantees (relevant to waybill since it produces SBOMs that must reproduce reliably for compliance auditing). **Per-project nightly cadence MUST be recorded verbatim in a dedicated matrix column** (e.g., "per-commit", "1×/day scheduled", "1×/day if changes", "manual only") rather than normalized to a common definition — real variance in nightly semantics IS the tradeoff being surveyed. The recommendation subsection (FR-005) selects a specific cadence for waybill and justifies it against the observed variance.

- **FR-004**: The deliverable MUST include a "waybill context" subsection listing the project's current state that any recommendation must fit: current version (`v0.1.0-alpha.70`), current release model (single sequential alpha channel), current CI shape (3-lane CI < 5 min per memory `project_ci_timing`), current known blockers (auto-tag-release.yml consistently fails on missing RELEASE_TAG_TOKEN per memory `reference_release_process`; release-bump PRs are 30+ min due to golden-fixture cache invalidation per memory `feedback_release_bump_prepr_slow`), current compliance target (CISA 2026 per constitution Principle V), and current signing posture (Sigstore keyless via m222 `--sign`).

- **FR-005**: The deliverable MUST include a **single decisive recommendation** subsection with: ONE named release model (not a menu of alternatives), including named release channels + cadence + tag/version convention per channel; a named migration path from waybill's current `alpha.N` model to the new model (or explicit "no migration; new model applies to future releases only"); explicit callouts of any pre-existing workflow the recommendation touches (release.yml, auto-tag-release.yml, release-bump PR template); consumer-facing "how to pick a channel" guidance; AND a "considered and rejected" subsection listing at least 2 alternatives that were surveyed but not chosen, each with a one-sentence "why not for waybill" rationale.

- **FR-006**: Every claim in the survey about a specific peer project's release model MUST be verifiable against that project's public documentation, GitHub Actions workflows, release page, or CHANGELOG at survey-authoring time. The deliverable MUST cite sources per project (URLs to release pages, workflow YAML, or docs).

- **FR-007**: The recommendation MUST explicitly address the interactions with waybill's specific concerns: (a) CISA 2026 signing (does each channel get signed?), (b) SBOM golden-fixture cache invalidation on version bumps (does the new model reduce release-bump PR overhead?), (c) `RELEASE_TAG_TOKEN` auto-tag brokenness (does the new flow reuse or replace the broken workflow?), (d) reproducibility (do the same source-inputs produce byte-identical artifacts across channels?).

- **FR-008**: The deliverable MUST include a "risks and open questions" subsection listing things the recommendation deliberately doesn't answer, deferring them to the follow-up implementation spec (e.g., "should the nightly channel have a symbolic `nightly` tag that moves, or per-day `nightly-YYYY-MM-DD` tags? — deferred to implementation").

- **FR-009**: The deliverable MUST be a single markdown document rendered correctly on GitHub without extra plugins. Tables MUST use standard pipe-delimited markdown. No custom rendering syntax.

- **FR-010**: The deliverable MUST be scoped to research + recommendation only. No workflow YAML changes, no `Cargo.toml` changes, no shipping code changes are part of this feature. Implementation is a separate follow-up spec.

- **FR-011**: Distribution-channel scope for the survey is bounded to waybill's current distribution surfaces — GitHub release-page tarballs + multi-arch OCI container image (per the existing `release.yml`). Other distribution paths (crates.io publishing, homebrew formula, cargo-binstall metadata, apt/rpm/dnf repositories) are OUT OF SCOPE for this survey.

- **FR-012**: The recommendation MUST NOT preclude future expansion to the OUT-OF-SCOPE distribution surfaces enumerated in FR-011. Channel names, tag/version conventions, and per-channel signing decisions MUST remain compatible with common downstream conventions (SemVer pre-release syntax that homebrew and cargo-binstall accept; OCI tag patterns that registries can host alongside application tags; crates.io's ability to publish pre-release versions via SemVer `-<pre>` suffixes). The deliverable MUST include a "future-distribution compatibility" subsection listing which downstream surfaces the recommended model has been checked against and which known constraints from those surfaces the recommendation honors (e.g., "crates.io accepts SemVer pre-release syntax like `-alpha.N` and `-beta.N`; recommendation uses this form so future crates.io publishing works without renaming").

### Key Entities *(include if feature involves data)*

- **Release channel**: a named quality-and-cadence bucket that a build passes through on its way from source-tree merge to downstream consumer. Nightly, beta, stable, RC are typical examples; specific names depend on which surveyed model gets recommended.
- **Release model**: the full policy that defines what channels exist, how commits flow through them, and how each channel is tagged/versioned/signed. Distinct from any single channel.
- **Peer project**: an OSS project whose release model is surveyed for informative comparison. Selection criteria per FR-002.
- **Tradeoff axis**: a dimension along which release models can be compared. Enumerated in FR-003.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A reader unfamiliar with waybill's release history can, using only the survey deliverable, correctly name at least 4 distinct release-track patterns used by peer OSS projects and explain the tradeoffs each imposes. Verification: 5-minute reading test + written recall.

- **SC-002**: The survey covers at least 5 peer projects across at least 3 of the 5 FR-002 categories, with each project's inclusion explicitly justified. Verification: manual count.

- **SC-003**: The tradeoff matrix scores every surveyed project on at least 5 axes (FR-003 enumerates them). Verification: matrix row-count × column-count == expected total.

- **SC-004**: The recommendation names specific release channels, cadences, tag conventions, and consumer audiences — specific enough that an engineer can write a follow-up implementation spec from the recommendation alone. Verification: attempt to write a tasks-shaped implementation plan from the recommendation; success is producing at least 5 executable tasks without ambiguity.

- **SC-005**: The recommendation explicitly addresses all 4 FR-007 waybill-specific concerns (CISA 2026 signing, cache invalidation, RELEASE_TAG_TOKEN, reproducibility). Verification: grep-based count of subsection matches.

- **SC-006**: Every peer-project claim in the survey is source-cited. Verification: manual spot-check of 5 randomly-chosen claims against the cited sources; target 5/5 match.

- **SC-007**: The deliverable is bounded to ≤ 800 lines of markdown. Longer suggests scope creep into implementation detail; shorter risks under-surveying.

- **SC-008**: A downstream consumer (representative persona: security-team operator using waybill in a CI pipeline) can, from the recommendation's consumer-facing section alone, decide which channel their pipeline should track. Verification: reading test against a written persona description; success is a defensible channel choice + reasoning that matches the recommendation's intended segmentation.

## Assumptions

- **Deliverable is exploration + recommendation only**: this feature ships a markdown document. No CI workflow changes, no `Cargo.toml` changes, no release-artifact changes. The recommendation IS the deliverable; implementing it is a follow-up spec (probably `229-release-flow-implementation` or similar).
- **Scope of "peer" projects**: peer means "small-to-medium OSS Rust CLI OR mature SBOM tool OR reference multi-track model" — NOT giant enterprise projects with dedicated release-engineering teams. The recommendation must fit waybill's actual maintainer bandwidth (small contributor pool).
- **SemVer bounds**: waybill has been on a SemVer-derived `0.1.0-alpha.N` model. The survey MAY recommend moving off SemVer (to CalVer or a hybrid), but MUST justify explicitly. Default assumption: SemVer stays as the versioning grammar; the innovation is in channel differentiation, not in versioning grammar.
- **Compliance framing**: waybill's compliance target (CISA 2026 per Principle V) constrains the recommendation to release models where each channel's artifacts can carry SBOM Author Signature + a valid Generation Context. Channels that couldn't be signed (hypothetically — none surveyed likely fit this) would be rejected.
- **Auto-tag workflow inheritance**: the recommendation MAY propose replacing the broken auto-tag-release.yml with a different trigger (e.g., a slash-command-triggered workflow, or a scheduled cron for nightlies). The recommendation MUST address this, not silently inherit the broken state.
- **No CalVer requirement**: some peer projects (e.g., Ubuntu, Manjaro, cargo-nightly-YYYY-MM-DD) use CalVer for nightly channels. The recommendation MAY adopt or reject CalVer for the nightly channel specifically; other channels default to SemVer.
- **jq-recipe verification not required for this spec**: unlike the m227 docs milestone, this feature doesn't emit consumer-facing SBOM recipes. Verification is limited to source-citation accuracy for surveyed projects.
- **Timing scope**: the survey is authored against peer projects' release models as of survey-authoring time (roughly mid-2026). Follow-up implementation might discover projects have evolved; that's acceptable — the survey is a point-in-time snapshot informing a decision, not an evergreen reference doc.
- **Distribution-surface scope + future-compatibility invariant**: the survey addresses only waybill's current distribution surfaces (gh-release + OCI) per FR-011, but the recommendation is bound by FR-012 to remain compatible with common downstream conventions (crates.io, homebrew, cargo-binstall, apt/rpm/dnf) so a future expansion doesn't require breaking the model.
