# Contract — Haskell component PURL shapes

Authoritative output-shape contract for components emitted by the milestone-143 Haskell reader. Test fixtures in `mikebom-cli/tests/haskell_*.rs` MUST assert exact PURL strings matching the shapes below.

## 1. Hackage exact-pin from `cabal.project.freeze`

**Input** (`cabal.project.freeze`):
```
constraints: aeson ==2.2.0.0
```

**Output**:
- `purl`: `pkg:hackage/aeson@2.2.0.0`
- `name`: `aeson`
- `version`: `2.2.0.0`
- `properties[]`:
  - `mikebom:source-type = "hackage-freeze"`
  - `mikebom:evidence-kind = "cabal-freeze"`

## 2. Hackage exact-pin from `stack.yaml.lock`

**Input**:
```yaml
packages:
  - completed:
      hackage: aeson-2.2.0.0@sha256:abc...,1234
    original:
      hackage: aeson-2.2.0.0
```

**Output**:
- `purl`: `pkg:hackage/aeson@2.2.0.0`
- `properties[]`:
  - `mikebom:source-type = "hackage-stack-lock"`
  - `mikebom:evidence-kind = "stack-yaml-lock"`

## 3. GHC boot library (Q1 — `mikebom:ghc-stdlib` annotation)

**Input** (`cabal.project.freeze`):
```
constraints: base ==4.18.0.0
```

**Output**:
- `purl`: `pkg:hackage/base@4.18.0.0`
- `properties[]`:
  - `mikebom:source-type = "hackage-freeze"`
  - `mikebom:evidence-kind = "cabal-freeze"`
  - `mikebom:ghc-stdlib = "true"` (per Q1 + FR-014)

## 4. Stackage snapshot placeholder — `lts-*` resolver

**Input** (`stack.yaml`):
```yaml
resolver: lts-22.0
```

**Input** (`stack.yaml.lock`):
```yaml
snapshots:
  - completed:
      sha256: 5cf7f73716ab1bff7c0e34dee5e6b69077c93e3c447bb71e2ae3a45f0b5c1018
      size: 654321
      url: "https://..."
    original:
      resolver: lts-22.0
```

**Output**:
- `purl`: `pkg:generic/stackage-lts-22.0@5cf7f73716ab1bff7c0e34dee5e6b69077c93e3c447bb71e2ae3a45f0b5c1018`
- `name`: `lts-22.0`
- `version`: `5cf7f73716ab1bff7c0e34dee5e6b69077c93e3c447bb71e2ae3a45f0b5c1018`
- `properties[]`:
  - `mikebom:source-type = "hackage-snapshot"`
  - `mikebom:evidence-kind = "stack-yaml-lock"`
  - `mikebom:stackage-resolver = "lts-22.0"`

## 5. Stackage snapshot placeholder — `nightly-*` resolver

**Input** (`stack.yaml`): `resolver: nightly-2024-01-15`

**Output**:
- `purl`: `pkg:generic/stackage-nightly-2024-01-15@<sha>`
- `properties[]`:
  - `mikebom:stackage-resolver = "nightly-2024-01-15"` (other properties identical to §4)

## 6. Stackage placeholder — `ghc-*` resolver (no Stackage bundle)

**Input** (`stack.yaml`): `resolver: ghc-9.6.4`

**Input** (`stack.yaml.lock`): no `snapshots[].completed.sha256` for this resolver (`ghc-*` resolvers are GHC-only, no bundle).

**Output**:
- `purl`: `pkg:generic/ghc-9.6.4@unspecified` (NOT prefixed with `stackage-`)
- `version`: `unspecified`
- `properties[]`:
  - `mikebom:stackage-resolver = "ghc-9.6.4"`

## 7. Design-tier dep from `*.cabal` (`%%`-equivalent + range constraint)

**Input** (`my-lib.cabal`, no lockfile):
```cabal
name: my-lib
version: 0.1.0

library
  build-depends: base >= 4.18 && < 4.20, text >= 2.0
```

**Output for `base` (boot library + range)**:
- `purl`: `pkg:hackage/base@>=_4.18_&&_<_4.20` (sanitized version)
- `properties[]`:
  - `mikebom:source-type = "hackage-cabal-design"`
  - `mikebom:evidence-kind = "cabal-pkg-descriptor"`
  - `mikebom:sbom-tier = "design"`
  - `mikebom:requirement-range = ">= 4.18 && < 4.20"` (raw range preserved)
  - `mikebom:ghc-stdlib = "true"` (Q1 — `base` is in the boot-library allowlist)

**Output for `text` (boot library + range)**:
- `purl`: `pkg:hackage/text@>=_2.0`
- `properties[]`: identical shape, includes `mikebom:ghc-stdlib = "true"` (text is in the allowlist)

## 8. Main-module from `*.cabal`

**Input** (`my-app/my-app.cabal`):
```cabal
name: my-app
version: 1.2.3
license: BSD-3-Clause

library
  build-depends: base, text, aeson
```

**Output (main-module)**:
- `purl`: `pkg:hackage/my-app@1.2.3`
- `name`: `my-app`
- `version`: `1.2.3`
- `properties[]`:
  - `mikebom:component-role = "main-module"`
  - `mikebom:source-type = "hackage-main-module"`

(Main-modules do NOT carry `mikebom:ghc-stdlib` or `mikebom:stackage-resolver` per data-model §4.5.)

## 9. Main-module fallback (missing `name:`/`version:` keywords)

**Input** (`/tmp/orphaned-pkg/orphaned-pkg.cabal` lacking `name:` and `version:`):
```cabal
license: BSD-3-Clause

library
  build-depends: base
```

**Output (main-module)**:
- `purl`: `pkg:hackage/orphaned-pkg@0.0.0-unknown`
  - `name` from parent-dir basename (`orphaned-pkg`)
  - `version` from `0.0.0-unknown` fallback

## 10. Multi-stanza `*.cabal` per-stanza scope tagging (Q2 + SC-012)

**Input** (`my-app/my-app.cabal`, no lockfile):
```cabal
name: my-app
version: 0.1.0

library
  build-depends: base, text

executable cli
  build-depends: base, my-app, optparse-applicative

test-suite spec
  build-depends: base, my-app, hspec

benchmark perf
  build-depends: base, my-app, criterion
```

**Output**: 6 components emit (deduped per Q2 union):

| Component | Source | CDX `scope` (milestone-052 bridge) | `mikebom:ghc-stdlib` |
|---|---|---|---|
| `pkg:hackage/base@unspecified` | union of library + executable + test-suite + benchmark | `required` (runtime — Q2 most-binding rule: appears in library/executable so runtime wins) | `true` |
| `pkg:hackage/text@unspecified` | library only | `required` (runtime) | `true` |
| `pkg:hackage/optparse-applicative@unspecified` | executable | `required` (runtime) | (absent) |
| `pkg:hackage/hspec@unspecified` | test-suite only | `excluded` (development → milestone-052 dev-scope bridge) | (absent) |
| `pkg:hackage/criterion@unspecified` | benchmark only | `excluded` (development) | (absent) |
| `pkg:hackage/my-app@unspecified` | executable + test-suite + benchmark self-ref | `required` (collapses with the main-module's PURL via `seen_purls` — likely doesn't separately emit) | (absent) |

Plus the main-module per §8.

## 11. Stack lockfile content-shape validation skip (Q3-style gate)

**Input** (a file named `stack.yaml.lock` containing valid YAML but no `snapshots:` array):
```yaml
unrelated: data
no_snapshots_array: true
```

**Output**: ZERO components emit. `tracing::warn!` diagnostic: `"haskell: failed to parse stack.yaml.lock — missing top-level snapshots array (Q3 content-shape gate)"`.

## 12. Hpack detect-and-warn (FR-015)

**Input**:
- `my-app/package.yaml` exists.
- `my-app/my-app.cabal` exists with first line: `-- This file has been generated from package.yaml by hpack version 0.36.0.`

**Output**:
- One main-module emits per §8 (from the `*.cabal`)
- Plus `tracing::warn!` diagnostic to stderr: `"haskell: Hpack-generated *.cabal detected alongside package.yaml — run 'hpack' to regenerate before scanning if package.yaml has been edited. cabal_path=<path> package_yaml=<path>"`

No SBOM-component-level annotation per the deferred decision in research §R6.

## 13. Cross-format byte-equivalence

For every emission in §1–§10, the same `purl` value MUST appear in:

- CycloneDX 1.6 output's `components[].purl`
- SPDX 2.3 output's `packages[].externalRefs[].referenceLocator` (where `referenceType == "purl"`)
- SPDX 3.0.1 output's `@graph[].software_packageUrl`

Per the milestone-013 format-parity-enforcement work, the `parity-check` subcommand verifies this invariant for milestone-143 test fixtures.

## 14. SBOM-format property name mapping

| Field | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| `mikebom:source-type` | `properties[].name = "mikebom:source-type"` | `annotations[]` with comment envelope | document-scope `Annotation` with envelope |
| `mikebom:evidence-kind` | `properties[]` | `annotations[]` | `Annotation` |
| `mikebom:sbom-tier` | `properties[]` | `annotations[]` | `Annotation` |
| `mikebom:requirement-range` | `properties[]` | `annotations[]` | `Annotation` |
| `mikebom:lifecycle-scope` | NATIVE: `components[].scope` | NATIVE: `relationships[].relationshipType = "DEV_DEPENDENCY_OF"` etc. | NATIVE: `LifecycleScopeType` |
| `mikebom:component-role` | `properties[]` | `annotations[]` | `Annotation` |
| `mikebom:ghc-stdlib` | `properties[]` | `annotations[]` | `Annotation` |
| `mikebom:stackage-resolver` | `properties[]` | `annotations[]` | `Annotation` |

Per Constitution Principle V, `mikebom:lifecycle-scope` flows through the milestone-052 native-field path. The other `mikebom:*` properties remain as standalone annotations because no spec-native carrier exists for the semantic per research §R6.
