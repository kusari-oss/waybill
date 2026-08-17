# Data Model: Universalize `waybill:unresolved-reason`

**Milestone**: 236 | **Date**: 2026-08-16

## Entity: `UnresolvedReason` (annotation value)

**Rust type**: `String` (raw; no newtype per research R1)
**Wire type**: JSON string
**Wire location**: `PackageDbEntry.extra_annotations["waybill:unresolved-reason"] = serde_json::Value::String(reason)`

### Fields

| Field | Type | Notes |
|---|---|---|
| value | `String` | The reason string. ASCII English. <200 chars. No PII / paths / credentials. |

### Validation rules

- **Non-empty**: reader-side assertion (empty strings are a bug; caught by per-reader unit tests per FR-009).
- **No PII / paths / credentials**: enforced by a substring-blacklist test that greps every emission call-site and every fixture-emitted SBOM (per FR-010).
- **Stability**: within a waybill build, same fixture input → byte-identical reason string; across releases display-only per Q1.

### Relationships

- **Attached to**: `PackageDbEntry` (Rust) → CDX `component`, SPDX 2.3 `Package`, SPDX 3 `software_Package` (wire).
- **Presence conditional**: MUST be present iff the same component carries `waybill:sbom-tier: "design"`. MUST NOT appear on components with `waybill:sbom-tier: "source"` or missing the tier annotation entirely.

## Entity: Reader (call-site metadata)

Each of the 18 reader files has one or more design-tier emission call-sites. Concretely:

| Reader (US1) | File | Call-site pattern |
|---|---|---|
| cargo | `cargo.rs` | manifest-only path (Cargo.toml declares dep, no Cargo.lock hit) |
| gem | `gem.rs` | Gemfile path (no Gemfile.lock hit) |
| maven | `maven.rs` | pom.xml `<dependency>` without resolvable `<version>` |
| npm/mod | `npm/mod.rs` | package.json declaration, no lockfile hit across all 4 lockfile variants |
| npm/walk | `npm/walk.rs` | workspace-member emission without lockfile-resolved version |
| pip | `pip/requirements_txt.rs` | requirements.txt entry without version specifier |

| Reader (US2) | File | Call-site pattern |
|---|---|---|
| kotlin_dsl | `kotlin_dsl/mod.rs` | Kotlin DSL declaration via `--include-declared-deps` |
| kotlin_dsl/build_script | `kotlin_dsl/build_script.rs` | Kotlin DSL buildscript-classpath declaration |
| scala | `scala.rs` | build.sbt declaration, no coursier-resolved lockfile |
| gradle_static | `gradle/static_parser.rs` | build.gradle Groovy declaration, US2 cache miss |
| helm | `helm.rs` | Chart.yaml dependency without `--helm-render` |
| yocto | `yocto/recipe.rs` | .bb recipe without PV/PR resolution |

| Reader (US3) | File | Call-site pattern |
|---|---|---|
| cocoapods | `cocoapods.rs` | Podfile declaration without Podfile.lock |
| composer | `composer.rs` | composer.json without composer.lock |
| dart | `dart.rs` | pubspec.yaml without pubspec.lock |
| elixir | `elixir.rs` | mix.exs without mix.lock |
| erlang | `erlang.rs` | rebar.config without rebar.lock |
| haskell | `haskell.rs` | stack.yaml / .cabal without lockfile fallback |
| pants_shell | `pants_shell/component_emit.rs` | shell tool pin without version specifier |
| pants_go | `pants_go/mod.rs` | expected_version without matching Go corpus component |

## Injection contract

At each call-site, the pattern MUST be:

```rust
// existing:
extra_annotations.insert(
    "waybill:sbom-tier".to_string(),
    serde_json::Value::String("design".to_string()),
);

// NEW (add at the same call-site, immediately after or immediately before):
extra_annotations.insert(
    "waybill:unresolved-reason".to_string(),
    serde_json::Value::String("<reader-specific reason>".to_string()),
);
```

Concretely: whichever line already sets `waybill:sbom-tier: "design"` gets a sibling line setting `waybill:unresolved-reason`. Same map, same call, adjacent inserts.

## State transitions

None — annotation is a stateless per-component value assigned at read-time.
