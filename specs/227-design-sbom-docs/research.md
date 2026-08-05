# Research: design-tier documentation surface

**Feature**: 227-design-sbom-docs
**Phase**: 0 (research)
**Date**: 2026-08-05

This phase is inventory-heavy — the goal is a verified per-reader map of design-tier trigger conditions, PURL shapes, and annotation values so the doc-writing phase can cite facts rather than recollections (spec FR-010 + memory `feedback-verify-research-empirical-claims`).

## §A — Cross-reader design-tier fallback inventory

### Decision: 18 readers currently emit `sbom_tier: Some("design")`

Grep evidence:

```bash
grep -rln 'sbom_tier: Some("design"' waybill-cli/src/scan_fs/package_db/
```

Confirmed readers (as of commit HEAD 2026-08-05):

| Reader source | Trigger condition (as coded today) |
|---|---|
| `cocoapods.rs` | Podfile / Podspec parsed, no matching Podfile.lock |
| `composer.rs` | composer.json parsed, no matching composer.lock |
| `dart.rs` | pubspec.yaml parsed, `--include-declared-deps` gate context — declared dep with no pub-hosted resolution |
| `elixir.rs` | mix.exs declared dep, no matching mix.lock |
| `erlang.rs` | rebar.config declared dep, no matching rebar.lock |
| `haskell.rs` | package.yaml / .cabal declared dep, no matching stack.yaml.lock or cabal.project.freeze |
| `helm.rs` | Chart.yaml dependency block, `--helm-render` OFF (default) — chart-tier declarations without resolved OCI subcharts |
| `kotlin_dsl/build_script.rs` + `mod.rs` | build.gradle.kts DSL-parsed dep, gated by `--include-declared-deps` flag |
| `maven.rs` | pom.xml declared dep with no `<version>` (inherited scope; unresolvable without mvn subprocess) |
| `npm/mod.rs` + `walk.rs` | package.json dep, no matching lockfile entry (package-lock.json / pnpm-lock.yaml / yarn.lock) |
| `nuget/mod.rs` | csproj / Directory.Build.{props,targets} / Directory.Packages.props declaration, resolution ladder exhausted (post #653) |
| `pants_go/mod.rs` | `pants.toml` `[golang] expected_version` — synthetic design-tier `pkg:generic/go@<version>` (m226) |
| `pants_shell/component_emit.rs` | BUILD-file tool pins (shellcheck / shfmt / shunit2) — synthetic design-tier `pkg:generic/<tool>` (m225) |
| `pip/requirements_txt.rs` | requirements.txt declared dep with no matching resolved lockfile (uv.lock / pip-tools) |
| `scala.rs` | build.sbt declared dep with no matching lockfile |
| `yocto/recipe.rs` | .bb recipe declaration — recipe-tier (design-tier) is the emission default (no "resolved" state for recipes) |

Additional readers with **versionless PURL** emission (semantically design-tier but flagged differently — using the empty-`version` field pattern, not always the `sbom_tier` field):

| Reader | Notes |
|---|---|
| `cargo.rs` | `build_cargo_purl` at cargo.rs:258 — versionless when version empty; regression test `build_cargo_purl_empty_version_emits_versionless_shape` at cargo.rs:3221 (m191 / #558) |
| `gem.rs` | `build_gem_purl` at gem.rs:392 — versionless when Gemfile.lock lacks matching entry (m191 / #558) + `build_gem_purl_versionless` synthetic Ruby built-in gems (m162) |
| `ipk_file.rs` / `opkg.rs` | Versionless PURL fallback in ipk / opkg readers (m190) — different from lockfile-vs-manifest split; more about OS-package metadata gaps |
| `nuget/mod.rs` | `build_nuget_purl` at mod.rs — versionless when version empty (m654 / #653) |
| `erlang.rs` | `erlang.rs:1307` — versionless when manifest.version == "0.0.0-unknown" or empty |
| `maven.rs` | `maven.rs:1278` — versionless when version.is_empty() OR contains `${}` MSBuild-esque property syntax |

### Design-tier + versionless coexist: two related but distinct signals

The two mechanisms serve different purposes:

- **`sbom_tier: Some("design")`** — the tier CLASSIFICATION (what tier a component belongs to; consumed by the milestone-158 graph-completeness pass and the milestone-191 cross-tier reconciler).
- **Versionless PURL** — the PURL SHAPE (a purl-spec-compliant identifier when the version is unknown; consumed by vulnerability scanners doing exact-version CVE matching — they get "no match" instead of a false-positive `@$(unresolved)` literal).

Every design-tier component in a well-formed waybill SBOM SHOULD carry BOTH signals: `sbom_tier="design"` AND (when the version can't be resolved) a versionless PURL. Some historical readers set only one of the two; the doc should call out this coexistence AND flag any per-ecosystem gaps discovered during writing.

### Rationale

The doc needs a per-ecosystem matrix so operators can predict outcomes. The inventory above IS the matrix source data. Every claim in the doc's per-ecosystem section maps back to a source line surfaced by these greps.

### Alternatives considered

- **Rely on existing scattered mentions in ecosystems.md**: rejected because scattered mentions are what the current problem IS — the doc lacks the concept-cluster + matrix.
- **Extract design-tier docs into a NEW file** (e.g., `docs/reference/design-tier.md`): rejected per spec Assumptions §1 — the ecosystems.md scope is per-ecosystem reader reference, and design-tier per-ecosystem info naturally belongs there. A new file would fragment the reader-reference story. The consumer-facing tier explanation stays in the m150 consumer guide (`docs/reference/reading-a-waybill-sbom.md`) — cross-linked but not duplicated.

## §B — `waybill:unresolved-reason` annotation adoption

### Decision: annotation is emitted by ONE reader today (nuget)

Grep evidence:

```bash
grep -rn '"waybill:unresolved-reason"' waybill-cli/src/
```

Only match: `nuget/mod.rs:376` (the #653 emission site) + `nuget/mod.rs:653` (the corresponding test-assertion site).

### Implication for the doc

The doc should:
1. Document the annotation as it exists today — NuGet-only, with the specific value shape "no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry".
2. Explicitly note that other design-tier readers do NOT emit this annotation TODAY, and consumers should not rely on it being present.
3. Flag this as a **cross-reader consistency gap** — file a follow-up issue (out of scope for this docs milestone per spec Assumptions §6) proposing that all 18 design-tier-emitting readers adopt the same annotation shape.

### Rationale

Documenting the annotation accurately (single-reader-today) is more honest than describing it as if it were universal. The follow-up issue closes the loop — this docs milestone surfaces the gap; a subsequent code milestone fills it.

### Alternatives considered

- **Wait for all readers to adopt the annotation, THEN write the doc**: rejected — the doc has value now (per spec P1 SC-001) even without the annotation being universal. The annotation is one of several signals; PURL shape + `sbom_tier` field are the primary signals and are universal.

## §C — jq recipe verification runbook

### Decision: recipes verified manually against real-world waybill-emitted SBOMs at doc-authoring time

Per spec FR-003 + SC-002, the doc includes `jq` recipes for filtering by tier, extracting unresolved-reason, etc. Recipes MUST run correctly against a real waybill SBOM.

Verification steps:
1. Use one or more of the existing audit SBOMs at `specs/audit-nuget-realworld/artifacts/` (RestSharp, Serilog, Orleans — all have design-tier + source-tier mixes post-#653/#657/#658).
2. Alternative source: run `waybill sbom scan` against a small local project that has BOTH a Cargo.lock (source-tier) AND a Directory.Build.props-only .NET project (design-tier after #658). This produces a mixed-tier SBOM in a single output.
3. Recipe pattern (CycloneDX):
   ```jq
   .components[] | select(any(.properties[]?; .name == "waybill:sbom-tier" and .value == "design"))
   ```
4. Recipe pattern (SPDX 2.3 — via `annotations[]`):
   ```jq
   .packages[] | select(any(.annotations[]?; .comment | test("waybill:sbom-tier.*design")))
   ```
5. Each recipe embedded in the doc gets tested against a real SBOM before the doc is committed. Failure → adjust recipe until it produces documented output.

### Rationale

Manual verification is sufficient given the small recipe count (~6 recipes) and the doc's ~500-line ceiling. Automated recipe testing (e.g., a `verify-recipes.sh` script like m151's) is out of scope but a reasonable follow-up if the recipe count grows.

### Alternatives considered

- **Ship a `verify-recipes.sh` alongside the doc** (m151 pattern): rejected for this milestone — the recipe count is small, and m151's verification harness was justified by ~30+ recipes in the consumer guide. Follow-up-able if the recipe count grows.
- **Skip recipe verification and just paste patterns from memory**: rejected — violates spec FR-010 (verified against source).

## §D — Consumer-guide cross-linking

### Decision: add a 1–3-line pointer from the consumer guide's tier passage to the new ecosystems.md tier section

Verification of consumer-guide tier coverage:

```bash
grep -n "sbom-tier\|design.tier\|source.tier\|component.tier" docs/reference/reading-a-waybill-sbom.md
```

If the consumer guide (m150–151) already has a tier-explanation passage, add a cross-reference to the new `ecosystems.md` tier section for per-ecosystem detail. If no such passage exists, no cross-link needed — the consumer guide is out of scope for this milestone per spec Assumptions §1.

### Rationale

Duplication risk is real; per spec Assumptions §3 the two docs serve different audiences (consumer vs operator+contributor). A cross-link is the minimum-friction way to connect them without duplicating content.

### Alternatives considered

- **Full duplication in both docs**: rejected — increases maintenance burden without value.
- **Move tier-concept content OUT of consumer guide INTO ecosystems.md**: rejected — the consumer guide's audience is broader (SBOM consumers, not waybill operators specifically). Keeping the consumer guide as the entry point + linking down to per-ecosystem detail matches the existing doc hierarchy.

## §E — Placement decision: coverage-matrix column vs standalone section

### Decision: standalone dedicated section (top-level `##`) placed AFTER coverage matrix but BEFORE first per-ecosystem section

Spec FR-002 offered two options: (a) new dedicated section OR (b) column added to the existing coverage matrix at top of file.

Rationale for going with (a) standalone section:

- The coverage matrix is already 5 columns wide (Ecosystem | Trigger inputs | Tier(s) emitted | Hashes | Enrichment). Adding a 6th column for "design-tier fallback trigger" would produce very narrow columns and hurt readability on the GitHub renderer.
- The design-tier concept needs conceptual framing (what does "design-tier" mean, how do consumers detect it, when is it enough) that a matrix cell can't carry.
- A dedicated section can also host the jq recipes, the consumer guidance, and the graph-completeness interaction — all of which need paragraph-scale explanation.

Compromise: the matrix keeps its 5 existing columns; the `Tier(s) emitted` column value gets updated where necessary to mention `+ design-tier fallback` for readers that emit it. This keeps the matrix accurate as an at-a-glance reference without overflowing.

### Rationale

Preserves matrix readability while giving the design-tier concept the space it needs.

### Alternatives considered

- **Add column, drop paragraph-scale detail**: rejected — the "when design-tier is enough vs when it isn't" guidance is the highest-value part per spec US1 P1.
- **Both column AND section**: acceptable but adds churn for maintainers on both surfaces every time a new reader is added. Standalone section wins on maintenance economy.

## §F — Line-budget check

### Decision: 500-line ceiling (spec SC-005) is achievable given the content plan

Estimated line breakdown for the new section:
- Tier concept explanation (source / design / binary): ~50 lines
- Per-ecosystem matrix (18–20 rows × ~2 lines each): ~50 lines
- jq recipes (6 recipes × ~10 lines each including preamble + expected-output examples): ~60 lines
- `waybill:unresolved-reason` annotation subsection: ~30 lines
- "When design-tier is enough vs when it isn't" guidance: ~60 lines
- Graph-completeness interaction: ~30 lines
- Upgrading design-tier to source-tier (per-ecosystem hints): ~40 lines
- Contributor guidance (how to emit design-tier in a new reader): ~40 lines
- Cross-reference block: ~10 lines

Total estimate: ~370 lines — comfortably under the 500-line ceiling with ~30% headroom for prose expansion during writing.

### Rationale

The estimate leaves headroom without inviting bloat. If the writing phase blows the budget, spec SC-005 forces a re-scope conversation (drop the contributor guidance? move recipes to consumer guide? etc.) rather than a silent budget breach.

### Alternatives considered

- **Un-budget the section**: rejected — bounded scope is a spec invariant.

## §G — Follow-up-issue seeds surfaced during research

These are OUT OF SCOPE for this docs milestone per spec Assumptions §6 but should be filed as issues after merge:

1. **Universalize `waybill:unresolved-reason`**: only nuget emits it today. Other 17 design-tier readers should emit an equivalent annotation for cross-reader consistency.
2. **Add `--tier=source-only` / `--tier=design-only` CLI filter flag**: operators may want to emit ONLY source-tier or ONLY design-tier for specific downstream consumers (e.g., a strict vuln scanner that shouldn't see design-tier at all).
3. **Automated jq-recipe verification harness** (m151 pattern): if this section's recipe count grows beyond ~10, add a `verify-recipes.sh` that runs each recipe against a real SBOM in CI.
4. **Cross-linking between `docs/reference/component-tiers.md` and this new ecosystems.md section**: the file-tier docs are in a separate reference; the source/design/binary tier concept spans both. Might warrant a unified "tiers explained" page.

These seeds live in this research doc for now; they'll be filed as separate GitHub issues after the docs merge.
