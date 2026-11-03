//! Milestone 1273 US1 (#1278) — an hpack `package.yaml` is read, and a
//! manifest that cannot be read says so.
//!
//! These run the real binary against fixture trees, which is a different
//! guarantee from the unit tests in
//! `scan_fs::package_db::haskell::hpack`: these also cover discovery, the
//! lowering call site, annotation registration and the emission pipeline —
//! any of which can be broken while the module's own tests stay green.
//!
//! **No test here touches the network or the host's nixpkgs cache.** US1 needs
//! neither: nothing in it resolves a version. `--offline` is passed anyway, so
//! a future change that reached for the network fails here rather than on a
//! machine without connectivity.
//!
//! Every package name is synthetic (`waybill-fixture-*`); see the fixture
//! tree's README for why.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/haskell_hpack")
        .join(name)
}

struct Scan {
    doc: Value,
    stderr: String,
}

fn scan(root: &Path) -> Scan {
    let out_dir = tempfile::tempdir().unwrap();
    let out_path = out_dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(root)
        .arg("--no-deep-hash")
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out_path.display()))
        // A scan must not consult the developer's nixpkgs cache. Nothing in
        // US1 resolves a version, so this is belt-and-braces: a change that
        // started resolving would otherwise silently depend on host state.
        .env("WAYBILL_NIXPKGS_CACHE", root.join("__no_cache__"))
        .env("RUST_LOG", "info")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: stderr={}",
        String::from_utf8_lossy(&result.stderr)
    );
    let bytes = std::fs::read(&out_path).unwrap();
    Scan {
        doc: serde_json::from_slice(&bytes).unwrap(),
        stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
    }
}

/// Hackage component names, excluding the project's own main module.
fn hackage_names(doc: &Value) -> Vec<String> {
    let main = doc
        .pointer("/metadata/component/name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut out: Vec<String> = doc
        .get("components")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|c| {
                    c.get("purl")
                        .and_then(|v| v.as_str())
                        .map(|p| p.starts_with("pkg:hackage/"))
                        .unwrap_or(false)
                })
                .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
                .filter(|n| *n != main)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

fn component<'a>(doc: &'a Value, name: &str) -> Option<&'a Value> {
    doc.get("components")
        .and_then(|v| v.as_array())?
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(name))
}

fn prop(c: &Value, key: &str) -> Option<String> {
    c.get("properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn doc_prop(doc: &Value, key: &str) -> Option<String> {
    doc.pointer("/metadata/properties")?
        .as_array()?
        .iter()
        .find(|p| p.get("name").and_then(|v| v.as_str()) == Some(key))
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Every document-scope property value concatenated. Several of these tests
/// assert that *some* document-scope signal names a file or a construct
/// without pinning which annotation carries it — the requirement is that the
/// document says it (FR-018), not that it says it in one particular slot.
fn doc_text(doc: &Value) -> String {
    doc.pointer("/metadata/properties")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| {
                    format!(
                        "{}={}",
                        p.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                        p.get("value").and_then(|v| v.as_str()).unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// The C200 `waybill:hpack-read-outcome` value, parsed.
///
/// Assertions go through this rather than through [`doc_text`] substring
/// matching. Mutation M10 — dropping the unmodelled-construct announcement
/// entirely — **SURVIVED** a substring assertion, because the fixture
/// directories are *named* `defaults` and `include`, so
/// `text.contains("include")` was matching the manifest's own path inside
/// `manifests-read`. The test asserted the fixture's name, not the
/// behaviour. A passing test may exercise nothing.
fn hpack_outcome(doc: &Value) -> Value {
    let raw = doc_prop(doc, "waybill:hpack-read-outcome")
        .expect("C200 is emitted whenever a package.yaml was seen");
    serde_json::from_str(&raw).expect("C200 value is JSON")
}

/// The `unmodelled-construct` values C200 reports, as a sorted list.
fn unmodelled_constructs(doc: &Value) -> Vec<String> {
    let o = hpack_outcome(doc);
    let mut v: Vec<String> = o
        .get("manifests-partial")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("unmodelled-construct"))
                .filter_map(|c| c.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// The reason codes C200 reports for manifests it could not read.
fn unread_reasons(doc: &Value) -> Vec<String> {
    let o = hpack_outcome(doc);
    o.get("manifests-unread")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("reason"))
                .filter_map(|c| c.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

// ───────────────────────────────────────────────────────────────────────
// T007 / C2 / SC-001 — the primary defect
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_hpack_only_emits_declared_dependencies() {
    let s = scan(&fixture("manifest_only"));
    let got = hackage_names(&s.doc);
    assert_eq!(
        got,
        vec![
            "waybill-fixture-aeson",
            "waybill-fixture-base",
            "waybill-fixture-text",
        ],
        "a package.yaml declaring 3 dependencies must emit 3 components; \
         measured at 0.14.0: 0"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T008 / C3 — inheritance reaches every stanza kind, with its own scope
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_toplevel_dependencies_reach_every_stanza_kind() {
    let s = scan(&fixture("inheritance"));
    let got = hackage_names(&s.doc);
    for name in [
        "waybill-fixture-shared",
        "waybill-fixture-libonly",
        "waybill-fixture-exeonly",
        "waybill-fixture-testonly",
        "waybill-fixture-benchonly",
    ] {
        assert!(
            got.iter().any(|n| n == name),
            "{name} missing; got {got:?}"
        );
    }

    let scope = |n: &str| {
        component(&s.doc, n)
            .and_then(|c| prop(c, "waybill:lifecycle-scope"))
            .unwrap_or_else(|| "<none>".into())
    };

    // Probe A1: the top-level list renders into all four stanza kinds, so a
    // name inherited everywhere is reachable from the library and is runtime.
    assert_ne!(
        scope("waybill-fixture-shared"),
        "development",
        "a top-level dependency reaches the library too, so it is not dev-only"
    );
    // Reached only through `tests:` / `benchmarks:`, which
    // `stanza_lifecycle_scope` already maps to Development.
    assert_eq!(scope("waybill-fixture-testonly"), "development");
    assert_eq!(scope("waybill-fixture-benchonly"), "development");
    // Declared by BOTH the library and the test suite: `merge_scope`'s
    // most-binding-wins keeps Runtime.
    assert_ne!(
        scope("waybill-fixture-libonly"),
        "development",
        "declared by the library as well as the tests, so Runtime wins"
    );
}

/// The mutant `m1273_us1_toplevel_dependencies_reach_every_stanza_kind` did
/// not catch: inheriting the top-level list into the **library only**.
///
/// That mutation SURVIVED the test above, because its fixture has a library,
/// so every top-level name was still emitted through it and every scope
/// assertion still held. The rule was asserted but not isolated.
///
/// This fixture has no library and no executable. The only stanza is a test
/// suite and the only declaration of its dependency is the top-level list, so
/// a library-only reader emits nothing at all.
#[test]
fn m1273_us1_toplevel_dependencies_reach_a_stanza_with_no_library() {
    let s = scan(&fixture("inheritance_tests_only"));
    let got = hackage_names(&s.doc);
    assert!(
        got.iter().any(|n| n == "waybill-fixture-inherited-into-tests"),
        "the top-level list must reach a `tests:` stanza even when the \
         manifest declares no library; got {got:?}"
    );
    let c = component(&s.doc, "waybill-fixture-inherited-into-tests")
        .expect("component present");
    assert_eq!(
        prop(c, "waybill:lifecycle-scope").as_deref(),
        Some("development"),
        "and reached only through a test suite, so it is Development"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T009 / C4 — last-wins, and NO intersection
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_repeated_name_keeps_last_constraint_only() {
    let s = scan(&fixture("repeated_name"));
    let c = component(&s.doc, "waybill-fixture-text")
        .expect("the repeated name must still emit exactly one component");
    let ranges = prop(c, "waybill:requirement-ranges").unwrap_or_default();

    assert!(
        ranges.contains("< 3"),
        "the LAST declaration's constraint must be the one recorded; got {ranges:?}"
    );
    // The negative half, which is the whole point. Probe A3 measured that
    // hpack renders `text <3` for this manifest -- the per-component entry
    // REPLACES the inherited constraint rather than intersecting with it.
    // Carrying both would publish a range the project does not declare.
    assert!(
        !ranges.contains(">= 1.0"),
        "the inherited constraint must be REPLACED, not intersected; got {ranges:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T010 / C5 — the range is the manifest's, not hpack's rendering
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_range_recorded_as_declared_not_as_hpack_renders() {
    let s = scan(&fixture("declared_range"));
    let c = component(&s.doc, "waybill-fixture-base").expect("component present");
    let ranges = prop(c, "waybill:requirement-ranges").unwrap_or_default();
    assert!(
        ranges.contains(">= 4") && ranges.contains("< 5"),
        "the manifest's own range must be recorded; got {ranges:?}"
    );
    // Probe A4: hpack would render this as `base ==4.*`. That normalisation is
    // cosmetic and belongs to an artifact this reader no longer reads.
    assert!(
        !ranges.contains("==4.*"),
        "hpack's constraint normalisation must NOT be reproduced; got {ranges:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T011 / C6 — the agreement contract: two dialects, one answer
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_when_block_agrees_with_cabal_conditional() {
    let from_manifest = scan(&fixture("when_flag_off"));
    let from_cabal = scan(&fixture("cabal_flag_off"));

    let read = |s: &Scan| -> (String, String) {
        let c = component(&s.doc, "waybill-fixture-optdep")
            .expect("the flag-gated dependency must be emitted in both dialects");
        (
            prop(c, "waybill:lifecycle-scope").unwrap_or_else(|| "<none>".into()),
            prop(c, "waybill:optional-derivation").unwrap_or_else(|| "<none>".into()),
        )
    };

    // Both answers are collected before either is asserted, so a disagreement
    // reports what BOTH dialects said rather than only the first to fail.
    let (m_scope, m_deriv) = read(&from_manifest);
    let (c_scope, c_deriv) = read(&from_cabal);

    assert_eq!(
        (m_scope.as_str(), m_deriv.as_str()),
        (c_scope.as_str(), c_deriv.as_str()),
        "a default-off `when: flag(cli)` block in package.yaml and a \
         `buildable: False` stanza in a *.cabal describe the same thing. \
         package.yaml said ({m_scope}, {m_deriv}); *.cabal said \
         ({c_scope}, {c_deriv}). Probe A5 measured that hpack lowers `when:` \
         to exactly that Cabal conditional, so two answers here means a \
         second evaluator was written."
    );
    assert_eq!(
        m_deriv, "cabal-flag-disabled-stanza",
        "and both must carry the existing derivation value, not a new one"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T012 / C6 — a literal `false` is pruned, not made optional
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_literal_false_when_contributes_nothing() {
    let s = scan(&fixture("when_literal_false"));
    let got = hackage_names(&s.doc);
    assert!(
        got.iter().any(|n| n == "waybill-fixture-base"),
        "the unconditional dependency must still be emitted; got {got:?}"
    );
    // Probe A5: hpack PRUNES a literal-false block. The dependency is absent
    // from the rendered .cabal entirely, so it is absent here -- not optional.
    assert!(
        !got.iter().any(|n| n == "waybill-fixture-pruned"),
        "a `condition: false` block is pruned by hpack, so its dependency must \
         not appear at all -- not even as optional; got {got:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T013 / C7 — the manifest wins; the generated *.cabal is suppressed
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_manifest_wins_over_generated_cabal() {
    let s = scan(&fixture("manifest_and_generated_cabal"));
    let got = hackage_names(&s.doc);

    assert!(
        got.iter().any(|n| n == "waybill-fixture-from-manifest"),
        "the manifest is the source of truth, so its dependency is emitted; \
         got {got:?}"
    );
    assert!(
        !got.iter().any(|n| n == "waybill-fixture-stale-generated"),
        "the hpack-GENERATED *.cabal must be suppressed, or the project has \
         two sources of truth -- the failure milestone 143 was right to fear; \
         got {got:?}"
    );
    // No component emitted twice.
    let mut sorted = got.clone();
    sorted.dedup();
    assert_eq!(sorted.len(), got.len(), "a component was emitted twice");
    // And the document records WHICH file was read, so no operator has to
    // guess which of the two the document describes.
    let text = doc_text(&s.doc);
    assert!(
        text.contains("package.yaml"),
        "the document must record that the manifest was the source read; \
         document-scope properties were:\n{text}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T014 / C7 — a hand-written *.cabal is NOT suppressed
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_handwritten_cabal_is_not_suppressed() {
    let s = scan(&fixture("manifest_and_handwritten_cabal"));
    let got = hackage_names(&s.doc);
    assert!(
        got.iter().any(|n| n == "waybill-fixture-from-manifest"),
        "the manifest is read; got {got:?}"
    );
    assert!(
        got.iter().any(|n| n == "waybill-fixture-from-handwritten"),
        "a *.cabal WITHOUT the hpack generation header is not hpack output, so \
         the manifest is not its source and it must still be read; got {got:?}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T015 / C8 / FR-003 / SC-009 — an unparseable manifest is announced
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_unparseable_manifest_is_announced() {
    let s = scan(&fixture("malformed"));
    // Asserted on the DOCUMENT, with the log available but not consulted for
    // the assertion (SC-009). A log-only diagnostic leaves the artifact
    // exactly as dishonest as the defect being fixed.
    //
    // Structured rather than substring, for the reason recorded on
    // `unmodelled_constructs`: a path can satisfy a substring by accident.
    let reasons = unread_reasons(&s.doc);
    assert_eq!(
        reasons,
        vec!["malformed-yaml".to_string()],
        "the document must say WHY the manifest could not be read; \
         C200 reported {reasons:?}"
    );
    let o = hpack_outcome(&s.doc);
    let unread = o["manifests-unread"].as_array().unwrap();
    assert!(
        unread
            .iter()
            .filter_map(|e| e.get("file").and_then(|f| f.as_str()))
            .any(|f| f.ends_with("package.yaml")),
        "and must name the file it could not read; got {unread:?}"
    );
    let text = doc_text(&s.doc);
    assert!(
        !text.contains("waybill:graph-completeness=complete"),
        "and must not then claim completeness over the result;\n{text}"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T016 / C8 / SC-002 — an unread manifest defeats the completeness claim
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_unread_manifest_defeats_completeness() {
    let s = scan(&fixture("malformed"));
    let gc = doc_prop(&s.doc, "waybill:graph-completeness")
        .expect("graph-completeness is always emitted (milestone 158 US2)");
    assert_ne!(
        gc, "complete",
        "measured at 0.14.0: `complete` over 0 components, with no diagnostic. \
         An absent reader result and a genuinely dependency-free project must \
         be distinguishable in the document."
    );
}

// ───────────────────────────────────────────────────────────────────────
// T017 / C9 / FR-001b / SC-010 — an unmodelled construct is announced
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_unmodelled_construct_is_announced() {
    for (dir, construct) in [("defaults", "defaults"), ("include", "include")] {
        let s = scan(&fixture(dir));
        // Structured, NOT a substring of the whole document. The fixture
        // directories are named after the constructs, so a substring
        // assertion passes on the manifest's own path even when nothing was
        // announced -- which is how mutation M10 survived the first version.
        let got = unmodelled_constructs(&s.doc);
        assert_eq!(
            got,
            vec![construct.to_string()],
            "a manifest using `{construct}:` yields a partial answer that SAYS \
             it is partial; C200 reported {got:?}"
        );
        // And the file is named, in the entry that carries the construct.
        let o = hpack_outcome(&s.doc);
        let files: Vec<&str> = o["manifests-partial"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| e.get("file").and_then(|f| f.as_str()))
            .collect();
        assert!(
            files.iter().any(|f| f.ends_with("package.yaml")),
            "and must name the file; got {files:?}"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// T018 / C10 / FR-001c / SC-010 — no network, for any manifest
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_defaults_performs_no_network_request() {
    // Both halves are needed. `--offline` alone would be satisfied by a pass
    // that silently skipped itself; the document assertion alone would be
    // satisfied by a pass that fetched successfully.
    let s = scan(&fixture("defaults"));
    assert_eq!(
        unmodelled_constructs(&s.doc),
        vec!["defaults".to_string()],
        "the construct is announced rather than resolved"
    );
    // Probe A7: hpack itself refuses the whole manifest when it cannot fetch
    // `defaults:`. waybill must not try -- so the modelled part of the
    // manifest is still read, and the gap is reported.
    assert!(
        !s.stderr.contains("raw.githubusercontent.com"),
        "nothing may attempt to retrieve a `defaults:` document; stderr:\n{}",
        s.stderr
    );
    assert!(
        hackage_names(&s.doc)
            .iter()
            .any(|n| n == "waybill-fixture-base"),
        "and the modelled part of the manifest is still read"
    );
}

// ───────────────────────────────────────────────────────────────────────
// T030 / C22 / FR-004 — the disagreement warning still fires
// ───────────────────────────────────────────────────────────────────────

#[test]
fn m1273_us1_disagreement_warning_still_fires() {
    let s = scan(&fixture("manifest_and_generated_cabal"));
    // Measured: `emit_hpack_warnings` iterates `cabal_manifests`, so this
    // warning fires ONLY where a generated sibling exists -- which is why
    // FR-001's gap and this behaviour are separate code paths, and fixing one
    // does not fix the other. FR-004 keeps it; FR-004a changes its advice.
    assert!(
        s.stderr.contains("package.yaml"),
        "the disagreement must stay visible to the operator; stderr:\n{}",
        s.stderr
    );
    // FR-004a / C21: the old advice cannot be true any more. waybill reads the
    // manifest, so regenerating the *.cabal changes nothing about the
    // document, and telling an operator to do it looks like a remedy.
    assert!(
        !s.stderr.contains("regenerate before scanning"),
        "FR-004a: the advice to regenerate is no longer true under FR-002; \
         stderr:\n{}",
        s.stderr
    );
}
