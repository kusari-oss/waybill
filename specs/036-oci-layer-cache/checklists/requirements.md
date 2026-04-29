# Spec Quality Checklist: OCI layer cache (031.z)

**Checklist for** `/specs/036-oci-layer-cache/spec.md`

## Coverage

- [X] Background cites the file:line seam (`registry.rs:91-102`
      `fetch_blob`).
- [X] User story has P-priority (P1 — workflow-critical for
      iterative scans on bigger images).
- [X] Independent Test is concrete (specific commands + observable
      timing + cache-hit log assertion).
- [X] 6 acceptance scenarios cover cold→warm, corruption recovery,
      `--no-oci-cache`, eviction, dir override, concurrent-write
      stress.
- [X] Edge Cases name read-only-FS, mid-write-disk-full, non-sha256
      digests, eviction-of-active-files, symlink-safety.
- [X] FR-001 through FR-009 numbered, each with file paths +
      signatures.
- [X] SC-001 through SC-007 measurable with explicit verification
      commands.
- [X] Out of Scope names every adjacent concern (manifest cache,
      distributed cache, --clear-oci-cache, pre-warm).

## Tighter spec set rationale

- [X] No `research.md` — recon answered every architectural
      question; the `fetch_blob` seam is well-understood from the
      034/035 work.
- [X] No `data-model.md` — `Cache { dir, size_cap }` is fully
      specified inline in FR-001.
- [X] No `contracts/` — public surface unchanged beyond two new
      CLI flags + a parameter on `pull_to_tarball`.
- [X] No `quickstart.md` — 4 short files self-explanatory.

This is the sixth use of the 4-file template (after 021, 022, 023,
034, 035). Pattern stable.

## Concreteness

- [X] FRs cite specific file paths and exported items.
- [X] FR-002 names the exact env-var precedence chain.
- [X] FR-006 names the env-var fallbacks for both flags.
- [X] SC-005 quantifies LOC ceiling (500).

## Internal consistency

- [X] FR-001 (Cache surface) aligns with FR-005 (RegistryClient
      uses it) aligns with FR-007 (pull_to_tarball threads it).
- [X] FR-003 (digest validation) aligns with Edge Case "non-sha256"
      handling.
- [X] FR-008 (no new deps) aligns with SC-006 (Cargo.toml diff
      empty).
- [X] R2 (NamedTempFile cross-FS) aligns with FR-001's "atomic
      rename" claim — mitigation is `NamedTempFile::new_in(&dir)`.

## Lessons from 016-035

- [X] Per-commit-clean discipline carried through (FR-009).
- [X] The cache wraps `fetch_blob`'s existing verify_sha256 — same
      pattern as auth.rs wrapped fetch_bearer_token. Reuse over
      reinvention.
- [X] Mock-server test using tokio TCP listener directly (no new
      crate) — same playbook as the 034 wire-up tests.
- [X] Smoke test gating aligns with 031/034/035 conventions
      (`MIKEBOM_OCI_NETWORK_TESTS=1`).
- [X] Recon-first: every claim backed by file:line refs from the
      just-read registry.rs / mod.rs / scan_cmd.rs.

## Pre-implementation

- [X] [PHASE-1] T001 reconnaissance done.
- [ ] [PHASE-1] T002 baseline snapshot.
- [ ] [PHASE-2] Commit 1 (cache module) landed.
- [ ] [PHASE-3] Commit 2 (wire-cache) landed.
- [ ] [PHASE-4] Commit 3 (docs + smoke) landed.
- [ ] [POLISH] SC-001-SC-007 verified.
- [ ] [POLISH] All 3 CI lanes green.

## Post-merge

- [ ] [QUALITATIVE] Iterate on a non-trivial image
      (`ubuntu:24.04`). Second scan should be visibly faster than
      the first. If yes, milestone delivered.
- [ ] [FOLLOW-ON] OCI follow-on queue is now exhausted (031.x #66,
      031.y #67, 031.z #68 all closed). Next: #64 (dpkg
      `status.d/` for distroless) or a new architectural surface.
