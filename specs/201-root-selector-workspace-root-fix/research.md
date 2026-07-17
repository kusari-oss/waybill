# Research: Root-Selector Workspace-Root Disambiguation

**Date**: 2026-07-17
**Purpose**: Resolve 4 mechanical unknowns before task decomposition.

## R1 — Fix strategy selection

**Investigation**: Four candidate fixes for the `is_workspace_root` collision on cargo-augmented main-modules:

| Option | Approach | Blast Radius | Complexity | Preferred? |
|---|---|---|---|---|
| A | Extend cargo m064 to append per-crate Cargo.toml to `evidence.source_file_paths`. Update is_workspace_root stamping to iterate paths looking for Cargo.toml specifically. | Medium — augmented entries change shape; may drift other tests observing source_file_paths | Medium — dedup needs to preserve per-crate paths through augment | ✗ |
| B | New internal-only annotation `mikebom:is-cargo-workspace-toplevel` set at cargo m064 emission time on the crate whose Cargo.toml has a `[workspace]` block. is_workspace_root stamping honors it as a positive-identifier override. | Small — internal annotation, filtered from SBOM output, adjacent to existing `mikebom:is-workspace-root` machinery | Small — 3-site change, clean semantic | ✓ |
| C | Reshape the m127 root-selector: when multiple workspace_root_modules exist, pick ecosystem-priority AMONG them (not fall back to ecosystem-priority across ALL main-modules). | Small in code, but doesn't solve the alphabetical tie-break problem — macros still beats vaultwarden within cargo | Small | ✗ |
| D | Have the cargo reader emit `mikebom:is-workspace-root` DIRECTLY at emission time on the workspace top-level crate, bypassing scan_fs/mod.rs stamping for cargo mainmods. | Medium — bifurcates the existing centralized stamping site into per-ecosystem responsibility | Medium — needs coordination between two stamping paths | ✗ |

**Decision**: Option B — new internal-only annotation `mikebom:is-cargo-workspace-toplevel`.

**Rationale**:
- Positive-identifier signal is unambiguous: it fires only when the parsed Cargo.toml contains BOTH `[package]` AND `[workspace]` blocks.
- Adjacent to existing `mikebom:is-workspace-root` machinery — same filter treatment via `is_internal_emission_key`, same consumer at `scan_fs/mod.rs:944-947`.
- Zero wire-format impact per Constitution Principle V (internal-only annotation).
- Preserves existing stamping logic for non-cargo main-modules — no regression risk for npm, python, maven, etc.
- Single-crate cargo projects (no `[workspace]` block) don't emit the annotation, and the filesystem-based logic correctly identifies them as workspace roots (their Cargo.toml IS at rootfs).

**Alternatives considered + rejected**:
- Option A rejected: augmented-entry shape changes propagate through dedup and may drift `evidence.source_file_paths`-observing goldens.
- Option C rejected: doesn't address the fundamental "we can't tell which cargo mainmod is at rootfs" problem — just shuffles which alphabetical tie-break fires.
- Option D rejected: violates the centralized-stamping-site convention that scan_fs/mod.rs owns for cross-ecosystem consistency.

**References**:
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:363-430` — `build_cargo_main_module_entry`.
- `mikebom-cli/src/scan_fs/mod.rs:922-947` — `is_workspace_root` stamping.
- `mikebom-cli/src/generate/root_selector.rs:437-439` — `is_internal_emission_key`.

## R2 — Workspace-toplevel detection at cargo reader time

**Investigation**: `build_cargo_main_module_entry` at `cargo.rs:363-430` already parses the Cargo.toml text into `parsed: toml::Value` (line 368). Detecting the `[workspace]` block is a single `parsed.get("workspace").is_some()` check.

**Decision**: In `build_cargo_main_module_entry`, after successfully parsing `[package].name`, check `parsed.get("workspace").is_some()`. When true, stamp `extra_annotations["mikebom:is-cargo-workspace-toplevel"] = json!(true)`. When false (workspace-member crate or standalone single-crate that happens to have no [workspace]), no stamp.

**Grammar note**: A standalone single-crate project has `[package]` but no `[workspace]`. Post-fix, standalone single-crate projects do NOT get the new annotation — and the existing filesystem-based is_workspace_root logic still correctly identifies them as workspace root because their Cargo.toml IS at rootfs. So single-crate scans are unaffected (FR-004 preserved).

**Alternatives considered + rejected**:
- Detect via a workspace_ctx lookup that already knows the workspace top-level: rejected — `workspace_ctx` at `cargo.rs:363` is a different concept (workspace inheritance for `[workspace.package]` metadata); checking the Cargo.toml directly is more explicit.
- Use m200's `root_names` set: rejected — that set contains EVERY workspace member's name, not just the top-level's. Would need additional filter to identify the top-level.

**References**:
- `mikebom-cli/src/scan_fs/package_db/cargo.rs:368` — `parsed: toml::Value`.
- Cargo grammar: [workspace] and [package] can coexist at manifest root (workspace root that is ALSO a distributable crate); [workspace] alone = virtual workspace; [package] alone = single-crate project.

## R3 — is_workspace_root stamping consumer

**Investigation**: The stamping at `scan_fs/mod.rs:922-942` currently derives `is_workspace_root` from `manifest_parent == canonical_root` comparison. The new annotation from R2 provides a positive-identifier signal for cargo.

**Decision**: Before the existing filesystem-based check at `scan_fs/mod.rs:925-942`, check `c.extra_annotations.get("mikebom:is-cargo-workspace-toplevel").and_then(|v| v.as_bool()).unwrap_or(false)`. When true, short-circuit `is_workspace_root = true` and skip the filesystem check. When false (or absent), fall through to the existing filesystem-based logic — preserving all non-cargo behavior byte-identically.

```rust
// Milestone 201 (FR-001, closes #587): honor cargo reader's
// positive-identifier signal. When the cargo reader stamped
// `mikebom:is-cargo-workspace-toplevel: true` on this component (its
// Cargo.toml had both [package] AND [workspace] blocks), short-circuit
// the shared-Cargo.lock-path collision that fools the filesystem
// check below into tagging ALL cargo mainmods as workspace roots.
let is_cargo_workspace_toplevel = c
    .extra_annotations
    .get("mikebom:is-cargo-workspace-toplevel")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
let is_workspace_root = if is_cargo_workspace_toplevel {
    true
} else {
    // Existing filesystem-based logic (unchanged).
    match (manifest_path, canonical_root.as_ref()) { ... }
};
```

**Semantic guarantee**: For CARGO main-modules, at most ONE per workspace scan gets `is_workspace_root = true` post-fix (the crate whose Cargo.toml has [workspace]). For NON-CARGO main-modules, the existing filesystem logic runs unchanged — an npm project at rootfs still gets is_workspace_root=true via the parent-dir check.

**Alternatives considered + rejected**:
- Consume the annotation AS the filesystem-check fallback (only when filesystem check returns false): rejected — the cargo case fires the filesystem check TRUE for ALL cargo mainmods (all share the workspace Cargo.lock at rootfs), so the fallback wouldn't help.
- Route the signal through the m127 root-selector directly (bypass `mikebom:is-workspace-root` entirely): rejected — the existing RepoRoot ladder consumer is the correct single-point-of-consumption; adding a parallel signal would fork the ladder.

**References**:
- `mikebom-cli/src/scan_fs/mod.rs:922-947` — the stamping site.
- `mikebom-cli/src/generate/root_selector.rs:243-250` + `304-309` — the RepoRoot ladder + `is_workspace_root` reader.

## R4 — Golden regen scope (post-m200 empirical-verification convention)

**Investigation** (pre-implementation grep of existing goldens for cargo workspace-root shape):

```bash
grep -rlE '"pkg:cargo/[^"]+"[[:space:]]*,?[[:space:]]*"type"[[:space:]]*:[[:space:]]*"application"' mikebom-cli/tests/fixtures/
```

Findings:
- No pre-existing fixture reproduces the cargo-workspace-with-[package]-at-root pattern from #587. The m200 fixture (`root_package_lifecycle/`) is the closest; extending it (rather than creating new) minimizes fixture bloat.
- The rust-ripgrep public-corpus golden has `pkg:generic/rust-ripgrep` as `metadata.component` (synthetic m195 anchor, not a cargo crate) — orthogonal to m201.
- No existing golden has both `[package]` + `[workspace]` at Cargo.toml root and multiple cargo main-modules where root-election would flip post-m201.

**Decision**: **Empirically likely: 0 golden regen files.** At implement time, re-run:
1. `cargo test --workspace --no-fail-fast` post-fix — any golden-based test failure identifies regen scope.
2. `git diff --stat mikebom-cli/tests/fixtures/` — the ONLY expected drift is the new `sub/package.json` file in the extended fixture.
3. Bonus check: manually scan test-vaultwarden post-fix and confirm `metadata.component.purl == "pkg:cargo/vaultwarden@1.0.0"` (SC-001 live verification, not a golden regen).

Following the m199/m200 empirical-verification lesson: this claim is TREATED AS UNVERIFIED until implement-time re-audit.

**Alternatives considered + rejected**:
- Preemptively regen all cargo goldens: rejected — no expected drift; would bulk the PR unnecessarily.
- Create a fresh fixture separate from m200's: rejected — extending the existing m200 fixture is smaller-diff and topically related.

**References**:
- Memory `feedback_verify_research_empirical_claims`.
- `mikebom-cli/tests/fixtures/cargo/root_package_lifecycle/` — the m200 fixture to extend.

## Decision Summary

| Decision | Chosen | Alternative | Rationale |
|---|---|---|---|
| Fix strategy | Option B — new internal-only annotation `mikebom:is-cargo-workspace-toplevel` | Options A/C/D | Small blast radius, positive identifier, adjacent to existing machinery |
| Detection site | `build_cargo_main_module_entry` (`cargo.rs:363`) via `parsed.get("workspace").is_some()` | m200's `root_names` post-processing | Single-source-of-truth at emission time; unambiguous grammar check |
| Consumption site | `scan_fs/mod.rs:922-947` `is_workspace_root` stamping short-circuit | m127 RepoRoot ladder direct route | Preserves single-annotation single-consumer architecture |
| Emission filter | Extend `is_internal_emission_key` at `root_selector.rs:437-439` | New filter | Matches existing `mikebom:is-workspace-root` treatment |
| Fixture strategy | Extend m200's `root_package_lifecycle/` with a nested npm sub-project | New standalone fixture | Smaller diff, topically related, matches m200 lineage |
| Golden regen scope | Empirically 0 files (verified at implement time) | Preemptive full-corpus regen | Zero-drift expected; matches m200's outcome |
| New Cargo deps | Zero | (n/a) | Nothing needed |
