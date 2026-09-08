---

description: "Task list for m776 — component source-provenance references"
---

# Tasks: Component Source-Provenance References (m776)

**Input**: Design documents from `/specs/776-component-source-refs/`
**Prerequisites**: plan.md (Phase 0/1 complete), spec.md (post-clarify, 2 clarifications), research.md (R1–R8), data-model.md, contracts/ (2 files), quickstart.md

**Tests**: Required by design. Contracts 2–6 and 9 of `enrichment-link-mapping.md` and Contracts 2/3/5 of `derived-distribution-refs.md` each specify a test. SC-009a requires an automated check that summary counts match the emitted document. Mapping, validation, dedup, and ordering are pure functions over in-memory data — no network, no toolchain, no privilege (Constitution Principle VII).

**Organization**: Two independent user stories. US1 (P1) carries the measured value and the observability; US2 (P2) is the offline complement and is separable.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Parallelizable (different files, no dependency on an incomplete task)
- **[Story]**: Required in Phase 3 (US1) and Phase 4 (US2) only
- Every task cites an exact file path

## Path Conventions

Single Cargo workspace at repo root. Three production files: `waybill-cli/src/enrich/depsdev_source.rs` (US1 primary), `waybill-cli/src/scan_fs/mod.rs` (US2 primary), `waybill-cli/src/cli/scan_cmd.rs` (summary emission). **No emitter is touched** — all three already consume `external_references` (research R5). New tests land in in-file `#[cfg(test)]` modules.

---

## Phase 1: Setup

**Purpose**: Capture the before-state that SC-001, SC-002, SC-003, SC-006, and SC-010 are measured against.

- [X] T001 Confirm branch `776-component-source-refs` is checked out and the tree is clean via `git status --short && git branch --show-current`. — verified.
- [X] T002 Build a pre-milestone baseline binary from `main` in a worktree: `git worktree add /tmp/waybill-main main && cd /tmp/waybill-main && cargo build -p waybill --release`. This is the comparison anchor for SC-003 (no coverage regression), SC-006 (wall time), and SC-010 (diff confined to added references). — done: worktree at `/tmp/waybill-main`, release binary from `2049f1f`.
- [X] T003 [P] Capture the SC-001/SC-002/SC-003 coverage baseline: scan each of the five measurement fixtures with the T002 binary, enrichment enabled, and record for each the count of components carrying any `externalReferences` entry and the count carrying a `vcs` entry specifically. Reference values measured during specification: py-uv ~1 of 109, npm-nodejs 0 of 369, rust-ripgrep 61 references but ~1 with a source kind, go-cobra 4 of 7, maven-jvm ~1 of 111. Retain the emitted documents for the SC-010 diff. — done. Baselines (vcs-carrying components): go-cobra 4/7, py-uv **0**/109, rust-ripgrep **0**/68 (61 refs, all `website`), npm-nodejs **0**/369, maven-jvm **0**/111. Note: py-uv and maven-jvm are 0, not the ~1 the spec estimated — that estimate came from reading a fractional score.
- [X] T004 [P] Capture the SC-006 wall-time baseline on the largest fixture (npm-nodejs, 369 components) with the T002 binary. Record best-of-two. — done: npm-nodejs best-of-two **18.25s**.

---

## Phase 2: Foundational

**Purpose**: The reference-normalization helper both stories depend on. **Blocking**: US1 and US2 both append to `external_references`, and FR-006 (dedup) and FR-013 (ordering) are properties of the combined result, so they cannot be implemented independently per story.

- [X] T005 Add a shared normalization helper that takes a component's accumulated `external_references` and returns them deduplicated on the `(ref_type, url)` pair and stably sorted on that same pair. Place it where both call sites can reach it (`waybill-common/src/resolution.rs` alongside the `ExternalReference` type, or a shared module under `waybill-cli/src/`). Dedup MUST be on the pair, not URL alone — the same URL under two kinds is two distinct claims (research R4, data-model.md validation rules). — done: `normalize_external_references` in `waybill-common/src/resolution.rs`, dedup+stable-sort on the `(ref_type, url)` pair. Also added `M776_DERIVED_REF_TYPES` bounding what this project derives.
- [X] T006 [P] Add a URL-validation helper rejecting empty and non-absolute URLs per FR-004, following the established `url::Url::parse` pattern at `waybill-cli/src/identifiers/sanitize.rs:59`. The `url` crate is already a workspace dependency (`Cargo.toml:37`) — no new dependency (FR-015). Return an `Option`/`Result` rather than panicking (Principle IV). — done, **placement deviation**: `is_valid_absolute_url` lives in `waybill-cli/src/enrich/depsdev_source.rs`, not `waybill-common`. The `url` crate is not a `waybill-common` dependency and adding it would touch that crate's manifest, against FR-015's intent. US2's URLs are constructed from templates and well-formed by construction, so enrichment is the only consumer.
- [X] T007 [P] Add unit tests for T005 and T006: duplicate pair collapses; same URL under two kinds is retained; shuffled input yields identical ordering; empty string, relative path, and scheme-less forms are all rejected. — passing: 5 tests in `waybill-common` (duplicate-pair collapse, same-URL-two-kinds retained, order-independence, idempotence, CDX-native kinds).

**Checkpoint**: Helpers compile and are unit-tested. Neither story has started.

---

## Phase 3: User Story 1 - Source-provenance references from enrichment metadata (Priority: P1) 🎯 MVP

**Goal**: Map the deps.dev `links[]` array — already fetched and discarded — onto external references, and make the mapping's behavior observable.

**Independent Test**: Scan the Python fixture with enrichment enabled; at least 80% of components carry a `vcs` reference (baseline ~1 of 109). Per quickstart Step 1.

### Phase 3a — Mapping

- [X] T008 [US1] In `waybill-cli/src/enrich/depsdev_source.rs`, extend `apply_version_info` (~line 83) — the function that already applies `info.licenses` — to also read `info.links`. For each link, map the label to a reference kind per the Contract 2 table: `SOURCE_REPO`→`vcs`, `ISSUE_TRACKER`→`issue-tracker`, `DOCUMENTATION`→`documentation`, `HOMEPAGE`→`website`, `ATTESTATION`→`attestation`. Reference: contracts/enrichment-link-mapping.md. — done: `ref_type_for_link_label` maps the five labels.
- [X] T009 [US1] Ensure the mapping is driven by the **label only**. The URL's shape MUST NOT influence the chosen kind — a `HOMEPAGE` pointing at a repository host stays `website`. Inferring kind from URL shape is precisely the guess FR-003 forbids, and real fixtures exercise this because many packages set their homepage to their repository page. — done: mapping is label-driven; test `m776_kind_is_label_driven_not_url_driven` asserts a repository-host URL under `HOMEPAGE` stays `website`.
- [X] T010 [US1] Skip unmapped labels without failing the scan and without per-occurrence output (FR-003). `ORIGIN` is treated exactly as any other unmapped label — **not** special-cased into silence, so it counts as a skip and the summary reflects reality (FR-002a, Clarifications Q1). Do not add an `ORIGIN` arm. — done: `ORIGIN` has no arm and is counted as an unmapped skip. Empirically 334 such skips on npm, **0 on pypi** — `ORIGIN` is npm-specific, correcting the spec's assumption that it appears universally.
- [X] T011 [US1] Apply the T006 URL validation before emitting: links with empty or non-absolute URLs are skipped and counted separately from unmapped-label skips (FR-004, FR-014b). A component whose enrichment metadata is entirely malformed is still emitted, without enrichment-derived references (NFR-002). — done: validation applied; malformed counted separately. Real data exercised it — `skipped_malformed_url=1` on npm.
- [X] T012 [US1] Apply the T005 normalization to the component's `external_references` after appending, so FR-006 (dedup) and FR-013 (ordering) hold over the combined set of derived references — those from US2's PURL derivation and those from US1's enrichment mapping. — done: normalization applied to derived references only, per the F2 remediation.
   **Scope correction**: operator-supplied references are NOT in scope here and will not be present. The supplement merge stores them as a `waybill:supplement-externalReferences` annotation (`waybill-cli/src/supplement/merge.rs:193`), not in the `external_references` field, and it runs at ~`scan_cmd.rs:3841` — long after enrichment. FR-012 ("preserve operator-supplied references, unmodified and un-reordered") is therefore satisfied structurally: this milestone cannot reach them. Do NOT go looking for them here, and do NOT wire them into `external_references` to make normalization cover them — that would change supplement semantics and would be the one way to actually violate FR-012.
- [X] T013 [US1] Verify no additional network request was introduced (FR-007): the links come from the `VersionInfo` already fetched for license enrichment, and the existing per-scan response cache is reused unchanged. Confirm by inspection that no client call was added to the mapping path. — verified: no client call added; the links come from the already-fetched `VersionInfo`. Wall time went **down** (16.34s vs 18.25s), which no per-component network call could do.
- [X] T014 [US1] Correct the stale comment at `waybill-cli/src/enrich/deps_dev_client.rs:4` which states the payload "drives license enrichment but `advisory_keys` / `links` aren't yet" consumed. `links` is now consumed; `advisory_keys` remains unconsumed. Leave the `Link` and `VersionInfo` types themselves unchanged. — done: comment at `deps_dev_client.rs:4` corrected; `advisory_keys` noted as still unconsumed.

### Phase 3b — Observability

- [X] T015 [US1] Accumulate the two skip counters in `enrich_components` (`waybill-cli/src/enrich/depsdev_source.rs:156`), which already loops every component and returns a count. Track unmapped-label skips and malformed-URL skips **separately** (FR-014b) — they call for opposite responses: a rising unmapped count means the upstream vocabulary moved and a label should be mapped; a rising malformed count means upstream data quality degraded and it should not. Extend the function's return to carry both alongside the existing enriched count. — done: `LinkMappingSkips { unmapped_label, malformed_url }` accumulated in `enrich_components` and returned alongside the enriched count.
- [X] T016 [US1] Emit the FR-014a aggregate summary once per scan from `waybill-cli/src/cli/scan_cmd.rs`, **after the component set has stopped changing**. Derive the per-kind emitted counts by counting the final `Vec<ResolvedComponent>` directly rather than by threading counters through both paths — this makes SC-009a hold by construction, because the reported counts cannot drift from the document they are counted from. The skip counters come from T015, since skips never reach the document. Emit exactly once regardless of component count; never per-component, never per-link. — done at the **corrected** emission point (after the m220 scope filter and all tag passes), per the F1 remediation. Implementation surfaced an additional dropping pass the analysis had not enumerated — the m220 post-discovery scope filter — reinforcing that ~3520 would have been wrong.
   **Emission point (corrected)**: do NOT emit near the `enrich_components` call (~3520). Several passes after it still mutate the component set — `deduplicate()` (~3587), `reconcile_design_source_tiers()` (~3600), a `components.retain` drop pass (~3624), the supplement install rebinding `components` (~3849), and the layer-digest / workspace-member tag passes (~3865, ~3883). A summary emitted at 3520 would **overcount** relative to the emitted document whenever any of those drops a component carrying references, which is precisely what SC-009a forbids. Emit after the last tag pass (~3883) and before serialization. Note also that the existing `"scan complete"` log at ~3406 fires BEFORE enrichment and is therefore not a valid anchor either.

### Phase 3c — Tests

- [X] T017 [US1] Add tests in `depsdev_source.rs`'s `#[cfg(test)]` module for Contract 2: each of the five mapped labels produces its expected kind; `ORIGIN` produces none; a synthetic unknown label produces none and does not fail. — passing.
- [X] T018 [US1] [P] Add a test for Contract 2's label-driven constraint (T009): a `HOMEPAGE` link whose URL points at a repository host still yields `website`, never `vcs`. — passing.
- [X] T019 [US1] [P] Add a test for Contract 3: every kind the mapping can produce is a member of the allowed natively-defined set (`vcs`, `issue-tracker`, `documentation`, `website`, `attestation`, `distribution`). This is the executable form of FR-005 / SC-009 — no `waybill:*` property is introduced for source provenance. — passing.
- [X] T020 [US1] [P] Add a test for Contract 4 / NFR-002: empty, relative, and scheme-less URLs are skipped, counted as malformed rather than unmapped, and the component survives with its other references intact. — passing: empty, scheme-less, and relative URLs all counted as malformed (3), not unmapped (0); the one valid reference survives.
- [X] T021 [US1] Add a test for Contract 9 / SC-009a over a fixture mixing mapped, unmapped, and malformed links: the summary is emitted exactly once, and its per-kind counts equal the references present in the emitted document. — passing; end-to-end confirmed: summary `by_kind` matches `jq` over the emitted document exactly.

### Phase 3d — Empirical validation

- [X] T022 [US1] Build release and verify SC-001 and SC-002 per quickstart Step 1: on py-uv and npm-nodejs with enrichment enabled, at least 80% of components carry a `vcs` reference. Baselines from T003 were ~1 of 109 and 0 of 369. Upstream availability was measured during specification at npm 100% (30/30) and pypi 93% (27/29), so a large shortfall indicates a defect rather than missing upstream data. — done. **SC-001 py-uv 0/109 → 99/109 (91%)**; **SC-002 npm 0/369 → 332/369 (90%)**. Also go-cobra 57%→86%, rust-ripgrep 0%→90%. maven-jvm 0%→29% — below 80%, but maven is not an SC-001/SC-002 target; deps.dev's maven link coverage is simply thinner.
- [X] T023 [US1] Verify the summary per quickstart Step 2: exactly one line per scan; per-kind counts agree with `jq` over the emitted document; both skip counters present and distinct. **A non-zero unmapped count is expected, not a failure** — `ORIGIN` appears on essentially every component and is deliberately unmapped, so roughly one unmapped skip per component is the normal steady state. — done. Both counters present and distinct. **Correction to this task's own prediction**: "roughly one unmapped skip per component" holds for npm (334/369) but NOT pypi (0) — `ORIGIN` is npm-specific.
- [X] T024 [US1] Verify SC-003 (no regression), SC-010 (diff confined to added references), and **FR-016 (all three formats)** against the T003 baseline documents. For FR-016, pick one component known to carry a `vcs` reference and confirm it appears in all three outputs: CycloneDX `externalReferences[]`, SPDX 2.3 `externalRefs[]` (category `OTHER`), and SPDX 3 `software_sourceInfo`. Research R5 asserts this is free because every emitter already consumes the field — that claim is load-bearing for the plan's "no emitter changes" structure and deserves one direct confirmation rather than only implicit coverage via the regression suites. Mask document-identity fields and, per memory `feedback_verify_golden_churn_normalized`, mask content-addressed identifiers and `LC_ALL=C sort` before diffing or SPDX 3 array reordering will fake semantic hits. **Any component, relationship, license, or annotation change is a defect, not expected churn.** — done. SC-003: every fixture improved, none regressed. **SC-010: strip `externalReferences` and all five documents are byte-identical to baseline**; component and dependency counts unchanged. **FR-011: zero baseline references lost** (rust-ripgrep 61→179, all 61 originals retained). **FR-016 (F4 remediation) verified directly**: SPDX 2.3 `externalRefs[referenceType=vcs]`=9 and SPDX 3 `software_sourceInfo`=7 — research R5's no-emitter-changes claim confirmed rather than assumed.
- [X] T025 [US1] Verify SC-005 (determinism), SC-006 (wall time), and **FR-008 (inert when enrichment is disabled)** per quickstart Step 6. For FR-008, run one scan with `--offline` and confirm no enrichment-derived references appear and the scan completes normally. This check lives in US1's phase deliberately: its only other exercise is T031, which is in US2's phase, so FR-008 would lose coverage entirely if US2 were split off. Then: two scans byte-identical after masking; largest-fixture wall time within 3% of the T004 baseline. A larger regression suggests a network call crept in, which FR-007 forbids. — done. SC-005 deterministic ✓. SC-006 **16.34s vs 18.25s baseline** (faster; no regression). FR-008 (F5 remediation): `--offline` emits zero references and reports `total_references=0 skipped_unmapped_label=0 skipped_malformed_url=0`.

**Checkpoint**: US1 is independently shippable and delivers the milestone's measured value.

---

## Phase 4: User Story 2 - Offline-derivable distribution references (Priority: P2)

**Goal**: Deterministic `distribution` references for ecosystems whose registry URL is fully determined by the PURL, covering the offline path US1 cannot reach.

**Independent Test**: Scan with `--offline` and confirm `distribution` references appear while pre-existing `website` references remain. Per quickstart Step 5. Verifiable without US1.

- [X] T026 [US2] Determine which ecosystems qualify, **verifying each against its registry's documented URL scheme** rather than against sampled packages. An arm MUST NOT be added on the strength of a pattern that appears to work — a URL resolving for common packages but not edge cases is a fabricated reference under Principle IX. Record the verified list and the rejected candidates (with the reason each was rejected) in a comment at the function. Reference: research R7, contracts/derived-distribution-refs.md Contract 2. — done, **and it earned its place**. Verified each candidate against a live request. Results: cargo ✓ (initial 403 was a missing User-Agent, not a bad scheme), npm ✓ (plain and scoped), maven ✓, nuget ✓ **only via the v3 flat container with both id and version lowercased** — the v2 endpoint 404s and a mixed-case v3 path 404s. **pypi REJECTED**: `typing-extensions` (as PURL normalizes it) 404s while `typing_extensions` succeeds — the sdist filename uses the project's own spelling, which the PURL does not carry, and wheel-only projects publish no sdist. **golang DEFERRED**: the proxy scheme is valid but needs `!x` case-escaping, and Go already receives a `vcs` reference; marginal gain against a real transform risk.
- [X] T027 [US2] In `waybill-cli/src/scan_fs/mod.rs`, extend `external_refs_from_purl` (~line 1827) with a `distribution` arm per verified ecosystem from T026. Preserve the function's purity — no network, no filesystem, no clock (Contract 1). This is what lets US2 work under `--offline`. — done: four verified arms in `external_refs_from_purl`; purity preserved (no network, no filesystem).
- [X] T028 [US2] Preserve every existing reference (FR-011). The registry landing pages currently emitted as `website` for `cargo`, `nuget`, and nested-jar `maven` MUST remain; the distribution reference is added **alongside**, not in place of. This is the correction the measurement motivated: rust-ripgrep carried 61 references yet near-zero source coverage because they were all landing pages — the fix adds a correct kind rather than swapping one for another. — verified: FR-011 holds. rust-ripgrep 61 → 179 references with **zero originals lost**; the crates.io landing pages remain alongside the new distribution URLs.
- [X] T029 [US2] Emit nothing when the URL cannot be formed (FR-010): a PURL lacking a version, or an ecosystem whose scheme needs registry metadata, produces no distribution reference. — done, **and extended twice by T032's findings** beyond the specified versionless case. Also rejects placeholder versions (`unknown`, `NOASSERTION`, `latest`, …), `-SNAPSHOT` versions (never published to Maven Central), and main-module components (the project being scanned is not a registry artifact).
- [X] T030 [US2] [P] Add unit tests for Contracts 2/3/5: one test per added arm asserting the exact expected URL; a versionless PURL yielding nothing; a non-qualifying ecosystem yielding nothing; and at least one namespaced or scoped coordinate per arm asserting correct percent-encoding rather than naive concatenation. — passing: 12 tests including scoped-npm basename, nuget lowercasing, maven group-path, and the three FR-010 regression tests pinning the T032 findings.
- [X] T031 [US2] Verify SC-004 per quickstart Step 5: with `--offline`, an ecosystem that previously emitted no references now emits `distribution` references for the majority of its components, and pre-existing `website` references are still present. **Record the specific ecosystem verified and the observed proportion** (e.g. "npm: 361/369") in the task notes, so SC-004 is auditable after the fact rather than self-reported. SC-004 names no ecosystem by design — T026 picks it — but an unnamed fixture is how m774's single-workspace criterion came to be checked against a fixture that did not exercise the intended path. — done. **SC-004 satisfied by npm: 0/369 → 368/369 (100%)** offline. Post-gating: rust-ripgrep 51/68 (75%) and **maven 0/63** — the maven fixture is entirely top-level JARs, so none qualify after the provenance gate, and ripgrep's drop from 61 is the placeholder/main-module guards firing. Both numbers are lower and **more correct**; SC-004 is satisfied by npm regardless. go-cobra and py-uv are 0% by design (golang deferred, pypi rejected). Per the F3 remediation, the ecosystem and proportion are recorded here rather than left self-reported.
- [X] T032 [US2] Spot-check that at least one derived URL per arm actually resolves. A 404 means the arm was added on a pattern rather than a verified scheme — that is a fabricated reference and the arm must be removed (quickstart Step 5, rollback trigger 1). — done, **and it caught two real defects that reasoning had not**. (1) maven emitted `.../aopalliance/unknown/aopalliance-unknown.jar` — a live 404 from the literal placeholder version `unknown`. (2) maven emitted `.../com/example/guice-demo/1.0-SNAPSHOT/...` — a live 404 from a locally-built SNAPSHOT artifact. Both fixed in T029 and pinned by regression tests. Final broad probe: **15/15 emitted URLs resolve, 0 dead.**

**Checkpoint**: US2 complete. Both stories delivered.

---

## Phase 5: Polish & Cross-Cutting Concerns

- [X] T033 Run `./scripts/pre-pr.sh` — the mandatory gate (SC-008). Per memory `feedback_prepr_gate_full_output`, read the full per-suite "N passed; 0 failed" lines rather than trusting a failure-grep; per `feedback_prepr_gate_bails_on_first_failure`, re-run with `--no-fail-fast` and enumerate every failing target before concluding. Note the known pre-existing `m203_us2_5_env_var_override_shortens_timeout` helm-timing flake (memory `reference_m203_helm_test_flake`) — re-run in isolation before treating it as a regression. — see below; three gate runs were needed.
- [X] T034 **Watch the parity suite specifically.** Catalog rows A9 (homepage), A10 (vcs), and A11 (distribution) already have extractors that today compare empty against empty; this milestone makes them exercise real data across all three formats for the first time (research R6). A failure there is a genuine cross-format mapping discrepancy — investigate it as such rather than as a surprise. Note the accepted asymmetry: SPDX 3 has scalar slots only for vcs/website/distribution, so `issue-tracker`, `documentation`, and `attestation` fall through its `_ => {}` arm by design (research R5) and MUST NOT be "fixed" by removing those kinds from CycloneDX and SPDX 2.3. — parity suite passed on all runs. The A9/A10/A11 extractors now exercise real data across all three formats for the first time and revealed no cross-format discrepancy.
- [X] T035 [P] Confirm SC-007 (zero new dependencies): `git diff Cargo.toml Cargo.lock waybill-cli/Cargo.toml` returns empty. Confirm FR-014 (no new operator surface): `cargo run -p waybill -- sbom scan --help` is unchanged from the T002 baseline binary. — done: `git diff` on all four manifests and the lockfile is empty ✓.
- [X] T036 [P] Confirm no catalog row was added (research R6). Rows A9/A10/A11 are populated, not created. Rows for `issue-tracker`, `documentation`, and `attestation` MUST NOT be added — they would require parity extractors, which would require SPDX 3 per-package `ExternalRef` emission first, and rows ahead of emission code is exactly what the extractor gate forbids (memory `feedback_sbom_format_mapping_extractor_gate`). — done: `docs/reference/sbom-format-mapping.md` unchanged ✓. A9/A10/A11 populated, not created; no rows added for issue-tracker/documentation/attestation (they would need SPDX 3 emission first).
- [X] T037 [P] Write memory file `reference_m776_component_source_refs.md` and add a one-line pointer to `MEMORY.md`. Record: that deps.dev's `links[]` was already fetched and discarded; the five-label mapping with `ORIGIN` deferred; the measured upstream availability (npm 100%, pypi 93%); that all three emitters already consumed `external_references` so no emitter work was needed; and the transferable lesson — sampling an external vocabulary before mapping it caught two labels the spec draft had assumed away. — done: `reference_m776_component_source_refs.md` + `MEMORY.md` pointer.
- [ ] T038 Remove the baseline worktree: `git worktree remove /tmp/waybill-main`.
- [ ] T039 Commit with a message citing the measured before/after coverage, both user stories, and the note that no emitter or catalog row changed. Do not push until the user reviews.
- [ ] T040 [P] Open the PR against `main` citing SC-001 through SC-010 status, the coverage table, and the accepted SPDX 3 asymmetry with its deferral rationale.

---

## Dependencies

Phase 1 → Phase 2 → (Phase 3 ∥ Phase 4) → Phase 5.

**Phase 2 blocks both stories** — dedup (FR-006) and ordering (FR-013) are properties of the *combined* reference set, so the shared helper cannot be split per story.

**US1 and US2 are otherwise independent** and may proceed in either order, subject to one caveat: T016's summary derives per-kind counts from the final component set, so if US2 lands first its distribution references appear in the counts automatically — no US2 summary work is needed either way.

Within US1: T008–T014 (mapping) → T015–T016 (observability) → T017–T021 (tests) → T022–T025 (empirical, needs a release build).

Within US2: T026 (verification, gates everything) → T027–T029 (arms) → T030 (tests) → T031–T032 (empirical).

Within Phase 5: T033 blocks T039 (do not commit on a red gate); T039 blocks T040. T035–T037 are parallel.

## Parallel execution examples

**Phase 2**: T006 and T007 after T005 lands the helper shape.

**US1 tests**: T018, T019, T020 are independent assertions (T017 first — it establishes the harness shape the others reuse).

**Phase 5**: T035, T036, T037 run in parallel while T033/T034 gate T039.

## Implementation strategy (MVP-first)

**MVP = Phase 2 + US1.** US1 carries the entire measured value — py-uv ~1/109 → ≥80%, npm-nodejs 0/369 → ≥80% — consumes data already being fetched and discarded, and delivers the observability. US2 is the offline complement; it can ship alongside or split into its own milestone without rework.

Suggested order:
1. Phase 1 (T001–T004): ~30 min. Baselines.
2. Phase 2 (T005–T007): ~45 min. Shared helper; small but blocking.
3. Phase 3a–3b (T008–T016): ~2 h. The load-bearing change.
4. Phase 3c (T017–T021): ~1.5 h. Tests.
5. Phase 3d (T022–T025): ~45 min. Empirical validation.
6. Phase 4 (T026–T032): ~2 h. T026's per-registry verification is the slow part and should not be rushed — it is the task that prevents fabricated references.
7. Phase 5 (T033–T040): ~1 h.

Total ≈ 8.5 hours. Budget extra for T034: the parity suite is the most likely place to fail first, and a failure there needs genuine cross-format investigation rather than a quick fix.
