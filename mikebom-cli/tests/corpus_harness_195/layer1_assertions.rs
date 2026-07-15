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

/// Extract the mikebom:graph-completeness value from CDX
/// `.metadata.properties[]`.
fn cdx_graph_completeness(cdx: &serde_json::Value) -> Option<String> {
    cdx.get("metadata")?
        .get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("mikebom:graph-completeness"))
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
    // The corpus harness invokes mikebom with `--root-name go-cobra
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
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete".to_string(),
            suggested_action: "investigate cargo reader regression — ripgrep cargo workspace should classify complete",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:cargo/ripgrep")) {
        return Err(AssertionFailure {
            invariant_name: "main-module-purl-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:cargo/ripgrep component".to_string(),
            expected: "at least one pkg:cargo/ripgrep@vX.Y.Z component".to_string(),
            suggested_action: "investigate m064 cargo main-module emission or m087 workspace-version resolution",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// npm-express (US2)
// -----------------------------------------------------------------------

pub fn npm_express_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete".to_string(),
            suggested_action: "investigate npm reader (m066 / m147 / m180) — express should classify complete",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:npm/express")) {
        return Err(AssertionFailure {
            invariant_name: "main-module-purl-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:npm/express component".to_string(),
            expected: "at least one pkg:npm/express@vX.Y.Z component".to_string(),
            suggested_action: "investigate m066 npm main-module emission — express package.json has a name, should emit",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// python-flask (US2)
// -----------------------------------------------------------------------

pub fn python_flask_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete".to_string(),
            suggested_action: "investigate pip reader (m068 / m183) — flask should classify complete",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:pypi/flask")) {
        return Err(AssertionFailure {
            invariant_name: "main-module-purl-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:pypi/flask component".to_string(),
            expected: "at least one pkg:pypi/flask@vX.Y.Z component".to_string(),
            suggested_action: "investigate m068 pip main-module emission",
        });
    }
    Ok(())
}

// -----------------------------------------------------------------------
// maven-guice (US2)
// -----------------------------------------------------------------------

pub fn maven_guice_layer1(sboms: &EmittedSboms) -> Result<(), AssertionFailure> {
    let gc = cdx_graph_completeness(&sboms.cdx).unwrap_or_else(|| "<missing>".to_string());
    if gc != "complete" {
        return Err(AssertionFailure {
            invariant_name: "graph-completeness",
            format: FailureFormat::Cdx,
            observed: gc,
            expected: "complete".to_string(),
            suggested_action: "investigate maven reader (m070 / m085 / m184) — guice should classify complete",
        });
    }
    if !cdx_has_component_purl(&sboms.cdx, |p| p.starts_with("pkg:maven/com.google.inject/guice")) {
        return Err(AssertionFailure {
            invariant_name: "main-module-purl-present",
            format: FailureFormat::Cdx,
            observed: "no pkg:maven/com.google.inject/guice component".to_string(),
            expected: "at least one pkg:maven/com.google.inject/guice component".to_string(),
            suggested_action: "investigate m070 maven main-module emission for multi-module projects",
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
