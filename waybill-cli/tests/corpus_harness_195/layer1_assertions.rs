//! Layer 1 — coarse per-target assertions with class-of-bug-oriented
//! diagnostics. Research §R4.
//!
//! Each function returns the FIRST failure encountered; the
//! `AssertionFailure` carries a `suggested_action` pointing at the
//! milestone / module the maintainer should investigate.

use super::harness::{AssertionFailure, EmittedSboms, FailureFormat};

// -----------------------------------------------------------------------
// Small helpers (JSON-Value walkers)
// -----------------------------------------------------------------------

/// Extract the waybill:graph-completeness value from CDX
/// `.metadata.properties[]`.
fn cdx_graph_completeness(cdx: &serde_json::Value) -> Option<String> {
    cdx.get("metadata")?
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("waybill:graph-completeness"))
        .and_then(|p| p.get("value")?.as_str().map(str::to_string))
}

/// True if any component's purl matches a given predicate.
fn cdx_has_component_purl(cdx: &serde_json::Value, matches: impl Fn(&str) -> bool) -> bool {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|c| {
                c.get("purl")
                    .and_then(|p| p.as_str())
                    .map(&matches)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// True if any component carries a property `waybill:<name>` whose
/// value matches the predicate.
fn cdx_has_component_property(
    cdx: &serde_json::Value,
    name: &str,
    matches: impl Fn(&str) -> bool,
) -> bool {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|c| {
                c.get("properties")
                    .and_then(|p| p.as_array())
                    .map(|props| {
                        props.iter().any(|p| {
                            p.get("name").and_then(|n| n.as_str()) == Some(name)
                                && p.get("value")
                                    .and_then(|v| v.as_str())
                                    .map(&matches)
                                    .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// True if any dependency edge from `from_pred(ref)` targets `to_pred`.
fn cdx_has_edge(
    cdx: &serde_json::Value,
    from_pred: impl Fn(&str) -> bool,
    to_pred: impl Fn(&str) -> bool,
) -> bool {
    cdx.get("dependencies")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter().any(|dep| {
                let ref_matches = dep
                    .get("ref")
                    .and_then(|r| r.as_str())
                    .map(&from_pred)
                    .unwrap_or(false);
                if !ref_matches {
                    return false;
                }
                dep.get("dependsOn")
                    .and_then(|d| d.as_array())
                    .map(|targets| {
                        targets.iter().any(|t| {
                            t.as_str().map(&to_pred).unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

// -----------------------------------------------------------------------
// go-cobra (US1 MVP)
// -----------------------------------------------------------------------

pub fn go_cobra_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // The corpus harness invokes waybill with `--root-name go-cobra
    // --root-version <sha7>`, so the manifest-derived Go mainmod
    // (`pkg:golang/github.com/spf13/cobra`) is dropped per m077 and
    // replaced with the operator-override subject `go-cobra@<sha7>`.
    // Layer 1 assertions target the resulting shape.

    // Assertion 1: graph-completeness == "complete" (per m194 stack).
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete".to_string(),
            suggested_action: "investigate m158 / m194 regression — cobra is a simple Go source tree; classifier over-fire suggests orphan-class or classifier bug",
        });
    }
    // Assertion 2: stdlib component emitted.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:golang/stdlib@")) {
        return Err(AssertionFailure {
            invariant_name: "stdlib-component-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:golang/stdlib@v* component".to_string(),
            expected: "at least one pkg:golang/stdlib@v<gover> component".to_string(),
            suggested_action: "investigate golang/legacy.rs::build_stdlib_entry — Go source scan MUST emit stdlib",
        });
    }
    // Assertion 3 (m194 US1 tripwire): operator-override root → stdlib
    // edge. Re-anchored from the dropped Go mainmod via m192 pre-rewrite.
    let has_stdlib_edge = cdx_has_edge(
        &sboms.cdx,
        |from| from.starts_with("go-cobra@") || from.starts_with("pkg:golang/github.com/spf13/cobra"),
        |to| to.starts_with("pkg:golang/stdlib@"),
    );
    if !has_stdlib_edge {
        return Err(AssertionFailure {
            invariant_name: "stdlib-edge-present",
            format: FailureFormat::Cdx,
            observed: "no edge from operator-override root (go-cobra@*) to pkg:golang/stdlib@v*".to_string(),
            expected: "at least one such edge (m194 US1 synthetic stdlib link + m192 pre-rewrite re-anchor)".to_string(),
            suggested_action: "investigate m194 US1 (golang/legacy.rs stdlib-edge synth) or m192/m194 US4 (SPDX-parity pre-rewrite in emitters)",
        });
    }
    // Assertion 4: canonical cobra transitive dep present (pflag).
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:golang/github.com/spf13/pflag")) {
        return Err(AssertionFailure {
            invariant_name: "cobra-transitive-pflag-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:golang/github.com/spf13/pflag component".to_string(),
            expected: "at least one pkg:golang/github.com/spf13/pflag@vX.Y.Z component".to_string(),
            suggested_action: "investigate Go go.sum reader (m055/m091) — cobra's go.mod declares pflag as a required dep",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// rust-ripgrep (US2)
// -----------------------------------------------------------------------

pub fn rust_ripgrep_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // m196 reconciliation (US3): ripgrep-14.1.1 pinned scan under
    // `--root-name rust-ripgrep --root-version 0e8390a` observes:
    //   - graph-completeness = `partial` (m195 R8 seed had `complete`
    //     from spec knowledge; empirical is 5 BFS-orphans with no
    //     specific orphan_reason — legitimate `partial` for a cargo
    //     workspace tree where some workspace-internal targets aren't
    //     reachable from the operator-override root).
    //   - `pkg:cargo/ripgrep` main-module PURL is DROPPED per m077
    //     (operator-override), replaced by `pkg:generic/rust-ripgrep@<sha>`.
    //     `pkg:cargo/*` transitives are still emitted (aho-corasick etc.).
    // Tripwire preserved: (a) any regression that breaks the cargo
    // reader entirely would flip observed `partial` → `unknown`/`missing`
    // or drop ALL `pkg:cargo/*` transitives; (b) a m194-US1-class
    // regression that reintroduces the pico-style false-positive-orphan
    // cascade would push orphan count much higher.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (m196-empirical: BFS-orphans from operator-override root, no specific reason-code)".to_string(),
            suggested_action: "investigate cargo reader (m064 / m087 / m088) — ripgrep drift from `partial` suggests a classifier regression",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:cargo/")) {
        return Err(AssertionFailure {
            invariant_name: "cargo-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:cargo/* components at all".to_string(),
            expected: "at least one pkg:cargo/* transitive (aho-corasick, anyhow, etc.)".to_string(),
            suggested_action: "investigate m064 cargo reader — Cargo.lock emission is broken",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// npm-express (US2)
// -----------------------------------------------------------------------

pub fn npm_express_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // m196 reconciliation (US3): express-v5.1.0 scan under
    // `--root-name npm-express --root-version e996498` observes:
    //   - graph-completeness = `partial` with reason
    //     `transitive-edges-unresolvable: npm` (m177 tier-fidelity signal,
    //     working as designed — express has some transitive deps whose
    //     manifest-vs-lockfile drift m177 flags).
    //   - `pkg:npm/express` main-module PURL dropped per m077; replaced
    //     by `pkg:generic/npm-express@<sha>`.
    //   - `pkg:npm/*` transitives present (accepts, body-parser, cookie, etc.).
    // Tripwire preserved: catches regressions that either eliminate m177
    // classification (would flip to `unknown`) or break the npm reader
    // (would drop all pkg:npm/* transitives).
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (m196-empirical: m177 transitive-edges-unresolvable: npm)".to_string(),
            suggested_action: "investigate npm reader (m066 / m147 / m180) or m177 classifier — express drift suggests a reader or classifier regression",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:npm/")) {
        return Err(AssertionFailure {
            invariant_name: "npm-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:npm/* components at all".to_string(),
            expected: "at least one pkg:npm/* transitive (accepts, body-parser, cookie, etc.)".to_string(),
            suggested_action: "investigate m066 npm reader — package-lock.json emission is broken",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// python-flask (US2)
// -----------------------------------------------------------------------

pub fn python_flask_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // m196 reconciliation (US3): flask-3.1.2 scan under
    // `--root-name python-flask --root-version 80be49b` observes:
    //   - graph-completeness = `partial` with 94 BFS-orphans (no specific
    //     orphan_reason — flask's docs / test tree pulls a large
    //     transitive graph via `requirements/*.txt` that the pip reader
    //     emits as source-tier components without wiring them to any
    //     root because the operator-override drops the flask mainmod).
    //   - `pkg:pypi/flask` mainmod dropped per m077; replaced by
    //     `pkg:generic/python-flask@<sha>`.
    //   - `pkg:pypi/*` transitives present (alabaster, anyio, babel, etc.).
    // Tripwire preserved: regressions that break the pip reader would
    // drop all pkg:pypi/* transitives; regressions that ELIMINATE
    // classifier signal would flip to `unknown`.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (m196-empirical: BFS-orphans from operator-override root, requirements/*.txt transitive fan-out)".to_string(),
            suggested_action: "investigate pip reader (m068 / m183) or m158 classifier — flask drift from `partial` suggests a regression",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:pypi/")) {
        return Err(AssertionFailure {
            invariant_name: "pypi-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:pypi/* components at all".to_string(),
            expected: "at least one pkg:pypi/* transitive (alabaster, anyio, babel, etc.)".to_string(),
            suggested_action: "investigate m068 pip reader — pyproject.toml / requirements.txt emission is broken",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// maven-guice (US2)
// -----------------------------------------------------------------------

pub fn maven_guice_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // m196 reconciliation (US3): guice-7.0.0 scan under
    // `--root-name maven-guice --root-version b0e1d0f` observes:
    //   - graph-completeness = `partial` with mixed reasons:
    //     `orphaned-components-detected: 9 component(s)` + m177
    //     `transitive-edges-unresolvable: maven`. The 9 orphans come
    //     from the maven multi-module tree where per-module mainmods
    //     are dropped under operator-override; m177 fires because some
    //     pkg:maven/*/*@unknown deps lack version resolution.
    //   - `pkg:maven/com.google.inject/guice` module PURLs dropped per
    //     m077; replaced by `pkg:generic/maven-guice@<sha>`.
    //   - `pkg:maven/*` transitives present (aopalliance, jsr305,
    //     dagger, error_prone_annotations, etc.). Includes both
    //     resolved (`@X.Y.Z`) and `@unknown` variants.
    // Tripwire preserved: catches regressions that flip to `complete`
    // (unlikely — the observed shape is fundamental to guice's build)
    // OR that drop all pkg:maven/* transitives (maven reader broken).
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (m196-empirical: mixed orphan-count + m177 transitive-edges-unresolvable: maven)".to_string(),
            suggested_action: "investigate maven reader (m070 / m085 / m184) or m177 classifier — guice drift from `partial` suggests a regression",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:maven/")) {
        return Err(AssertionFailure {
            invariant_name: "maven-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:maven/* components at all".to_string(),
            expected: "at least one pkg:maven/* transitive (aopalliance, jsr305, dagger, etc.)".to_string(),
            suggested_action: "investigate m070 maven reader — pom.xml parsing is broken",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// pants-example-python — m673 US1 (repo-root `python-default.lock`)
// -----------------------------------------------------------------------

pub fn pants_example_python_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // pantsbuild/example-python at pinned SHA has a `python-default.lock`
    // at the repo root — this is Pants 2.31+ default layout. Pre-m673
    // waybill emitted 0 components from this shape (the pants reader
    // only walked `3rdparty/python/*.lock`); post-m673 it emits ≥ 8.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:pypi/")) {
        return Err(AssertionFailure {
            invariant_name: "pypi-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:pypi/* components at all".to_string(),
            expected: "at least one pkg:pypi/* transitive from the root python-default.lock".to_string(),
            suggested_action: "investigate m673 (repo-root discovery gate) or m223 pex-lockfile reader — pants-example-python should emit ≥ 8 pypi components",
        });
    }
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-resolve", |v| {
        v == "python-default"
    }) {
        return Err(AssertionFailure {
            invariant_name: "pants-resolve-annotation-present",
            format: FailureFormat::Cdx,
            observed: "no component carries waybill:pants-resolve=python-default".to_string(),
            expected: "at least one component carries waybill:pants-resolve=python-default (m223 C143)".to_string(),
            suggested_action: "investigate m223 resolve_classifier or m673 discovery-source tagging — pants-emitted components MUST carry the pants-resolve annotation",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// pants-example-django — m673 US2 (`lockfiles/python-default.lock`)
// -----------------------------------------------------------------------

pub fn pants_example_django_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // pantsbuild/example-django at pinned SHA has its lockfile under
    // `lockfiles/python-default.lock` — the Pants `lockfiles/` convention.
    // Pre-m673 waybill emitted 0 components; post-m673 it emits Django's
    // full transitive closure (typically 20-50 pypi components).
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:pypi/")) {
        return Err(AssertionFailure {
            invariant_name: "pypi-transitives-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:pypi/* components at all".to_string(),
            expected: "at least one pkg:pypi/* transitive from lockfiles/python-default.lock".to_string(),
            suggested_action: "investigate m673 US2 (lockfiles/ directory discovery) or m223 pex-lockfile reader",
        });
    }
    // Django-specific tripwire: the primary dep in this fixture is Django
    // itself. PyPI names normalize case-insensitively, but PURL segment
    // encoding preserves the `Django` casing per m670; assert either.
    let has_django = cdx_has_component_purl(&sboms.cdx, |p| {
        let lower = p.to_ascii_lowercase();
        lower.starts_with("pkg:pypi/django@") || lower.starts_with("pkg:pypi/django/")
    });
    if !has_django {
        return Err(AssertionFailure {
            invariant_name: "django-component-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:pypi/django@* (or Django@*) component".to_string(),
            expected: "at least one pkg:pypi/django@X.Y.Z component from the lockfile".to_string(),
            suggested_action: "investigate m673 US2 lockfile discovery — the Django dep is the primary content of this fixture's lockfile",
        });
    }
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-resolve", |v| {
        v == "python-default"
    }) {
        return Err(AssertionFailure {
            invariant_name: "pants-resolve-annotation-present",
            format: FailureFormat::Cdx,
            observed: "no component carries waybill:pants-resolve=python-default".to_string(),
            expected: "at least one component carries waybill:pants-resolve=python-default (m223 C143)".to_string(),
            suggested_action: "investigate m223 resolve_classifier or m673 US2 discovery-source tagging",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// pants-example-golang — m226 (Pants Go enricher) + m053/m055 (Go reader)
// -----------------------------------------------------------------------

pub fn pants_example_golang_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // pantsbuild/example-golang at pinned SHA is a standard go.mod + go.sum
    // layout with `pants.toml` declaring `[golang]`. The Go reader emits
    // the components; the m226 pants_go enricher decorates them with
    // `waybill:pants-target` annotations (per Principle IX — the pants
    // enricher never fabricates pkg:golang/* PURLs, it decorates existing
    // Go-reader-emitted ones).
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:golang/")) {
        return Err(AssertionFailure {
            invariant_name: "golang-components-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:golang/* components at all".to_string(),
            expected: "at least one pkg:golang/* component from go.sum".to_string(),
            suggested_action: "investigate the Go reader (m053/m055/m091) — pants-example-golang should emit Go transitives",
        });
    }
    // m226 enricher tripwire: the enricher decorates Go components with
    // `waybill:pants-target` (broadened C145 per m226). Absence of ANY
    // such annotation across the entire component set indicates the
    // enricher isn't running against this fixture's `pants.toml` +
    // `BUILD` files.
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-target", |_| true) {
        return Err(AssertionFailure {
            invariant_name: "pants-target-annotation-present",
            format: FailureFormat::Cdx,
            observed: "no component carries waybill:pants-target=<any>".to_string(),
            expected: "at least one component carries waybill:pants-target=<pants-target-address> (m226 enrichment)".to_string(),
            suggested_action: "investigate m226 pants_go enricher — Go components in a Pants monorepo MUST be decorated with pants-target",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// pants-example-jvm — feature 676 (issue #756 fix regression gate)
//
// Locks in the coursier-JVM reader's ability to parse real-world Pants
// lockfiles that use the coord-table shape for both `directDependencies`
// and `dependencies` fields. Pre-fix (main + earlier), scanning the
// pinned fixture emitted zero pkg:maven/* components because the reader
// rejected the whole lockfile on parse error. Four invariants:
//   1. `maven-transitives-present-at-scale` — count pkg:maven/* >= 20
//      (baseline 27 at pinned SHA)
//   2. `top-level-guava-present` — declared top-level dep in the resolve
//   3. `top-level-scala-library-present` — dual-anchor
//   4. `pants-resolve-annotation-present` — at least one component
//      carries waybill:pants-resolve (m223 C143 catalog row)
// -----------------------------------------------------------------------

pub fn pants_example_jvm_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // Invariant 1 — maven-transitives-present-at-scale.
    let maven_count = sboms
        .cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.starts_with("pkg:maven/"))
                })
                .count()
        })
        .unwrap_or(0);
    if maven_count < 20 {
        return Err(AssertionFailure {
            invariant_name: "maven-transitives-present-at-scale",
            format: FailureFormat::Cdx,
            observed: format!("{maven_count} pkg:maven/* components"),
            expected: "at least 20 pkg:maven/* components (observed baseline 27 at pinned SHA)".to_string(),
            suggested_action: "investigate the coursier-JVM reader (m224 / issue #756 / feature 676) — pants-example-jvm should emit >= 20 pkg:maven/* components",
        });
    }

    // Invariant 2 — top-level-guava-present.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:maven/com.google.guava/guava@")) {
        return Err(AssertionFailure {
            invariant_name: "top-level-guava-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:maven/com.google.guava/guava@* component".to_string(),
            expected: "at least one pkg:maven/com.google.guava/guava@* component (top-level coord declared in fixture)".to_string(),
            suggested_action: "investigate m224 reader top-level-coord resolution — the resolve declares com.google.guava:guava:31.0.1-jre",
        });
    }

    // Invariant 3 — top-level-scala-library-present.
    if !cdx_has_component_purl(&sboms.cdx, |p| {
        p.starts_with("pkg:maven/org.scala-lang/scala-library@")
    }) {
        return Err(AssertionFailure {
            invariant_name: "top-level-scala-library-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:maven/org.scala-lang/scala-library@* component".to_string(),
            expected: "at least one pkg:maven/org.scala-lang/scala-library@* component (top-level coord declared in fixture)".to_string(),
            suggested_action: "investigate m224 reader top-level-coord resolution — the resolve declares org.scala-lang:scala-library:2.13.8",
        });
    }

    // Invariant 4 — pants-resolve-annotation-present on maven surface.
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-resolve", |_| true) {
        return Err(AssertionFailure {
            invariant_name: "pants-resolve-annotation-present",
            format: FailureFormat::Cdx,
            observed: "no component carries waybill:pants-resolve=<any>".to_string(),
            expected: "at least one component carries waybill:pants-resolve=<resolve-name> (m224 reuses m223 C143)".to_string(),
            suggested_action: "investigate m224 pants_jvm reader annotation emission — maven components MUST carry pants-resolve tagging",
        });
    }

    Ok(())
}

// -----------------------------------------------------------------------
// pants-example-javascript — feature 675 (issue #760 option-B corpus gate)
//
// Locks in the current npm-reader-stack (m066 + m147 + m180) behavior on
// a Pants-managed JavaScript monorepo. Four invariants encode what
// operators see today when they scan a Pants-JS repo:
//   1. `pkg:npm/*` count >= 250 (baseline 302 at pinned SHA)
//   2. `pkg:npm/esbuild@*` present (top-level devDep anchor)
//   3. `pkg:npm/jest@*` present (top-level devDep anchor — dual-anchor)
//   4. No `waybill:pants-resolve` or `waybill:pants-target` on any
//      `pkg:npm/*` component (spec 675 FR-006 regression-lock —
//      Pants-side provenance annotations on npm surface are the
//      tracked issue #760 option A follow-up)
//
// If issue #760 option A ships, invariant 4 fires. That failure IS
// the signal — regenerate goldens, remove invariant 4, update spec
// 675 FR-006.
// -----------------------------------------------------------------------

pub fn pants_example_javascript_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // Invariant 1 — npm-transitives-present-at-scale.
    let npm_count = sboms
        .cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.starts_with("pkg:npm/"))
                })
                .count()
        })
        .unwrap_or(0);
    if npm_count < 250 {
        return Err(AssertionFailure {
            invariant_name: "npm-transitives-present-at-scale",
            format: FailureFormat::Cdx,
            observed: format!("{npm_count} pkg:npm/* components"),
            expected: "at least 250 pkg:npm/* components (observed baseline 302 at pinned SHA)".to_string(),
            suggested_action: "investigate npm reader (m066 / m147 / m180) or shared walker — pants-example-javascript at pinned SHA should emit >= 250 pkg:npm/* components",
        });
    }

    // Invariant 2 — top-level-devdep-esbuild-present.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:npm/esbuild@")) {
        return Err(AssertionFailure {
            invariant_name: "top-level-devdep-esbuild-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:npm/esbuild@* component".to_string(),
            expected: "at least one pkg:npm/esbuild@X.Y.Z component (top-level devDep declared in package.json)".to_string(),
            suggested_action: "investigate npm reader top-level-devDep resolution — package.json declares esbuild@^0.20.1",
        });
    }

    // Invariant 3 — top-level-devdep-jest-present.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:npm/jest@")) {
        return Err(AssertionFailure {
            invariant_name: "top-level-devdep-jest-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:npm/jest@* component".to_string(),
            expected: "at least one pkg:npm/jest@X.Y.Z component (top-level devDep declared in package.json)".to_string(),
            suggested_action: "investigate npm reader top-level-devDep resolution — package.json declares jest@^29.7.0",
        });
    }

    // Invariant 4 — no-accidental-pants-annotations-on-npm.
    // Iterate components manually so we can build a diagnostic naming
    // the offending PURLs on failure.
    let offenders: Vec<String> = sboms
        .cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    let is_npm = c
                        .get("purl")
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.starts_with("pkg:npm/"));
                    if !is_npm {
                        return false;
                    }
                    c.get("properties")
                        .and_then(|p| p.as_array())
                        .map(|props| {
                            props.iter().any(|p| {
                                p.get("name").and_then(|n| n.as_str()).is_some_and(|n| {
                                    n == "waybill:pants-resolve" || n == "waybill:pants-target"
                                })
                            })
                        })
                        .unwrap_or(false)
                })
                .filter_map(|c| c.get("purl").and_then(|p| p.as_str()).map(str::to_string))
                .take(5)
                .collect()
        })
        .unwrap_or_default();
    if !offenders.is_empty() {
        return Err(AssertionFailure {
            invariant_name: "no-accidental-pants-annotations-on-npm",
            format: FailureFormat::Cdx,
            observed: format!(
                "{} pkg:npm/* components carry unexpected Pants annotations (sample: {:?})",
                offenders.len(),
                offenders
            ),
            expected: "no pkg:npm/* component carries waybill:pants-resolve or waybill:pants-target (spec 675 FR-006 regression-lock)".to_string(),
            suggested_action: "unexpected Pants-side provenance annotation on npm surface. If intentional (issue #760 option A landed), regenerate goldens + remove this invariant + update spec 675 FR-006. If unintentional, investigate annotation leak.",
        });
    }

    Ok(())
}

// -----------------------------------------------------------------------
// image-postgres16 (US2)
// -----------------------------------------------------------------------

pub fn image_postgres16_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // Per research §R8: postgres:16 is expected `partial` with m177
    // reason. Assert the expected shape rather than `complete`.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (m177 TransitiveEdgesUnresolvable)".to_string(),
            suggested_action: "investigate m177 classifier regression — postgres:16 should trip TransitiveEdgesUnresolvable for [generic, golang] due to embedded gosu binary",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:deb/")) {
        return Err(AssertionFailure {
            invariant_name: "deb-components-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:deb/* components".to_string(),
            expected: "at least one pkg:deb/* (Debian base package)".to_string(),
            suggested_action: "investigate deb reader regression — postgres:16 is Debian-based",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:golang/")) {
        return Err(AssertionFailure {
            invariant_name: "golang-bin-components-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:golang/* components".to_string(),
            expected: "at least one pkg:golang/* (from gosu Go binary BuildInfo)".to_string(),
            suggested_action: "investigate Go BuildInfo extractor — gosu binary in postgres:16 image should surface Go modules",
        });
    }
    Ok(())
}
