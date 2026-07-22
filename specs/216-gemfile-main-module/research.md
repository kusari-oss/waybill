# Phase 0 Research: Gemfile-only Ruby application main-module

**Feature**: 216-gemfile-main-module
**Date**: 2026-07-22

## R1 — Walker predicate (which directories qualify?)

**Decision**: A directory qualifies as a Ruby-application root when ALL of:
1. It contains a file named exactly `Gemfile` (case-sensitive, no extension).
2. It contains NO `*.gemspec` file at the same level (FR-007 gemspec-wins).
3. Its `Gemfile` is at rootfs or in a non-install-state subdirectory (walker skips `vendor/`, `gems/`, `specifications/`, `.bundle/` — matches m069's `find_top_level_gemspecs` exclusion set).

The `Gemfile.lock` sibling is NOT required (FR-006: emit main-module even without lock; downgrade graph-completeness signal).

**Rationale**:
- Case-sensitive `Gemfile` matches the bundler convention. Alternate spellings (`gems.rb`, `Gemfile.example`, `Gemfile.next`) are non-canonical; the walker treats only `Gemfile` as authoritative for MVP.
- The gemspec-wins guard preserves the pre-existing m069 identity path (FR-007). Two same-directory main-modules would break the split-mode enumeration.
- Exclusion set mirrors m069 — matching walker semantics keeps operator mental model consistent.

**Alternatives considered**:
- **Walk-time detection of `gems.rb` alias**: bundler accepts `gems.rb` as a synonym for `Gemfile` in some versions. REJECTED for MVP — extremely rare in the wild, easy to add later if users request it, and matches how the pre-feature gem reader already ignores it.
- **`.gemspec`-present dirs still emit application main-module with a merger**: REJECTED per FR-007. The single-identity contract per directory is easier to reason about; consumers who care about both aspects can inspect the gemspec-derived main-module's annotations.

## R2 — Application name derivation

**Decision**: The `name` field of the emitted main-module comes from:
1. The parent-directory basename of the `Gemfile`, lowercased.
2. Special-character stripping per the m215 slug rules (`waybill-cli/src/generate/split.rs::subject_slug`) — same substitution table for `/`, `@`, and unsafe filesystem chars.
3. Empty-name fallback: if sanitization yields an empty string (pathological case), skip emission and emit `tracing::warn!` — matches the m127 synthetic-placeholder skip pattern.

**Rationale**:
- No Ruby-runtime evaluation of `Gemfile` (Constitution Principle I forbids embedded scripting parsers, and the Gemfile DSL is executable Ruby).
- The directory basename is the ONLY stable filesystem-derived identity signal. Alternatives (parsing bundler `#name` comments, reading `.ruby-version`) are either non-standard or orthogonal.
- Reusing the m215 slug rules keeps the emitted-PURL name and the `--split` filename in lock-step (no divergence between `metadata.component.name` and the `<slug>.<ecosystem>.<format-ext>.json` filename).

**Alternatives considered**:
- **`app_name` inference from `Rakefile` / `config.ru` / `bin/` script names**: REJECTED. Too many failure modes; the directory basename is more predictable and matches how Rails/Sinatra apps are typically deployed.
- **`Gemfile` comment parsing** (e.g., `# name: my-app`): non-standard; unlikely to exist in real repos.

## R3 — Version fallback ladder

**Decision**: Three-step ladder, matching milestone-053's Go version resolution:
1. **`git describe --tags --always` in the application directory** (2-second subprocess timeout). If it returns a value, use it verbatim.
2. **`git describe --tags --always` at the scan root** (fallback for repos with a single tag applying to the whole tree). Same 2-second timeout.
3. **Literal `"0.0.0-unknown"`** (matches the m069 gemspec-fallback + m053 Go-module fallback). This value passes `Purl::new` validation and is a documented waybill convention.

The ladder is best-effort; failures at any step advance to the next without emitting a warning (matches m053's silent-fallback pattern for Go). Only step-3 fallback emits `tracing::debug!` for observability.

**Rationale**:
- Matches an existing accepted precedent (m053 Go module resolution). Zero design novelty.
- git-describe is a widely-accepted convention for version-inference on unversioned software.
- Subprocess cost is bounded (2 seconds per step, at most 2 shellouts per application).

**Alternatives considered**:
- **Parse `Gemfile.lock`'s `BUNDLED WITH` field for the app's own version**: doesn't exist — that field describes bundler, not the app.
- **Read `.ruby-version`**: describes the Ruby interpreter, not the app version.
- **No git-describe, always emit `0.0.0-unknown`**: acceptable MVP, but the git-describe ladder is a low-cost quality improvement that matches every other ecosystem reader's behavior. Include it in v1 rather than punt to a follow-up.

## R4 — Companion annotation (parity-bridging)

**Decision**: Every Gemfile-derived application main-module carries:
- `waybill:package-shape = "application"`

This annotation is a new key in the waybill vocabulary. Documented in `docs/reference/sbom-format-mapping.md` per Constitution Principle V's parity-bridging requirement.

Value vocabulary at v1:
- `"application"` — bundler-managed Ruby application (this feature)
- Reserved for future use: `"library"`, `"binary"`, `"framework"`

**Rationale**:
- **Purl-spec has no application-vs-library type distinction** — the type space is by ecosystem, not by role-in-consumption. `pkg:generic/` is the escape hatch for "this isn't a real registry package", and this annotation is the ecosystem signal that says "you can trust that this generic package came from a Ruby Gemfile".
- **CDX `Component.type` overlap**: CycloneDX 1.6 has an enum with values `application`/`library`/`framework`/etc., but its semantic is "role in the assembly" (is this a library, or a top-level app?), not "how was this identity inferred?" — orthogonal to the new annotation. Waybill emits `Component.type = "application"` for the CDX component regardless (matches how the m069 gemspec-derived main-modules already emit `Component.type = "application"`).
- **SPDX has no equivalent field** at 2.3 or 3.0.1. Parity-bridging annotation is justified.

**Alternatives considered**:
- **`waybill:gem-source = "application"`**: too gem-specific. If other ecosystems adopt this pattern in the future (pip apps vs libraries, npm CLI vs library, etc.), a `waybill:package-shape` key generalizes cleanly.
- **`waybill:main-module-shape = "application"`**: overspecific — the "main-module" prefix duplicates the co-emitted `waybill:component-role = "main-module"` annotation.
- **Reuse the `Component.type` CDX field alone**: doesn't survive SPDX round-trip; consumers reading SPDX SBOMs wouldn't have the signal.

## R5 — Emission dispatch position in `read()`

**Decision**: Add a second loop in `gem.rs::read()` immediately after the existing gemspec-loop (approximately line 1057-1090 today). The application-loop walks `find_top_level_gemfiles(rootfs)`, filters out any directory that already produced a gemspec-derived main-module in the same run, and appends application main-modules to `out`.

The augment-existing-or-emit-new pattern from the gemspec loop is NOT needed here (FR-007 guarantees no same-directory overlap). Simple push-into-out.

**Rationale**:
- Adjacent to the existing main-module dispatch — reviewers see one code region for all main-module emission.
- No cross-loop state to manage (gemspec-loop drops dedup entries first, then application-loop runs against the already-populated `out` and skips any dir whose PURL prefix matches a gemspec directory).

**Alternatives considered**:
- **Merge into `find_top_level_gemspecs`**: rejected — the walker returns paths of `*.gemspec` files; conflating with Gemfile paths would break the existing tests + require a downstream discriminator anyway.
- **Emit at a different scan_fs layer**: rejected — this is per-ecosystem detection; the correct home is `gem.rs`.

## R6 — Backwards-compat guarantee mechanism

**Decision**: Every existing waybill fixture that:
- Contains no `Gemfile` → the new walker returns empty, no change.
- Contains a `.gemspec` (with or without `Gemfile`) → FR-007 gemspec-wins branch fires, no additional emission.

Result: 100% of pre-feature `{cdx,spdx,spdx3}_regression` byte-identity tests continue to pass without touching goldens (SC-004).

**Verification approach**: after each implementation step, run the m069 regression fixture through the emit pipeline and diff against the pre-feature golden. Any byte drift means the emission-dispatch loop leaked into a non-Gemfile-only path — fix and re-check.

**Rationale**: byte-identity is the cheapest guarantee we have that existing consumers see no unexpected drift. The alternative — trusting semantic invariants — has failed twice in waybill's history (m214 rename + m204 helm annotation), each time surfaced by a byte-identity test.

## R7 — Test fixture shape

**Decision**: New fixture at `waybill-cli/tests/fixtures/gemfile_application/`:
```
gemfile_application/
├── Gemfile              # declares 2-3 deps (source, gem 'rack', gem 'json')
└── Gemfile.lock         # bundler-generated lock with same deps + transitives
```

Minimal — no `Rakefile`, no `bin/`, no `.gemspec`, no `.ruby-version`. Keeps the fixture small (~30 lines total) and the intent single-purpose. The Gemfile.lock is authored by hand (mimicking bundler output shape) rather than generated by `bundle install` so the test is hermetic (no need for a Ruby runtime in CI).

**Sibling fixture** (for the no-lock test): `waybill-cli/tests/fixtures/gemfile_no_lock/` with just `Gemfile` (no `Gemfile.lock`) — covers FR-006.

**Sibling fixture** (for the gemspec-precedence test): reuses the m069 fixture (existing `.gemspec`-carrying) with a synthetic Gemfile added by the test setup — validates FR-007 without adding a whole new directory.

**Rationale**: three narrow fixtures beat one giant fixture for reviewability. Each fixture proves one thing.

**Alternatives considered**:
- **Real-world Rails app snapshot**: too large; hides the tested surface in noise.
- **Run `bundle install` in the fixture as part of CI setup**: violates Constitution Principle I spirit (no Ruby runtime dep) and adds ~60-second overhead per CI run.

## R8 — Integration test scenarios

**Decision**: Four integration tests in `waybill-cli/tests/gemfile_main_module.rs`:
1. `gemfile_only_dir_emits_pkg_generic_main_module` — happy path (fixture: `gemfile_application/`). Asserts `pkg:generic/<name>@<version>` PURL + `waybill:package-shape = "application"` annotation.
2. `gemspec_present_wins_over_gemfile` — FR-007. Uses the m069 fixture + a synthetic Gemfile added at test-setup. Asserts exactly ONE main-module emitted, and its PURL is `pkg:gem/...` (gemspec-derived).
3. `gemfile_without_lock_still_emits_main_module` — FR-006 (fixture: `gemfile_no_lock/`). Asserts main-module present in `components[]`.
4. `iac_reproducer_pattern_split_mode` — end-to-end shape: 2 sibling Gemfile-only dirs → 2 sub-SBOMs under `--split`. Doesn't require the real iac repo; a mini-monorepo fixture with 2 dirs suffices.

## R9 — Docs update scope

**Decision**: Add ONE row to `docs/reference/sbom-format-mapping.md` for `waybill:package-shape`. Format matches the existing table rows (annotation name, format-native equivalent per CDX/SPDX-2.3/SPDX-3, notes). Documents the parity-bridging justification per Constitution Principle V.

No other docs changes — the `--split` CLI reference (m215) doesn't need updating since the feature is opaque to the CLI surface; the `split-manifest.md` docs get one implicit consequence (new `ecosystem = generic` entries in the manifest for Ruby apps) but the manifest schema hasn't changed.

**Rationale**: minimal doc surface = minimal doc rot. The parity-bridging annotation IS the interesting design point; the operator surface is unchanged.
