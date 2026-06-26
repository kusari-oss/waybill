# Feature Specification: Haskell ecosystem reader

**Feature Branch**: `143-haskell-reader`
**Created**: 2026-06-25
**Status**: Draft
**Input**: User description: "423"

## Background

Haskell is the most-deployed statically-typed purely-functional language. It powers production systems at Facebook (Sigma anti-abuse rule engine), GitHub (Semantic code intelligence), Standard Chartered (financial modeling), Tweag (build tooling), and the entire Cardano blockchain platform. Beyond the FAANG-tier deployers, Haskell anchors a long tail of compiler/language-tools projects (Pandoc, ShellCheck, XMonad, Hasura, PostgREST) that ship as widely-distributed CLI tools.

The Haskell build-tool landscape has two dominant systems with mutually-incompatible lockfile conventions:

- **`cabal-install`** — the canonical build tool, distributed with GHC since 2016. Uses `cabal.project` (multi-package project descriptor), `cabal.project.freeze` (line-format pinned constraints), and `*.cabal` (per-package descriptor with `build-depends:` blocks).
- **Stack** — a reproducibility-focused alternative built atop Cabal-the-library. Uses `stack.yaml` (project config with `resolver:` field), `stack.yaml.lock` (YAML lockfile with snapshot SHA + explicit `extra-deps`), and the same `*.cabal` descriptors.

mikebom currently has zero Haskell coverage — every Hackage-published dep in cabal-managed or Stack-managed projects (`aeson`, `text`, `bytestring`, `lens`, `servant`, `pandoc`, Cardano `plutus-*`, etc.) is invisible to scans.

Hackage is the central Haskell package registry. Every Haskell project, regardless of build tool, resolves package coordinates against Hackage by `<package-name> <version>` pairs. PURL: `pkg:hackage/<package>@<version>` — the `hackage` type IS purl-spec-blessed ([hackage-definition.md](https://github.com/package-url/purl-spec/blob/main/types-doc/hackage-definition.md)). No new PURL type, no Scala-style suffix shenanigans — Haskell package names are stable across compiler versions (the `base` library is `base-4.18.0.0` for GHC 9.6, `base-4.19.0.0` for GHC 9.8, etc.; the name doesn't get a compiler suffix).

Critical Stack-specific complication: Stack's `resolver:` field declares a **Stackage snapshot** (e.g., `lts-22.0`, `nightly-2024-01-15`, `ghc-9.6.4`). The snapshot itself is a curated bundle of ~2500 Hackage packages with mutually-compatible version pins, hosted at <https://www.stackage.org/>. Stack's `stack.yaml.lock` pins the snapshot's content-hash (so the same `lts-22.0` resolves identically across machines) but does NOT enumerate the snapshot's individual packages in the lockfile — to get the full transitive pin list, you'd either fetch the snapshot manifest from stackage.org OR run `stack ls dependencies`. Per FR-012 (no network access) + the issue's documented approach, this milestone emits ONE placeholder component per Stack snapshot resolver (with `mikebom:source-type = "hackage-snapshot"`) plus explicitly-pinned `extra-deps`. Full snapshot expansion is deferred to v1.1.

This feature closes the Haskell gap so an operator scanning any cabal- or Stack-managed Haskell project gets a complete SBOM with every explicitly-pinned Hackage dep represented + clear indication of Stack snapshot resolvers in use.

## Clarifications

### Session 2026-06-25

- Q: How should GHC-shipped boot libraries (`base`, `text`, `bytestring`, `containers`, etc. — ~20 packages whose versions are coupled to the compiler release) be marked in the emitted SBOM so consumers can filter "user code only" views? → A: **Land `mikebom:ghc-stdlib = "true"` annotation in v1** via a hardcoded boot-library allowlist (`base`, `ghc-prim`, `template-haskell`, `integer-gmp`, `integer-simple`, `array`, `bytestring`, `containers`, `deepseq`, `directory`, `filepath`, `ghc`, `mtl`, `parsec`, `pretty`, `process`, `stm`, `text`, `time`, `transformers`, `unix`, `Win32`). Allowlisted entries emit as regular `pkg:hackage/<name>@<version>` components AND additionally carry `mikebom:ghc-stdlib = "true"`; non-allowlisted entries emit without the annotation. The annotation is informational — does NOT gate emission, so completeness (Principle VIII) is preserved. Mirrors the milestone-141 Q1 OTP-stdlib pattern exactly; supersedes the prior "deferred to v1.1" wording in spec Out of Scope.
- Q: For multi-stanza `*.cabal` files (library + executable + test-suite + benchmark), what populates the main-module's `depends` set? → A: **Union all stanzas + per-stanza lifecycle-scope tagging**. Main-module `depends` = `library.build-depends` ∪ `executable.build-depends` ∪ `test-suite.build-depends` ∪ `benchmark.build-depends` ∪ `build-tool-depends`. Each edge-target component is tagged with `mikebom:lifecycle-scope`: library/executable → runtime; test-suite/benchmark/build-tool-depends → development (per FR-010). Self-referential deps (e.g., `executable` depending on the `library` of the same package) dedup naturally by PURL. Operator filterability preserved via the CDX `scope` field. Matches the milestone-141 Q2 union approach for Erlang's `applications:` keyword-family.
- Q: How should Hpack-generated `*.cabal` files be handled — silent ignore, warn-on-detection, or parse `package.yaml` directly? → A: **Detect-and-warn**. When `package.yaml` is found in the same dir as a generated-by-Hpack `*.cabal` (identified by the leading `-- This file has been generated from package.yaml by hpack version X.Y.Z.` comment), emit a `tracing::warn!` diagnostic naming both file paths and recommending `hpack` regeneration before scanning. The reader still reads the `*.cabal` as the authoritative artifact — does NOT parse `package.yaml` directly (avoids doubling parser surface + Hpack-syntax-quirks complexity). Operator-actionable signal for the ~30% of Stack projects using Hpack to catch stale-cabal scenarios without taking on a second source-of-truth parser. Operators with reliable pre-commit hooks can ignore the warning at no functional cost.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Operator scans a cabal-managed Haskell project with `cabal.project.freeze` (Priority: P1) 🎯 MVP

A Haskell developer runs `mikebom sbom scan --path .` on their cabal-managed Haskell project containing a `*.cabal` package descriptor + `cabal.project.freeze` lockfile (the dominant production-pinning convention since cabal-install 2.4 circa 2019). They receive an SBOM containing one component per pinned Hackage dep. Each component carries a `pkg:hackage/<package>@<version>` PURL.

**Why this priority**: The headline use case. Production Haskell projects ship `cabal.project.freeze` for reproducible builds; this is the highest-value scan target since the freeze file is fully self-contained and human-readable.

**Independent Test** (SC-001): Synthetic fixture with `my-app.cabal` (declaring 3 direct deps) + `cabal.project.freeze` pinning those 3 plus 5 transitives (8 total exact-version `==X.Y.Z` constraints). Run `mikebom sbom scan --path <tmp>`. Assert exactly 8 `pkg:hackage/*` components emit with correct names + versions.

**Acceptance Scenarios**:

1. **Given** a Haskell project with `cabal.project.freeze` pinning `aeson ==2.2.0.0`, `text ==2.0.2`, `bytestring ==0.11.5.3`, **When** the operator runs `mikebom sbom scan --path <project>`, **Then** the emitted SBOM contains components for each pinned dep with PURL `pkg:hackage/<name>@<version>`.
2. **Given** the same project, **When** the operator inspects the emitted SBOM, **Then** transitive deps pinned in the freeze file (e.g., `attoparsec`, `vector`, `time`) also appear as components — the freeze file is the authoritative dep set, not just `*.cabal`'s `build-depends:`.
3. **Given** a source tree WITHOUT `*.cabal` or `cabal.project.freeze` or `stack.yaml.lock`, **When** the operator scans, **Then** no Haskell components appear AND no warning fires (clean no-op).
4. **Given** a project whose `my-app.cabal` declares `name: my-app` + `version: 1.2.3`, **When** the operator scans, **Then** a main-module component emits with PURL `pkg:hackage/my-app@1.2.3` carrying `mikebom:component-role = "main-module"` and `mikebom:sbom-tier = "source"` annotations.

---

### User Story 2 — Operator scans a Stack-managed Haskell project with `stack.yaml.lock` (Priority: P2)

A Haskell developer using Stack runs `mikebom sbom scan --path .` on their Stack project containing `stack.yaml` (declaring `resolver: lts-22.0`) + `stack.yaml.lock` (pinning the snapshot SHA + listing explicit `extra-deps`). The SBOM distinguishes three classes: the snapshot-resolver placeholder (one component per resolver), the explicit extra-deps (one component each), and the per-package main-module from the local `*.cabal`.

**Why this priority**: Stack has a substantial production user base (~40% of Haskell ecosystem circa 2026, dominant in industrial Haskell). The snapshot-resolver mechanism is fundamentally different from cabal-install's freeze-file model; conflating them would mis-represent dep provenance.

**Independent Test** (SC-002): Synthetic fixture with `stack.yaml` (resolver `lts-22.0`) + `stack.yaml.lock` (snapshot SHA + 2 explicit `extra-deps`). Scan. Assert: 2 `pkg:hackage/*` components for the extra-deps + 1 placeholder for the snapshot resolver carrying `mikebom:source-type = "hackage-snapshot"`.

**Acceptance Scenarios**:

1. **Given** a Stack project with `stack.yaml` declaring `resolver: lts-22.0` and `stack.yaml.lock` pinning the resolver's SHA-256, **When** the operator scans, **Then** a placeholder component emits with PURL `pkg:generic/stackage-lts-22.0@<snapshot-sha>` carrying `mikebom:source-type = "hackage-snapshot"` and `mikebom:stackage-resolver = "lts-22.0"` evidence so consumers know this represents a curated bundle.
2. **Given** the same project's `stack.yaml.lock` listing explicit `extra-deps` like `aeson-2.2.0.0` and `lens-5.2.3`, **When** the operator scans, **Then** TWO additional `pkg:hackage/aeson@2.2.0.0` and `pkg:hackage/lens@5.2.3` components emit with `mikebom:source-type = "hackage-stack-lock"` evidence.
3. **Given** a project mixing `resolver: nightly-2024-01-15`, **When** the operator scans, **Then** the snapshot placeholder reflects the nightly naming: `pkg:generic/stackage-nightly-2024-01-15@<sha>`.
4. **Given** a Stack project whose `stack.yaml` declares `resolver: ghc-9.6.4` (a GHC-only resolver, no Stackage bundle), **When** the operator scans, **Then** the snapshot placeholder uses `pkg:generic/ghc-9.6.4@unspecified` with `mikebom:stackage-resolver = "ghc-9.6.4"` evidence.

---

### User Story 3 — Operator scans a Haskell project WITHOUT a committed lockfile (Priority: P3)

Some Haskell library projects (especially Hackage-published libraries) deliberately do NOT commit `cabal.project.freeze` or `stack.yaml.lock` — they ship the `*.cabal` with version-range `build-depends:` only. Scanning should produce SOME inventory rather than empty output, marked as `design`-tier with the original version range preserved as evidence.

**Why this priority**: Important for the Hackage-library ecosystem; many published libraries don't commit lockfiles (lockfiles are for application repros, not library deploys).

**Independent Test** (SC-003): Synthetic fixture with `my-lib.cabal` declaring 2 direct deps via `build-depends:` ranges but NO lockfile. Scan. Assert 2 components emit with `mikebom:sbom-tier = "design"` annotation + the version range preserved as `mikebom:requirement-range` evidence.

**Acceptance Scenarios**:

1. **Given** a Haskell library project with `my-lib.cabal` only (no lockfile), declaring `build-depends: base >= 4.18 && < 4.20, text >= 2.0`, **When** the operator scans, **Then** components emit for each declared dep with `mikebom:sbom-tier = "design"` and the original range preserved.
2. **Given** the same project, **When** the operator inspects the emitted SBOM, **Then** NO transitive deps appear (lockfile is required for transitive resolution).
3. **Given** a `*.cabal` declaring deps inside a `test-suite` stanza (e.g., `hspec`, `tasty`), **When** the operator scans in design-tier mode, **Then** those deps carry `mikebom:lifecycle-scope = "development"` annotation; downstream `--exclude-scope dev` filtering successfully suppresses them.

---

### Edge Cases

- **`==` exact-pin vs range constraints in `cabal.project.freeze`**: production freeze files use exclusively `==X.Y.Z` exact pins (the format's purpose). v1 emits exact pins as source-tier components; range constraints (uncommon in freeze files but legal) emit as design-tier with the range preserved.
- **Cabal flag constraints**: `constraints: pkg +flag, pkg -flag` are flag toggles, NOT version pins. v1 ignores flag-only constraints; they don't emit components (they're build-configuration metadata, not dep declarations).
- **Multi-package cabal projects**: a `cabal.project` may declare `packages: ./pkg-a ./pkg-b ./pkg-c` enumerating multiple local packages, each with its own `*.cabal`. v1 emits ONE main-module per discovered `*.cabal` (matches the milestone-141 umbrella + milestone-142 multi-project convention).
- **Multi-package Stack projects**: same shape via `stack.yaml`'s `packages: [./pkg-a, ./pkg-b]`. v1 emits one main-module per discovered `*.cabal`.
- **GHC-shipped packages**: `base`, `ghc-prim`, `template-haskell`, `integer-gmp`, `array`, `containers`, etc. ship with GHC and are technically deps but their versions move in lockstep with the compiler version. They appear in lockfiles when explicitly pinned. v1 emits them with the regular `pkg:hackage/<name>@<version>` PURL (matching their Hackage listing) AND, per Q1, additionally carries the `mikebom:ghc-stdlib = "true"` annotation for entries matching the hardcoded boot-library allowlist (see Clarifications Session 2026-06-25). Operators filtering "user code only" do `mikebom:ghc-stdlib != "true"` via the standard property filter.
- **`stack.yaml.lock` schema versions**: Stack's lockfile format has been stable since Stack 2.1 (2019). v1 targets the current `# Lock file, version 1` shape. Pre-2.1 Stack versions are out of scope.
- **`cabal.project.freeze` line continuations**: the format permits a single `constraints:` keyword followed by a multi-line comma-separated list. v1 handles multi-line via line-join before tokenization.
- **Multiple `*.cabal` files in one directory**: defensive against ambiguous projects. v1 picks the alphabetically-first `*.cabal` per directory as the main-module source; warn-and-continue for the others.
- **Hpack-generated `*.cabal`**: some Stack projects generate `*.cabal` from `package.yaml` via Hpack. The generated `*.cabal` is the committed/checked-in artifact; mikebom reads it directly. Per Q3, when `package.yaml` is detected alongside a generated-by-Hpack `*.cabal` (identified by the `-- This file has been generated from package.yaml by hpack version X.Y.Z.` header comment), the reader emits a `tracing::warn!` diagnostic naming both files and recommending `hpack` regeneration before scanning. The reader still uses the `*.cabal` as the authoritative artifact — does NOT parse `package.yaml` directly (avoid double-source-of-truth complexity).
- **Custom Hackage mirrors**: `cabal.config`'s `repository:` directive can point at a private Hackage mirror. v1 ignores the mirror config and emits the default `pkg:hackage/<name>@<version>` PURL; downstream parity-bridge annotations for mirror provenance are deferred.
- **`build-tool-depends:` vs `build-depends:`**: `build-tool-depends:` declares build-time-only tools (e.g., `alex`, `happy`). v1 emits these with `mikebom:lifecycle-scope = "development"` annotation per FR-010 + the cross-milestone convention.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect Haskell projects by the presence of any of: `*.cabal` files, `cabal.project`, `cabal.project.freeze`, `stack.yaml`, OR `stack.yaml.lock` anywhere under the scan root. Any of these triggers reader activation.
- **FR-002**: System MUST parse `cabal.project.freeze` (cabal-install line-format pinning). Extract each `==<version>` exact-pin constraint from the `constraints:` block (handling multi-line continuations via line-join). Flag-only constraints (`pkg +flag` / `pkg -flag`) are recognized and skipped per Edge Case. Range constraints (`>=X.Y && <X.Y.0`) parse but emit as design-tier with the range preserved as `mikebom:requirement-range` evidence.
- **FR-003**: System MUST parse `stack.yaml.lock` (Stack lockfile, schema-v1, Stack 2.1+). Extract: (a) the snapshot resolver (`snapshots[].completed.url` + `snapshots[].original.resolver` fields, or top-level `snapshots:` array depending on schema variant), emitting one placeholder component per resolver; (b) explicit `packages:` entries (each with `original.hackage` or `completed.pantry-tree` fields), emitting one `pkg:hackage/` component per entry.
- **FR-004**: System MUST emit one component per parsed `cabal.project.freeze` exact-pin OR `stack.yaml.lock` `extra-deps` entry with PURL `pkg:hackage/<package>@<version>`. Hackage package names are case-insensitive but conventionally lowercased; mikebom emits lowercased names per the purl-spec hackage-definition convention.
- **FR-005**: For Stack snapshot resolvers, system MUST emit a placeholder component with PURL `pkg:generic/<resolver-name>@<snapshot-sha-or-unspecified>` carrying `mikebom:source-type = "hackage-snapshot"` and `mikebom:stackage-resolver = "<resolver-name>"` evidence so consumers can identify which curated bundle was in use. Full snapshot expansion (fetching the ~2500-package list from stackage.org) is deferred to v1.1 — v1 surfaces the resolver identity only.
- **FR-006**: System MUST emit dependency edges from each project's main-module (per FR-013) to each direct dep declared across ALL stanzas of `*.cabal`. Per Q2 union semantics, the main-module's `depends` set is the union of: `library.build-depends` ∪ each `executable.build-depends` ∪ each `test-suite.build-depends` ∪ each `benchmark.build-depends` ∪ `build-tool-depends`. Each edge-target component is tagged with its per-stanza `mikebom:lifecycle-scope` per FR-010 (library/executable → runtime; test-suite/benchmark/build-tool-depends → development). When the same dep appears in multiple stanzas, the most-binding scope wins (runtime > development) so an `aeson` declared in both `library` and `test-suite` stanzas emits as runtime-scope. Transitive components surface as standalone components; inter-package dependency edges deferred to v1.1.
- **FR-007**: When neither `cabal.project.freeze` nor `stack.yaml.lock` is present but `*.cabal` is, system MUST emit components for direct deps declared in `*.cabal`'s `build-depends:` blocks via regex extraction. Each design-tier component carries `mikebom:sbom-tier = "design"` annotation and the original version range as `mikebom:requirement-range` evidence.
- **FR-008**: System MUST treat a source tree containing none of the FR-001 artifacts as a clean no-op — no components emitted, no warnings logged.
- **FR-009**: System MUST tolerate per-file parse errors without aborting the whole scan — log a structured warning naming the affected file path and continue. When `cabal.project.freeze` OR `stack.yaml.lock` is malformed AND a sibling `*.cabal` exists, fall back to design-tier emission per FR-007.
- **FR-010**: System MUST tag deps declared inside `*.cabal`'s `test-suite`, `benchmark`, OR `build-tool-depends:` stanzas with `mikebom:lifecycle-scope = "development"` (matches the milestone-052 / 137-142 convention). Deps declared in `library` or `executable` stanzas map to runtime-scope.
- **FR-011**: System MUST handle multi-package Haskell projects (a `cabal.project` declaring `packages: ./pkg-a ./pkg-b` OR a `stack.yaml` declaring `packages: [./pkg-a, ./pkg-b]`) by emitting **one main-module component per discovered `*.cabal`**. Each package's `build-depends:` contributes to its own main-module's `depends` set per FR-006. Same-PURL deps across sub-packages collapse via standard `seen_purls` dedup.
- **FR-012**: System MUST NOT make any network calls during the scan — `cabal.project.freeze` and `stack.yaml.lock` are fully self-contained on-disk; the snapshot-expansion deferral (FR-005) is what keeps this invariant intact.
- **FR-013**: For each `*.cabal` file at the project root (and each `<subdir>/*.cabal` for multi-package projects), system MUST emit one **main-module component** with PURL `pkg:hackage/<name>@<version>` extracted from the `name:` and `version:` keywords. The component MUST carry `mikebom:component-role = "main-module"` and `mikebom:sbom-tier = "source"` annotations. When `name:` or `version:` is unparseable, fall back to: `name → parent-dir basename`, `version → "0.0.0-unknown"` (per the milestone-141/142 cascade pattern). When multiple `*.cabal` files exist in the same directory, the alphabetically-first wins per the Edge Case.
- **FR-014**: Per Q1 clarification, system MUST emit a `mikebom:ghc-stdlib = "true"` annotation on every emitted component whose name matches the hardcoded GHC boot-library allowlist (`base`, `ghc-prim`, `template-haskell`, `integer-gmp`, `integer-simple`, `array`, `bytestring`, `containers`, `deepseq`, `directory`, `filepath`, `ghc`, `mtl`, `parsec`, `pretty`, `process`, `stm`, `text`, `time`, `transformers`, `unix`, `Win32`). Allowlist match is case-insensitive against the lowercased PURL `name` slot. The annotation is informational — it does NOT gate emission; allowlisted components emit identically to non-allowlisted ones except for the additional property. Mirrors the milestone-141 OTP-stdlib pattern; this is the durable parity-bridge so downstream tooling can do `mikebom:ghc-stdlib != "true"` to filter "Hackage user code only" views without needing a hardcoded boot-library list of its own.
- **FR-015**: Per Q3 clarification, when `package.yaml` is detected in the same directory as a `*.cabal` file whose leading line matches the Hpack-generated header pattern `^-- This file has been generated from package\.yaml by hpack version`, system MUST emit a `tracing::warn!` diagnostic naming both file paths and recommending `hpack` regeneration before scanning. The reader MUST still use the `*.cabal` as the authoritative parse source — it does NOT parse `package.yaml` directly. The warning is one-shot per detection (does not retrigger on subsequent `*.cabal` files in the same scan).

### Key Entities

- **`cabal.project.freeze`**: Plain-text constraint list, line-format. Single top-level `constraints:` keyword followed by a comma-separated list of `<pkg> ==<version>` exact pins or `<pkg> +<flag>` / `<pkg> -<flag>` flag toggles. Multi-line continuations allowed. Produced by `cabal-install` via `cabal freeze`.
- **`stack.yaml.lock`**: YAML lockfile produced by Stack 2.1+. Top-level shape: `{snapshots: [{completed: {sha256, size}, original: {resolver}}], packages: [{completed: {pantry-tree, sha256, size}, original: {hackage}}]}`. Pins the resolver SHA (so the snapshot bundle is content-addressable) and lists explicit `extra-deps`.
- **`*.cabal`**: Per-package descriptor in cabal's custom DSL (NOT YAML, NOT JSON — a curly-brace-optional indented format). Top-level fields: `name:`, `version:`, `license:`, `author:`, etc. Stanzas: `library`, `executable <name>`, `test-suite <name>`, `benchmark <name>`. Each stanza has its own `build-depends:` list.
- **`cabal.project`**: Multi-package project descriptor for cabal-install. Top-level fields: `packages:` (list of local packages — `./pkg-a` / glob pattern), `optional-packages:`, `with-compiler:`, etc.
- **`stack.yaml`**: Stack project config. Top-level fields: `resolver:` (Stackage snapshot identifier), `packages:` (local packages list), `extra-deps:` (additional pins beyond the snapshot). The lockfile pins what `stack.yaml` resolves to.
- **Stackage snapshot**: A curated bundle of ~2500 mutually-compatible Hackage packages, named `lts-X.Y` (long-term-support), `nightly-YYYY-MM-DD`, or `ghc-X.Y.Z`. Hosted at <https://www.stackage.org/>. Resolver names are stable identifiers.
- **GHC-shipped packages**: Boot libraries that ship with GHC (`base`, `ghc-prim`, `template-haskell`, `integer-gmp`/`integer-simple`, `array`, `bytestring`, `containers`, `deepseq`, `directory`, `filepath`, `ghc`, `mtl`, `parsec`, `pretty`, `process`, `stm`, `text`, `time`, `transformers`, `unix`/`Win32`). Their versions move in lockstep with the compiler. They appear in lockfiles when explicitly pinned.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A scan of a synthetic Haskell project with `my-app.cabal` (3 direct deps) + `cabal.project.freeze` (those 3 plus 5 transitives = 8 exact-pin `==X.Y.Z` constraints) produces a CDX SBOM whose Haskell component count matches the freeze entry count exactly (8) plus 1 main-module (= 9 total Haskell-derived components); direct-dep edges target real bom-refs.
- **SC-002**: A scan of a Stack project with `stack.yaml` (`resolver: lts-22.0`) + `stack.yaml.lock` (snapshot SHA + 2 extra-deps) produces: 2 `pkg:hackage/` components for the extra-deps + 1 `pkg:generic/stackage-lts-22.0@<sha>` placeholder carrying `mikebom:source-type = "hackage-snapshot"` + 1 main-module = 4 total Haskell-derived components.
- **SC-003**: A scan of a Hackage library project with `*.cabal` only (no lockfile) produces components for declared `build-depends:` entries with `mikebom:sbom-tier = "design"` annotation and the version range preserved as `mikebom:requirement-range` evidence.
- **SC-004**: A source tree containing no Haskell files produces an SBOM byte-identical (modulo timestamps + serial numbers) to a pre-feature baseline scan. (No-op preservation invariant.)
- **SC-005**: A scan completes successfully (exit code 0, valid SBOM) on a fixture where one `cabal.project.freeze` has corrupted syntax alongside three valid Haskell project subdirectories. The output contains components from the three valid projects plus a warning naming the corrupted file; the corrupted project falls back to design-tier emission from its sibling `*.cabal`.
- **SC-006**: An external SBOM consumer reading the emitted CDX JSON can enumerate every Hackage-managed Haskell dep via the standard `components[]` array filtered on `purl =~ "^pkg:hackage/"`. No Haskell-specific consumer code is required.
- **SC-007**: A scan of a fixture with a `*.cabal` declaring `test-suite spec\n  build-depends: hspec >= 2.10` produces a component carrying `mikebom:lifecycle-scope = "development"` annotation; downstream `--exclude-scope dev` filtering successfully suppresses the component.
- **SC-008**: A scan of a project whose `my-app.cabal` declares `name: my-app` + `version: 1.2.3` produces a main-module component with PURL `pkg:hackage/my-app@1.2.3` carrying `mikebom:component-role = "main-module"` annotation.
- **SC-009**: A scan of a multi-package Haskell project with 3 local packages (each with its own `<subdir>/*.cabal`) declared in `cabal.project`'s `packages:` field produces 3 main-module components — one per local package. Same-PURL deps across local packages collapse to single component entries via standard dedup.
- **SC-010**: A scan of a Stack project with `resolver: lts-22.0` produces the snapshot placeholder PURL `pkg:generic/stackage-lts-22.0@<sha>` carrying BOTH `mikebom:source-type = "hackage-snapshot"` AND `mikebom:stackage-resolver = "lts-22.0"` annotations so downstream tooling can correlate the snapshot bundle without parsing PURLs.
- **SC-011**: A scan of a fixture lockfile containing both boot libraries (`base ==4.18.0.0`, `text ==2.0.2`, `containers ==0.6.7`) and user-pulled Hackage packages (`aeson ==2.2.0.0`, `lens ==5.2.3`) produces 5 components total. Filtering on `mikebom:ghc-stdlib == "true"` retrieves exactly 3 (the boot libs); filtering on `mikebom:ghc-stdlib != "true"` (or property absent) retrieves exactly 2 (the user-pulled deps). Operator-actionable discrimination via the standard CDX property filter per FR-014.
- **SC-012**: A scan of a multi-stanza `*.cabal` declaring `library { build-depends: base, text }`, `executable cli { build-depends: base, my-lib, optparse-applicative }`, `test-suite spec { build-depends: base, my-lib, hspec }`, `benchmark perf { build-depends: base, my-lib, criterion }` (no lockfile, design-tier mode) produces 6 distinct components beyond the main-module (`base`, `text`, `optparse-applicative`, `hspec`, `criterion`, `my-lib` — the package's own library self-ref dedups by PURL). Filtering on CDX `scope == "excluded"` (dev-scope bridge per milestone-052) retrieves exactly `hspec` + `criterion` (test/benchmark deps); `base` appears in BOTH library AND test/bench stanzas but resolves to runtime-scope per the "most-binding wins" rule in Q2.

## Assumptions

- **cabal-install 2.4+ for `cabal.project.freeze`**: pre-2.4 cabal-install used an older freeze format (`cabal.config` with different shape). Out of scope.
- **Stack 2.1+ for `stack.yaml.lock`**: pre-2.1 Stack didn't emit lockfiles. Out of scope.
- **`cabal.project.freeze` / `stack.yaml.lock` are authoritative when present**: prefer either lockfile over `*.cabal` design-tier fallback. When both `cabal.project.freeze` AND `stack.yaml.lock` exist in the same project (rare but legal in projects that support both build tools), the reader emits components from BOTH and dedups by PURL via `seen_purls`.
- **No live `cabal` or `stack` invocation**: read-only on-disk parsing. The reader does NOT shell out to `cabal v2-build --dry-run`, `cabal v2-freeze`, `stack ls dependencies`, or any other build-tool subprocess. GHC + cabal + stack are not guaranteed to exist on the scan host.
- **Hackage is the registry**: the `hackage` PURL type is purl-spec-blessed and used verbatim. Stack-distributed packages from Stackage are still Hackage packages (Stackage is a curation layer on top of Hackage, not a separate registry); their `pkg:hackage/<name>@<version>` PURLs match Hackage exactly.
- **`mikebom:source-type` value set**: uses the `hackage-` prefix (`hackage-freeze` / `hackage-stack-lock` / `hackage-snapshot` / `hackage-cabal-design` / `hackage-main-module`) per the milestone-122/137-142 prefixed convention.
- **Regex parsing of `cabal.project.freeze`**: the freeze format is regular enough for regex extraction (one constraint per logical line after multi-line join). Brace/paren counting not required (no nested expressions in freeze syntax).
- **YAML parsing of `stack.yaml.lock`**: reuses the workspace `serde_yaml` crate (already a dep per the dart/cocoapods readers from milestones 137+139).
- **`*.cabal` is line-format with indentation**: each top-level field is `<keyword>: <value>` on its own line; stanzas are `<keyword> <args>` followed by indented `<keyword>: <value>` lines. Regex extraction handles `name:`, `version:`, `build-depends:`, and stanza-block detection per the milestone-140/141/142 DSL-extraction precedent.
- **`stack.yaml`'s `resolver:` is the authoritative source for the snapshot identifier**: even when `stack.yaml.lock` carries the resolver in nested fields, the human-friendly identifier (`lts-22.0`) lives in `stack.yaml`.
- **Hackage name casing**: Hackage's website displays names as the publisher submitted them (sometimes `MissingH`, sometimes `case-insensitive`). The `pkg:hackage/` PURL is conventionally lowercased per the purl-spec; mikebom lowercases per spec. Edge case: `MissingH` and `missingh` resolve to the same Hackage package; mikebom emits the lowercased form.

## Out of Scope

- **Live invocation of `cabal` / `stack` / GHC**: read-only metadata parse only.
- **Hpack `package.yaml` parsing**: Stack's Hpack alternative regenerates `*.cabal` from `package.yaml`. v1 reads only the generated `*.cabal`; operators using Hpack should regenerate before scanning.
- **Pre-cabal-2.4 `cabal.config`**: legacy freeze format, deprecated. Out of scope.
- **Pre-Stack-2.1 lockfile formats**: pre-2.1 Stack didn't lock; older versions out of scope.
- **`cabal-install` `plan.json`**: newer cabal-install (3.0+) writes a more detailed `plan.json` under `dist-newstyle/cache/`. Out of scope for v1 — `cabal.project.freeze` is the source of truth for source-tier emission.
- **Full Stackage snapshot expansion**: per FR-005, v1 emits ONE snapshot-resolver placeholder per project. Expanding the snapshot to its ~2500 individual packages would require fetching the snapshot manifest from stackage.org (network access — incompatible with FR-012). Deferred to v1.1 via an opt-in `--expand-stackage-snapshots` flag.
- **Custom Hackage mirrors**: `cabal.config`'s `repository:` directive ignored; deferred.
- **Per-package transitive dep edges**: v1 emits standalone components; inter-package edges deferred to v1.1.
- **License extraction**: deferred (`*.cabal` declares `license:` field — could be promoted to PackageDbEntry.licenses in a follow-up; defer for milestone-scope discipline).
- **Stackage snapshot offline cache / network expansion** (the `--expand-stackage-snapshots` flag) — v1 emits one snapshot-resolver placeholder per project per FR-005; full snapshot expansion stays out of scope.

## Dependencies and Constraints

- **Builds on milestone 002** (initial language-reader architecture).
- **Builds on milestone 140** (Elixir/Mix — regex-extracted DSL parsing + multi-line tokenization precedent; closest in shape to `*.cabal` parsing).
- **Builds on milestone 141** (Erlang/OTP — multi-tier emission with main-module + design-tier-fallback + profile-scope tagging + umbrella support).
- **Builds on milestone 142** (Scala/SBT — multi-project union discovery + dual lockfile-source dispatch pattern).
- **Reuses the existing source-tree walker** (`scan_fs::walk::safe_walk`).
- **Reuses workspace YAML parser**: `serde_yaml` (already a dep per milestone 137 dart + milestone 139 cocoapods).
- **Does NOT touch existing language readers** — Haskell support is strictly additive.
- **Does NOT introduce new external dependencies** — `regex` + `serde_yaml` + `serde_json` are workspace deps.

## Related

- Closes: #423 (Add Haskell ecosystem support (cabal.project.freeze + stack.yaml.lock))
- Adjacent: milestone 140 (Elixir/Mix — DSL parsing precedent), milestone 141 (Erlang/OTP — multi-tier emission template + functional-language-runtime ecosystem precedent), milestone 142 (Scala/SBT — multi-project union discovery + dual-lockfile-source template), milestone 070 (Maven — `pkg:<registry>/` PURL shape precedent — though here we use `pkg:hackage/` not `pkg:maven/`)
- Foundational reference: `mikebom-cli/src/scan_fs/package_db/erlang.rs` (closest sibling — functional ecosystem, multi-shape lockfile dispatch, main-module + design-tier dual mode), `mikebom-cli/src/scan_fs/package_db/scala.rs` (multi-project Q2 union discovery pattern)
