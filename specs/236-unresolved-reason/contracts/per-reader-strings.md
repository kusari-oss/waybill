# Per-Reader Reason String Contract

**Milestone**: 236

Locked reason strings per reader. Byte-stable within a waybill build. Display-only per Q1 clarification — may be refined between releases without a semver-major bump.

## US1 — Top-5 ecosystems (P1)

| Reader | Reason string |
|---|---|
| **cargo** | `no matching entry in Cargo.lock` |
| **gem** | `no matching entry in Gemfile.lock` |
| **maven** | `no <version> in pom.xml; no dependency-reduced-pom.xml or effective-pom fallback` |
| **npm/mod** | `no matching entry in package-lock.json / pnpm-lock.yaml / yarn.lock / bun.lock` |
| **npm/walk** | `workspace member; no lockfile-resolved version` |
| **pip** | `no version specifier in requirements.txt; no uv.lock / poetry.lock fallback` |

## US2 — JVM + tool ecosystems (P2)

| Reader | Reason string |
|---|---|
| **kotlin_dsl/mod** | `Kotlin DSL declaration; --include-declared-deps enables emission; requires Gradle daemon for full resolution` |
| **kotlin_dsl/build_script** | `Kotlin DSL buildscript declaration; --include-declared-deps enables emission` |
| **scala** | `declared in build.sbt; no coursier-resolved lockfile` |
| **gradle_static** | `declared in build.gradle; US2 cache reader had no matching seed` |
| **helm** | `unrendered Chart.yaml dependency; --helm-render subprocess disabled or unavailable` |
| **yocto** | `recipe .bb declaration; no PV/PR resolution` |

## US3 — Long-tail ecosystems (P3)

| Reader | Reason string |
|---|---|
| **cocoapods** | `no matching entry in Podfile.lock` |
| **composer** | `no matching entry in composer.lock` |
| **dart** | `no matching entry in pubspec.lock` |
| **elixir** | `no matching entry in mix.lock` |
| **erlang** | `no matching entry in rebar.lock` |
| **haskell** | `declared in stack.yaml / .cabal; no stack.yaml.lock fallback` |
| **pants_shell** | `pants shell tool pin without version specifier` |
| **pants_go** | `pants_go expected_version declared; no matching go corpus component` |

## Regression guard — NuGet (existing)

| Reader | Reason string (unchanged) |
|---|---|
| **nuget** | `no Version= on <PackageReference>, no CPM entry in Directory.Packages.props, no packages.lock.json entry` |

**FR-006 assertion**: this string is preserved verbatim across the milestone. Byte-identity test in `waybill-cli/tests/unresolved_reason_universal.rs` asserts the NuGet reader emits this exact string on the existing NuGet fixture pre/post merge.

## Constraints

- Each string is ASCII English, <200 chars.
- No PII / paths / credentials (FR-010).
- Human-readable + boundary-naming (FR-002).
- Byte-stable within a build (FR-003).

## Verification

- **Per-reader unit test**: for each row above, a test in the reader's `mod tests` section asserts the exact string on a deterministic fixture (FR-009).
- **Cross-reader integration test**: `waybill-cli/tests/unresolved_reason_universal.rs` scans a directory containing all 18 fixtures (17 new + NuGet regression-guard) + asserts every design-tier component in every fixture's emitted SBOM carries the annotation with a non-empty string (SC-001).
- **Blacklist scan**: CI substring blacklist over the emission call-sites + emitted SBOM values (FR-010).
