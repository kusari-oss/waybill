# Spec Quality Checklist: OCI Registry Image Scan

**Checklist for** `/specs/031-oci-registry-image-scan/spec.md`

## Coverage

- [X] Background section explains the UX gap (syft/trivy
      take refs directly, mikebom takes only tarballs) + cites
      file:line evidence for the integration seam (`scan_fs/docker_image.rs:70`
      for extract; `scan_cmd.rs:429` for current --image
      dispatch; `Cargo.toml:15` for reqwest+rustls).
- [X] User story has a P-priority (P1 — UX gap) and a "why
      this priority" justification grounded in real-world
      adoption friction.
- [X] Independent Test is concrete (specific build commands +
      observable end-to-end behavior).
- [X] Acceptance scenarios use Given/When/Then framing (5
      scenarios covering anon pull, path-vs-ref dispatch,
      feature-off error, registry-failure error, and
      multi-platform).
- [X] Edge Cases section names corner cases (no-tag default,
      digest refs, registry hostnames, gzipped layers, layer
      digest verification, anonymous-only scope, multi-arch
      host-arch-only, async runtime bridge).
- [X] Functional Requirements numbered (FR-001 through FR-013).
- [X] Key Entities — `oci_pull::pull_to_tarball` signature
      specified inline in FR-002.
- [X] Success Criteria measurable (SC-001 through SC-008),
      each with a verification mechanism.
- [X] Clarifications section captures the 6 scope decisions
      (default-off feature gate; pull-then-scan vs streaming;
      anonymous-only deferral; host-arch-only deferral; no
      caching; async-to-sync bridge).
- [X] Out of Scope explicitly names every adjacent concern with
      named follow-on milestones (031.x / 031.y / 031.z) and
      indefinite-defer items (zstd layers, daemon socket,
      streaming, signature verification).

## Tighter spec set rationale (4 files vs 8)

- [X] No `research.md` — the dep-choice analysis (`oci-client`
      rationale + rejected alternatives) lives in plan.md
      Architecture / Crate choice section. The recon was
      thorough enough to commit to a crate without a separate
      research doc.
- [X] No `data-model.md` — only one new public fn signature +
      one helper struct kind (an `ImageArgKind` enum), specified
      inline in FRs.
- [X] No `contracts/` — the only public-API change is the CLI
      `--image` flag's expanded shape, fully specified in FR-004.
- [X] No `quickstart.md` — 4 short files self-explanatory.

This is the **10th use** of the 4-file template (after 021,
022, 023, 024, 025, 026, 028, 029, 030). Pattern fully
validated even for milestones with new feature gates and new
crate deps.

## Independence

- [X] Single user story self-contained.
- [X] Each per-commit deliverable (3 commits) independently
      verifiable (per FR-013 each commit's pre-PR passes both
      build profiles).
- [X] **Sub-scoping discipline**: anonymous-only / host-arch-only
      keep this milestone shippable in ~3 days. Auth + multi-arch
      / caching tracked as 031.x / 031.y / 031.z. Avoids the
      milestone-grows-under-foot pattern.

## Concreteness

- [X] FRs cite specific file paths and line numbers.
- [X] FR-001 names the exact Cargo feature name (`oci-registry`)
      and the exact dep entry shape.
- [X] FR-002 names the exact public-fn signature.
- [X] FR-004 specifies the dispatch logic with the four
      branches enumerated.
- [X] FR-007/008/009 name the exact error-message shape for
      the three categories (auth not supported, multi-arch
      missing match, zstd layers).
- [X] SC-005 quantifies the dep-audit bar (no `*-sys` / `*-c`
      transitive deps surface).
- [X] SC-008 names the verification image
      (`gcr.io/distroless/static-debian12:latest`) — chosen for
      stability + small size + permissive license + public
      anonymous availability.

## Internal consistency

- [X] FR-002 (oci_pull module) + FR-004 (CLI dispatch) + FR-005
      (TempDir lifetime) + FR-006 (error propagation) flow
      end-to-end.
- [X] Deferred items in Out of Scope (031.x auth, 031.y
      multi-arch flag, 031.z caching) align with FR-007 / FR-008
      / FR-009's "not yet supported" error messages — users see
      the same scope boundary in the error path that the spec
      documents in prose.
- [X] Edge Case "async runtime bridge" aligns with plan.md's R4
      and the FR-002 implementation sketch in tasks T010.

## Lessons from milestones 016-030

- [X] FR-013 carries the per-commit-clean discipline,
      extended to dual-profile (default + feature-on).
- [X] **Feature-gate precedent**: matches milestone 020's
      `ebpf-tracing` feature gate. Same shape: opt-in capability,
      transitive deps gated, default profile stays slim.
- [X] **Dep-discipline lesson from 029**: 029 was zero-new-deps;
      this milestone introduces a new dep but with a feature
      gate. The audit step (T006) preserves Principle I
      enforcement.
- [X] **Sub-scope discipline lesson from 026**: 026 explicitly
      sub-scoped from 7 libraries to easy-4 with the rest as
      026.x. This milestone applies the same pattern (anon-only
      / host-arch-only with 031.x and 031.y as named follow-ons).
- [X] Recon-first: every claim grounded in pre-spec recon at
      file:line level.

## Pre-implementation

- [X] [PHASE-1] T001 reconnaissance done (2026-04-28).
- [ ] [PHASE-1] T002 baseline snapshot captured.
- [ ] [PHASE-2] Commit 1 (feature gate + dep audit + stub) landed.
- [ ] [PHASE-3] Commit 2 (oci_pull module + CLI dispatch + tests)
      landed.
- [ ] [PHASE-4] Commit 3 (smoke test + docs) landed.
- [ ] [POLISH] SC-001-SC-008 verified.
- [ ] [POLISH] All 3 standard CI lanes green on default profile.
- [ ] [POLISH] Manual end-to-end smoke test against
      `gcr.io/distroless/static-debian12:latest` succeeds.

## Post-merge

- [ ] [QUALITATIVE] Next time someone wants to demo mikebom
      against a public image, they run
      `cargo install mikebom --features oci-registry` then
      `mikebom sbom scan --image alpine:3.19` — same UX as
      `syft alpine:3.19`. If yes, milestone delivered.
- [ ] [DEFERRED FOLLOW-ONS] 031.x (auth) is the highest-priority
      next step since most real-world container scanning involves
      private registries.
