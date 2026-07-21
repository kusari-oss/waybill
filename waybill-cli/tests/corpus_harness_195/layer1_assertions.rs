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
