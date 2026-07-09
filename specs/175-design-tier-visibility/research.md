# Phase 0 Research: Design-tier component visibility (m175)

**Feature**: 175-design-tier-visibility
**Date**: 2026-07-09

Five research questions resolved by inspection of the existing codebase + audit of prior-milestone advisory precedents. Every question was answerable without spawning subagents — the pattern-matches all trace to existing milestones (m047/m081 lifecycle-phase, m110 deprecation-notice, m173/m176 advisory-log).

---

## R1 — Which readers already emit `sbom_tier = "design"`?

**Decision**: **Broad coverage — 11 readers today** (spot-verified 2026-07-09 via `grep -rn 'sbom_tier.*"design"' mikebom-cli/src/scan_fs/package_db/`):

| Ecosystem | Reader file | Constraint-only trigger |
|---|---|---|
| pip | `pip/requirements_txt.rs` | `requirements*.txt` at directory root, no matching `pyproject.toml`/`uv.lock`/`poetry.lock` |
| Cargo | `cargo.rs` | Manifest-only (`Cargo.toml` without `Cargo.lock`) |
| Ruby | `gem.rs` | `Gemfile` without matching `Gemfile.lock` |
| npm | `npm/package_json.rs` | Root `package.json` without matching lockfile |
| Maven | `maven.rs` | `pom.xml` `<dependencyManagement>` entries without version pins |
| Composer | `composer.rs` | `composer.json` without `composer.lock` |
| CocoaPods | `cocoapods.rs` | `Podfile` without `Podfile.lock` (rare — Ruby DSL) |
| Erlang | `erlang.rs` | `rebar.config` without `rebar.lock` |
| Scala | `scala.rs` | `build.sbt` deps without `build.sbt.lock` |
| Haskell | `haskell.rs` | Cabal file without `cabal.project.freeze` / `stack.yaml.lock` |
| Dart | `dart.rs` | `pubspec.yaml` without `pubspec.lock` |

**Implication for m175**: the advisory-log predicate `design_tier_count > 0` reads the SAME `Option<String>` field across every reader. Zero reader-side changes. The remediation docs need per-ecosystem entries (pip / npm / Cargo / Ruby minimum per FR-008; others as bandwidth allows).

**Alternatives considered**:
- **Emit a per-ecosystem advisory** (Python-only if pip-only, Ruby-only if Ruby-only, etc.): rejected. The advisory is operator-UX, not diagnostic granularity. One line, one grep-substring, one docs anchor is easier to CI-integrate.
- **Add a new `sbom_tier` value** (`"design-unresolved"` vs `"design-resolved"`): rejected. The tier vocabulary is stable across milestones; splitting it would ripple through m047/m081/m127 etc.

---

## R2 — What's the FR-005 suppression signal choice?

**Decision**: **`MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` env var only.** No new CLI flag.

**Rationale**:
1. **Precedent**: milestone 110's deprecation-notice suppression uses `MIKEBOM_NO_DEPRECATION_NOTICE=1` (env-var only). Same operator-UX category (at-scan-time diagnostic → env-var opt-out). m175's suppression fits the same shape.
2. **CLI-flag surface conservatism**: mikebom currently has ~30 CLI flags. Adding one per advisory over time creates surface bloat. Env-var opt-outs stay out of `--help` and don't interact with completion.
3. **Testability**: env vars are trivially set in integration tests via `Command::env()`; no clap-argument-parsing scaffolding needed.
4. **Consolidation path**: if two more advisory suppressions accrue, a follow-up milestone can introduce a unified `--no-advisories` flag OR a `MIKEBOM_NO_ADVISORIES=1` env var that suppresses all. Deferred until proven need.

**Alternatives considered**:
- **New CLI flag `--no-design-tier-advisory`**: rejected. CLI surface bloat + no functional gain over env var + no operator has asked for shell-completion of this specifically.
- **Reuse an existing flag** (`--quiet` or `--suppress-hints`): rejected. Neither exists; `--quiet` would silence everything (too broad); `--suppress-hints` doesn't exist and creating it is itself a design decision worthy of a separate milestone.
- **No suppression at all**: rejected. CI pipelines that intentionally scan constraint-only projects (e.g., linters running against templates) would generate log noise on every run.

---

## R3 — Where does the advisory log fire in `scan_cmd.rs`?

**Decision**: **At the emission-tail site, immediately after the m176 `monorepo shape detected` advisory block, before the final `SBOM written` `tracing::info!` line.**

**Verified via**: `scan_cmd.rs:2894–2942` (m173 advisory block) + `scan_cmd.rs:2943–2985` (m176 advisory block per commit c1b8ed5). The site is the canonical "final diagnostics before scan-emission-complete" location.

**Predicate composition**:
```rust
let design_tier_count = components
    .iter()
    .filter(|c| c.sbom_tier.as_deref() == Some("design"))
    .count();
let suppress = std::env::var("MIKEBOM_NO_DESIGN_TIER_ADVISORY")
    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    .unwrap_or(false);
if design_tier_count > 0 && !components.is_empty() && !suppress {
    tracing::info!(
        "design-tier components detected: {} components lack resolved versions. \
         Remediation: generate a lockfile (uv lock / poetry lock / pip-compile / \
         npm install / bundle lock / cargo generate-lockfile) OR install into a \
         venv and re-scan. See docs/reference/reading-a-mikebom-sbom.md for jq \
         recipes.",
        design_tier_count,
    );
}
```

**Stable grep substring**: `"design-tier components detected: "` (includes the trailing space + colon for token-boundary clarity; matches the m176 `"monorepo shape detected: "` substring shape).

**Alternatives considered**:
- **Fire per reader** (each pip reader logs when it emits a design-tier entry): rejected. Would produce N log lines for N design-tier entries — noise. Also would require plumbing the advisory-fire signal up through the reader trait, which the current architecture doesn't have.
- **Fire from a new helper module** (e.g., `generate::design_tier_advisory`): rejected. m173/m176 kept their advisory blocks inline in `scan_cmd.rs`; consistency wins. A helper is warranted if a third advisory needs the same shape — deferred.

---

## R4 — How is the `KEEP-NATIVE-FIRST` tag polarity introduced without breaking existing docs consumers?

**Decision**: **New tag string appearing verbatim in ONE row of `sbom-format-mapping.md`.** No schema change to the doc; no existing row is modified.

**Rationale**: `sbom-format-mapping.md` is a Markdown reference table with a free-text "Justification" column. Existing rows are tagged `KEEP-NO-NATIVE` (rejected native alternatives) or leave the justification untagged (structural / conventional entries). Adding `KEEP-NATIVE-FIRST` as a new tag string in the same column is additive — pre-175 docs consumers reading the file see one new tag, no format churn.

**SC-007 gate**: `grep -n KEEP-NATIVE-FIRST docs/reference/sbom-format-mapping.md` returns exactly one match (the new m175 row). The tag polarity is now discoverable by future contributors doing Principle V audits — they'll find prior-art on both polarities:
- `KEEP-NO-NATIVE`: existing native construct rejected as semantically-different; introduced `mikebom:*` (e.g., m173's C118/C119).
- `KEEP-NATIVE-FIRST`: existing native construct accepted; explicitly did NOT introduce `mikebom:*` (m175 is the first).

**Alternatives considered**:
- **Introduce the tag as a `## Section J — KEEP-NATIVE-FIRST audits` new section**: rejected. Would fragment the audit-record content across two sections. One row in the existing Section C alongside its wire-shape peers keeps the audit adjacent to the emission code.
- **Reuse `KEEP-NO-NATIVE` inverted** (e.g., `KEEP-NATIVE-NO-INVENTION`): rejected. Cognitive overhead — the reader has to remember which polarity is which. Two clearly-named tags with opposite semantics is cleaner.

---

## R5 — Which golden fixtures produce design-tier components today, and do they need advisory-suppression during regression tests?

**Decision**: **Zero golden fixtures need advisory-suppression during regression tests.** The regression tests use `MIKEBOM_FIXED_TIMESTAMP` and other env vars for byte-identity but never assert on stderr contents — stderr is captured for diagnosis on failure only. The advisory log lands on stderr; SBOM output is unchanged.

**Verified via**: `mikebom-cli/tests/cdx_regression.rs` + `spdx_regression.rs` + `spdx3_regression.rs` all diff against pre-recorded fixture bytes on stdout / output file, never on stderr. Adding a new `tracing::info!` line under a predicate that fires for the `pip` and `cocoapods` fixtures (both of which have design-tier components) does NOT affect their byte-identity.

**Implication for SC-006**: byte-identity guarantee holds trivially. Zero golden regeneration required.

**Alternatives considered**:
- **Wrap the advisory in `if !cfg!(test) { ... }`**: rejected. Would silence the advisory during integration tests where we DO want to assert on it (the m175 integration test EXPLICITLY needs the advisory to fire).
- **Auto-suppress under `MIKEBOM_FIXED_TIMESTAMP` being set** (a proxy for "we're in a regression test"): rejected. Coupling advisory-suppression to a determinism env var is semantically confused; the two are orthogonal.

---

## Summary table

| ID | Question | Decision |
|---|---|---|
| R1 | Which readers tag design-tier? | 11 ecosystems today; same field on `ResolvedComponent`; zero reader changes |
| R2 | FR-005 suppression mechanism? | Env var `MIKEBOM_NO_DESIGN_TIER_ADVISORY=1` (matches m110 precedent) |
| R3 | Where in scan_cmd.rs? | Emission-tail after m176 block, before "SBOM written"; stable substring `"design-tier components detected: "` |
| R4 | KEEP-NATIVE-FIRST tag introduction? | Additive string in one new Section-C row; existing rows unchanged |
| R5 | Golden fixtures need suppression? | No; regression tests never assert on stderr |
