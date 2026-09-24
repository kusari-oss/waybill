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

    // Assertion 1: graph-completeness == "partial".
    //
    // Milestone 866 (#880) changed this from `complete`, and the change
    // is the point rather than a regression to tolerate.
    //
    // The corpus scans offline with no module cache, so Go transitive
    // resolution cannot run. Pre-866 the go.sum fallback attached every
    // unresolved module to the main module; that made each one reachable,
    // so the reachability check found zero orphans and reported
    // `complete` over a graph waybill had itself filled in. Those edges
    // claimed `go.mod` as their source and `go.mod` does not contain
    // them.
    //
    // With them removed, `blackfriday` and `check.v1` have no incoming
    // edge — correctly, since nothing in the tree says where they belong
    // — and the existing orphan classifier reports `partial`. A cobra
    // scan in this mode that says `complete` again means the fabrication
    // is back.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial".to_string(),
            suggested_action: "offline cobra cannot resolve transitive edges, so `partial` is correct. `complete` means edges are being asserted that no go.mod declares — see milestone 866 / #880 and specs/866-go-graph-completeness/contracts/edge-backing.md",
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
    // Issue #892 — this expected `partial` until 2026-09-17, and the
    // expectation encoded a defect rather than a property of the target.
    //
    // The classifier's edge filter omitted `OptionalDependsOn`, added in
    // milestone 179 for optional and extras dependencies. CycloneDX emits
    // every variant as an ordinary `dependsOn` edge, so ripgrep's five
    // "orphans" were components the emitted document reached and the
    // classifier could not see. With the filter corrected, walking
    // `dependencies[]` from the root finds **zero** unreachable components of
    // 61, so `complete` is the accurate verdict.
    //
    // Verified by walking the emitted graph rather than by trusting the
    // annotation — the annotation is the thing that was wrong.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete (#892: every component reachable from the operator-override root once OptionalDependsOn counts as an edge)".to_string(),
            suggested_action: "walk `dependencies[]` from `metadata.component.bom-ref` and compare against the reported orphan count before assuming a reader regression — a disagreement between the two is a classifier bug, not a coverage loss",
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
    // #911 — membership is a lex-sorted JSON array, carried as JSON-in-string
    // in CycloneDX. Decode and look for the resolve inside it rather than
    // comparing the whole value, which would silently stop matching the
    // moment a second resolve pins the same package.
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-resolve", |v| {
        serde_json::from_str::<serde_json::Value>(v)
            .ok()
            .and_then(|parsed| {
                Some(
                    parsed
                        .as_array()?
                        .iter()
                        .any(|x| x.as_str() == Some("python-default")),
                )
            })
            .unwrap_or(false)
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
    // #911 — membership is a lex-sorted JSON array, carried as JSON-in-string
    // in CycloneDX. Decode and look for the resolve inside it rather than
    // comparing the whole value, which would silently stop matching the
    // moment a second resolve pins the same package.
    if !cdx_has_component_property(&sboms.cdx, "waybill:pants-resolve", |v| {
        serde_json::from_str::<serde_json::Value>(v)
            .ok()
            .and_then(|parsed| {
                Some(
                    parsed
                        .as_array()?
                        .iter()
                        .any(|x| x.as_str() == Some("python-default")),
                )
            })
            .unwrap_or(false)
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

// -----------------------------------------------------------------------
// haskell-aeson (#898)
// -----------------------------------------------------------------------

/// The first Haskell target in either corpus.
///
/// That absence is what let the #891 defects live as long as they did, and it
/// recurred: between 2026-09-21 and 2026-09-23 this reader needed four separate
/// fixes (#937, #936, #938, #943), every one of them found by scanning a real
/// repository by hand rather than by a gate. `aeson` is chosen so those four
/// classes are all exercised by one document, checked nightly instead of
/// measured once.
///
/// Measured at the pinned revision under the harness invocation (`--root-name
/// haskell-aeson --root-version 682162c6`), which matters — the operator
/// override drops the ecosystem main-module PURL per m077:
///
///   - 63 components, 61 of them `pkg:hackage/*`, 61 design-tier
///   - graph-completeness `partial`, reason `transitive-edges-unresolvable:
///     hackage` — honest, since the repository carries no `cabal.project.freeze`
///     and no `stack.yaml.lock`, so transitive resolution is genuinely
///     unavailable rather than merely unattempted
///   - six `*.cabal` manifests across six directories, one of them reached
///     through a `benchmarks/examples -> ../examples` symlink
pub fn haskell_aeson_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    // Tripwire 1 — the reader produces Hackage identifiers at all. Any
    // regression that breaks `.cabal` parsing outright lands here first.
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:hackage/")) {
        return Err(AssertionFailure {
            invariant_name: "hackage-components-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:hackage/* components at all".to_string(),
            expected: "61 pkg:hackage/* components from six *.cabal manifests".to_string(),
            suggested_action: "investigate the m143 haskell reader — `.cabal` build-depends extraction is broken",
        });
    }

    // Tripwire 2 (#943) — Hackage names are case-sensitive, and this repository
    // declares three that carry capitals. A lowercasing regression makes
    // `QuickCheck` into `quickcheck`, which resolves to nothing; it also makes
    // `Diff` into `diff`, which resolves to a REAL BUT DIFFERENT package and so
    // fails silently with a confident wrong answer. This assertion is the only
    // thing in the corpus that would notice.
    if !cdx_has_component_purl(&sboms.cdx, |p| p == "pkg:hackage/QuickCheck") {
        return Err(AssertionFailure {
            invariant_name: "hackage-purl-case-preserved",
            format: FailureFormat::Cdx,
            observed: "pkg:hackage/QuickCheck absent (check for pkg:hackage/quickcheck)".to_string(),
            expected: "pkg:hackage/QuickCheck, byte-for-byte as declared in aeson.cabal".to_string(),
            suggested_action: "a case-folding regression in the haskell reader (#943). Hackage is case-sensitive: `quickcheck` 404s and `diff` names a different package than `Diff`. Case may be folded where names are MATCHED, never where identity is MINTED",
        });
    }

    // Tripwire 3 (#936) — `base` is declared in all six manifests with differing
    // bounds. Before the fix the emitted component kept whichever manifest
    // sorted first and discarded the rest, so a consumer read one subproject's
    // constraint as the whole repository's. A regression collapses this back to
    // a single citation.
    let base_multi_manifest = sboms
        .cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter().any(|c| {
                c.get("name").and_then(|n| n.as_str()) == Some("base")
                    && c.get("properties")
                        .and_then(|p| p.as_array())
                        .map(|props| {
                            props.iter().any(|p| {
                                p.get("name").and_then(|n| n.as_str())
                                    == Some("waybill:source-files")
                                    && p.get("value")
                                        .and_then(|v| v.as_str())
                                        .and_then(|v| {
                                            serde_json::from_str::<Vec<String>>(v).ok()
                                        })
                                        .map(|files| files.len() > 1)
                                        .unwrap_or(false)
                            })
                        })
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if !base_multi_manifest {
        return Err(AssertionFailure {
            invariant_name: "cross-manifest-declarations-unioned",
            format: FailureFormat::Cdx,
            observed: "`base` cites at most one source manifest".to_string(),
            expected: "`base` cites all six *.cabal manifests that declare it".to_string(),
            suggested_action: "a #936 regression: design-tier components keyed on PURL alone discard every declaration after the first, losing both the constraint and the manifest path. They must key on (PURL, manifest) so the union passes can see them",
        });
    }

    // Tripwire 4 (#938) — no lockfile exists here, so every dependency must
    // reach design tier. A suppression regression empties the document while
    // leaving the main modules, which is why component-count alone is a poor
    // check and tier is the right one.
    if !cdx_has_component_property(&sboms.cdx, "waybill:sbom-tier", |v| v == "design") {
        return Err(AssertionFailure {
            invariant_name: "design-tier-emission-not-suppressed",
            format: FailureFormat::Cdx,
            observed: "no design-tier components".to_string(),
            expected: "61 design-tier components — the repository has no cabal.project.freeze and no stack.yaml.lock".to_string(),
            suggested_action: "a #938 regression: design-tier emission suppressed without a lockfile supplying pins to replace what was silenced",
        });
    }

    // Tripwire 5 — completeness stays honestly `partial`. Flipping to
    // `complete` here would mean transitive edges were claimed for a repository
    // that pins nothing, which is invention rather than improvement.
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "partial" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "partial (reason `transitive-edges-unresolvable: hackage` — no freeze file, no stack lockfile)".to_string(),
            suggested_action: "if this flipped to `complete`, check whether transitive edges were fabricated rather than resolved; if to `unknown`/`missing`, the haskell reader likely stopped emitting entirely",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// haskell-language-server (#969)
// -----------------------------------------------------------------------

/// Document-scope property value, or `None`.
fn cdx_doc_property(cdx: &serde_json::Value, name: &str) -> Option<String> {
    cdx.get("metadata")?
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Every `pkg:hackage/*` component, with its properties flattened.
fn hackage_components(
    cdx: &serde_json::Value,
) -> Vec<(&serde_json::Value, std::collections::BTreeMap<String, String>)> {
    cdx.get("components")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|p| p.as_str())
                        .is_some_and(|p| p.starts_with("pkg:hackage/"))
                })
                .map(|c| {
                    let props = c
                        .get("properties")
                        .and_then(|p| p.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|p| {
                                    Some((
                                        p.get("name")?.as_str()?.to_string(),
                                        p.get("value")?.as_str()?.to_string(),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (c, props)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The first corpus target that exercises nixpkgs-backed Haskell version
/// resolution (#947 / milestone 926).
///
/// `haskell-aeson` carries no `flake.lock`, so that entire code path had no
/// corpus coverage at all: all four of its defects were caught by hand or by
/// CI, none by a test. The decisive one — the gate requiring the *author's*
/// reference to pin an exact revision, rather than the *lock* — resolved 0 of
/// 19 and 0 of 44 on two real repositories, and every fixture written for the
/// milestone encoded the same assumption as the code, so the whole suite
/// agreed with the bug.
///
/// Measured at the pinned revision under the harness invocation
/// (`--offline`, corpus-owned nixpkgs cache, `--root-name
/// haskell-language-server --root-version 1b4b3c6b`):
///
///   - 204 components, 165 of them `pkg:hackage/*`
///   - 97 resolved through nixpkgs, **all 97** with a version AND a native
///     SHA-256; 0 with provenance but no version
///   - 25 versionless, **all 25** carrying a reason
///     (`compiler-supplied` ×24, `absent-from-package-set` ×1)
///   - document-scope C174 naming revision `cbb5cf35…`, `resolved: 97`,
///     `disagreements: 9`
///
/// The floors below sit well above the pre-fix baseline (43 versioned, 0
/// resolved) so a gate regression cannot squeak past, while leaving room for
/// nixpkgs content to drift — layer 2's golden pins the exact numbers.
pub fn haskell_language_server_layer1(
    sboms: &EmittedSboms,
) -> Result<(), AssertionFailure> {
    // Tripwire 1 — the pass ran and named the revision it resolved through.
    //
    // This is the assertion the #947 gate bug would have tripped: a broken
    // gate degrades before any retrieval, so C174 carries `revision: null`
    // and nothing resolves. It is also the assertion that could not be
    // written before #973, because a successful pass recorded nothing at
    // document scope at all.
    let c174 = cdx_doc_property(&sboms.cdx, "waybill:nixpkgs-haskell-resolution");
    let revision_named = c174
        .as_deref()
        .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
        .and_then(|v| v.get("revision").and_then(|r| r.as_str()).map(str::to_string))
        .filter(|r| !r.is_empty());
    if revision_named.as_deref() != Some("cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73") {
        return Err(AssertionFailure {
            invariant_name: "nixpkgs-revision-resolved",
            format: FailureFormat::Cdx,
            observed: format!("waybill:nixpkgs-haskell-resolution = {c174:?}"),
            expected: "revision cbb5cf358f50aa6acc9efd6113b7bcfbc352cd73, as pinned by the target's own flake.lock".to_string(),
            suggested_action: "the nixpkgs-Haskell pass did not resolve. Check the flake.lock gate (#947 FR-012 — the LOCK pins the revision, not the author's `original` reference) and the offline cache path (#975 — `--offline` must read the corpus-owned cache, not refuse it). A null revision with a `degraded` reason names which",
        });
    }

    let hackage = hackage_components(&sboms.cdx);
    let resolved: Vec<_> = hackage
        .iter()
        .filter(|(_, p)| p.contains_key("waybill:nixpkgs-resolved-via"))
        .collect();

    // Tripwire 2 — resolution actually assigned versions at scale.
    //
    // Pre-fix this repository resolved 0. A floor of 80 (measured: 97)
    // cannot be met by accident and cannot be met at all by a degraded pass.
    if resolved.len() < 80 {
        return Err(AssertionFailure {
            invariant_name: "nixpkgs-resolution-scale",
            format: FailureFormat::Cdx,
            observed: format!("{} components resolved through nixpkgs", resolved.len()),
            expected: "at least 80 (measured 97 at the pinned revision)".to_string(),
            suggested_action: "resolution ran but assigned far fewer versions than the pinned nixpkgs carries. Suspect the package-set parser (#970 — keying must be on the ATTRIBUTE, not `pname`) or the boot-library rule over-claiming",
        });
    }

    // Tripwire 3 (Principle IX) — never assert a version without provenance,
    // and never claim provenance without the artifacts that justify it.
    //
    // The decoded nixpkgs source hash is the SHA-256 of the package's Hackage
    // tarball, which is why it is emitted in the NATIVE checksum field. A
    // component carrying resolution provenance but no version, or a version
    // but no hash, means the two halves have come apart.
    let missing_version = resolved.iter().filter(|(c, _)| c.get("version").is_none()).count();
    let missing_hash = resolved
        .iter()
        .filter(|(c, _)| {
            !c.get("hashes")
                .and_then(|h| h.as_array())
                .is_some_and(|arr| {
                    arr.iter().any(|h| h.get("alg").and_then(|a| a.as_str()) == Some("SHA-256"))
                })
        })
        .count();
    if missing_version > 0 || missing_hash > 0 {
        return Err(AssertionFailure {
            invariant_name: "nixpkgs-resolution-is-complete-per-component",
            format: FailureFormat::Cdx,
            observed: format!(
                "{missing_version} components claim nixpkgs provenance with no version; \
                 {missing_hash} with no native SHA-256"
            ),
            expected: "0 of each — provenance, version and source hash are assigned together".to_string(),
            suggested_action: "a component carrying `waybill:nixpkgs-resolved-via` asserts the pinned revision supplied its version. Emitting that without the version, or without the tarball digest that backs it, is an unbacked claim (Principle IX)",
        });
    }

    // Tripwire 4 (Principle X) — nothing is left silently versionless.
    //
    // A versionless component with no reason is indistinguishable from one
    // the reader never considered. This is the invariant that made the two
    // wrong boot rules visible during #947.
    let unreasoned: Vec<&str> = hackage
        .iter()
        .filter(|(c, p)| {
            c.get("version").is_none()
                && !p.contains_key("waybill:haskell-version-unresolved-reason")
        })
        .filter_map(|(c, _)| c.get("name").and_then(|n| n.as_str()))
        .collect();
    if !unreasoned.is_empty() {
        return Err(AssertionFailure {
            invariant_name: "every-versionless-haskell-component-has-a-reason",
            format: FailureFormat::Cdx,
            observed: format!(
                "{} versionless component(s) with no reason, e.g. {:?}",
                unreasoned.len(),
                &unreasoned[..unreasoned.len().min(5)]
            ),
            expected: "0 — every versionless Hackage component names why".to_string(),
            suggested_action: "a versionless component with no `waybill:haskell-version-unresolved-reason` cannot be told apart from one the reader never looked at (C151 / Principle X)",
        });
    }

    // Tripwire 5 — boot libraries stay versionless.
    //
    // `base`, `bytestring`, `containers` and friends ship WITH the compiler;
    // nixpkgs has entries for some of them whose versions a GHC-built project
    // never uses. Two rules during #947 under-included boot libraries, each
    // time letting a package-set version through — the dangerous direction,
    // because the result is a confident wrong version carrying a real hash.
    // A collapse of the boot rule shows up here as this count falling.
    let boot = hackage
        .iter()
        .filter(|(_, p)| {
            p.get("waybill:haskell-version-unresolved-reason").map(String::as_str)
                == Some("compiler-supplied")
        })
        .count();
    if boot < 15 {
        return Err(AssertionFailure {
            invariant_name: "boot-libraries-remain-compiler-supplied",
            format: FailureFormat::Cdx,
            observed: format!("{boot} components classified compiler-supplied"),
            expected: "at least 15 (measured 24 at the pinned revision)".to_string(),
            suggested_action: "boot libraries are taking versions from the package set. A GHC-built project uses the compiler's copy, so the nixpkgs entry is a version the build never sees — asserted with that other tarball's hash. Check `boot_libraries::union_nulled` and the candidate-series union (#947 FR-014a), and the offline hydration of every `configuration-ghc-*.nix` (#975)",
        });
    }

    Ok(())
}

// -----------------------------------------------------------------------
// Layer 0 — document integrity, applied to EVERY target (#980)
// -----------------------------------------------------------------------

/// Invariant I2: every edge endpoint must resolve to a component present in
/// the document.
///
/// I2 is not new. It is named in `generate::graph_completeness` (m860,
/// FR-001, C-3.3) and asserted there against a hand-built three-element
/// fixture. It had never been checked against a real emitted document — and
/// #980 is what that cost: the nixpkgs Haskell pass assigned versions, which
/// rewrote component PURLs, and the PURL is the identity the dependency
/// graph keys on. Every component the feature resolved was disconnected.
/// On one real project that dropped 66% of the SPDX relationships and left
/// 35 of 47 CycloneDX edges pointing at nothing.
///
/// Neither format complains. CycloneDX has no referential-integrity rule for
/// `bom-ref`, and SPDX simply omits the relationship, so the document stays
/// schema-valid while being wrong. Schema validation cannot see this class at
/// all; only an explicit invariant can.
///
/// This runs for every target, before the per-target tripwires, because the
/// defect it catches is not ecosystem-specific — any pass that rewrites a
/// component identity after edges are built reintroduces it.
pub fn layer0_document_integrity(
    target: &str,
    sboms: &EmittedSboms,
) -> Result<(), AssertionFailure> {
    // ---- CycloneDX: dangling `dependsOn` targets ----
    let mut refs: std::collections::HashSet<&str> = sboms
        .cdx
        .get("components")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().filter_map(|c| c.get("bom-ref")?.as_str()).collect())
        .unwrap_or_default();
    if let Some(root) = sboms
        .cdx
        .get("metadata")
        .and_then(|m| m.get("component"))
        .and_then(|c| c.get("bom-ref"))
        .and_then(|r| r.as_str())
    {
        refs.insert(root);
    }

    let mut dangling: Vec<String> = Vec::new();
    if let Some(deps) = sboms.cdx.get("dependencies").and_then(|d| d.as_array()) {
        for e in deps {
            let from = e.get("ref").and_then(|r| r.as_str()).unwrap_or("?");
            for t in e.get("dependsOn").and_then(|d| d.as_array()).into_iter().flatten() {
                if let Some(t) = t.as_str() {
                    if !refs.contains(t) {
                        dangling.push(format!("{from} -> {t}"));
                    }
                }
            }
        }
    }
    if !dangling.is_empty() {
        let shown: Vec<&String> = dangling.iter().take(5).collect();
        return Err(AssertionFailure {
            invariant_name: "i2-no-dangling-edge-endpoint",
            format: FailureFormat::Cdx,
            observed: format!(
                "{} dependsOn target(s) match no component bom-ref, e.g. {:?}",
                dangling.len(),
                shown
            ),
            expected: "0 — every edge endpoint resolves to a component in the document".to_string(),
            suggested_action:
                "invariant I2 (m860 FR-001 / C-3.3). Something rewrote a component's identity after the \
                 dependency edges were built, so the edges point at the old bom-ref. #980 was exactly \
                 this: the nixpkgs Haskell pass assigned a version, which changed the PURL, and PURLs \
                 are what `Relationship::from`/`::to` hold. Whatever changed an identity must rewrite \
                 the endpoints in the same step",
        });
    }

    // ---- SPDX 2.3: relationship endpoints must exist ----
    let known: std::collections::HashSet<&str> = sboms
        .spdx_2_3
        .get("packages")
        .and_then(|p| p.as_array())
        .map(|arr| arr.iter().filter_map(|p| p.get("SPDXID")?.as_str()).collect())
        .unwrap_or_default();
    let doc_id = sboms.spdx_2_3.get("SPDXID").and_then(|i| i.as_str());
    let mut unknown = 0usize;
    if let Some(rels) = sboms.spdx_2_3.get("relationships").and_then(|r| r.as_array()) {
        for r in rels {
            for key in ["spdxElementId", "relatedSpdxElement"] {
                if let Some(v) = r.get(key).and_then(|v| v.as_str()) {
                    // NONE / NOASSERTION are legitimate SPDX sentinels.
                    if v == "NONE" || v == "NOASSERTION" || Some(v) == doc_id {
                        continue;
                    }
                    if !known.contains(v) && !v.starts_with("SPDXRef-File") {
                        unknown += 1;
                    }
                }
            }
        }
    }
    if unknown > 0 {
        return Err(AssertionFailure {
            invariant_name: "i2-no-unknown-spdx-relationship-endpoint",
            format: FailureFormat::Spdx23,
            observed: format!("{unknown} relationship endpoint(s) name no package in the document"),
            expected: "0 — every relationship endpoint is a declared SPDXID".to_string(),
            suggested_action:
                "invariant I2 in SPDX 2.3. Note the failure mode differs from CycloneDX: SPDX usually \
                 DROPS such a relationship rather than dangling it, so a clean result here does not by \
                 itself prove the edges survived — compare root out-edge counts across formats too",
        });
    }

    let _ = target;
    Ok(())
}
