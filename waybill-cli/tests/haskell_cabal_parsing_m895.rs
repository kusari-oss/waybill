//! Milestone 895 (#891) — `.cabal` dependency parsing.
//!
//! The Haskell reader fabricated components and dropped declared ones. Four
//! defects came from #891; a fifth was found while planning — only the first
//! dependency list in a section was read, and depending on whether a blank
//! line separated the lists, the later ones were either dropped outright or
//! glued into the preceding entry's version.
//!
//! Measured on `aeson.cabal` before the fix: 10 of 48 declared dependencies
//! never emitted, and 13 of 38 emitted identifiers malformed.
//!
//! Every accuracy assertion here compares emitted component names against the
//! names in the scanned file (contract A-8). Asking the parser how many
//! dependencies it found is asking the change to grade itself.
//!
//! Fixtures are composed at test time; every synthetic package name carries
//! the `waybill-fixture-*` prefix per the fixture policy.
//!
//! Cross-linked: `specs/895-fix-cabal-parser/contracts/cabal-parsing.md`.

#![cfg(test)]
#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_waybill")
}

/// Write a single `.cabal` file into a fresh directory and scan it.
fn scan_cabal(body: &str) -> (Value, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fixture.cabal"), body).unwrap();
    let out = dir.path().join("out.cdx.json");
    let result = Command::new(binary_path())
        .arg("--offline")
        .arg("sbom")
        .arg("scan")
        .arg("--path")
        .arg(dir.path())
        .arg("--format")
        .arg("cyclonedx-json")
        .arg("--output")
        .arg(format!("cyclonedx-json={}", out.display()))
        .arg("--no-deep-hash")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "scan failed: {}",
        String::from_utf8_lossy(&result.stderr),
    );
    let doc: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    (doc, dir)
}

/// Emitted `pkg:hackage/*` component names, with any version segment removed.
fn emitted_names(doc: &Value) -> BTreeSet<String> {
    doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["purl"].as_str())
        .filter_map(|p| p.strip_prefix("pkg:hackage/"))
        .map(|p| p.split('@').next().unwrap_or(p).to_lowercase())
        .collect()
}

/// Full emitted identifiers, for assertions about identifier *shape*.
fn emitted_purls(doc: &Value) -> Vec<String> {
    let mut v: Vec<String> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| c["purl"].as_str())
        .filter(|p| p.starts_with("pkg:hackage/"))
        .map(String::from)
        .collect();
    v.sort();
    v
}

/// Every dependency name the `.cabal` body declares, read independently of
/// the parser under test.
///
/// This is contract A-8's measurement. It applies the cabal layout rule
/// directly — a field block ends at the first subsequent non-blank line
/// indented at most as far as the field line — so it does not inherit the
/// bug it exists to detect.
fn declared_names(body: &str) -> BTreeSet<String> {
    fn indent(l: &str) -> usize {
        l.len() - l.trim_start_matches([' ', '\t']).len()
    }
    let lines: Vec<&str> = body.lines().collect();
    let mut out = BTreeSet::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("build-depends:") || trimmed.starts_with("build-tool-depends:")) {
            continue;
        }
        if indent(line) == 0 {
            continue; // a stanza-level field, not inside a section
        }
        let field_indent = indent(line);
        let mut body_text = String::from(line.split_once(':').map(|(_, r)| r).unwrap_or(""));
        for next in lines.iter().skip(i + 1) {
            if next.trim().is_empty() {
                continue;
            }
            if indent(next) <= field_indent {
                break;
            }
            body_text.push(' ');
            body_text.push_str(next);
        }
        for part in body_text.split(',') {
            let name: String = part
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            // A build-tool coord is `package:executable`; the package is the
            // declared name.
            if !name.is_empty() && !name.starts_with('-') {
                out.insert(name.to_lowercase());
            }
        }
    }
    out
}

/// Both directions. Accuracy is `emitted - declared`; completeness is
/// `declared - emitted`. A one-directional check would have missed the fifth
/// defect entirely, which is why this returns the pair.
fn accuracy_and_completeness(body: &str, doc: &Value) -> (Vec<String>, Vec<String>) {
    let declared = declared_names(body);
    let emitted = emitted_names(doc);
    (
        emitted.difference(&declared).cloned().collect(),
        declared.difference(&emitted).cloned().collect(),
    )
}

/// A component's properties as a name -> value map.
fn props(doc: &Value, purl_prefix: &str) -> std::collections::BTreeMap<String, String> {
    doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| {
            c["purl"]
                .as_str()
                .is_some_and(|p| p.starts_with(purl_prefix))
        })
        .and_then(|c| c["properties"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some((
                p["name"].as_str()?.to_string(),
                p["value"].as_str()?.to_string(),
            ))
        })
        .collect()
}


/// Assert no emitted identifier carries text that cannot be part of a package
/// name or version: a cabal field name, a comment marker, or the underscore
/// the sanitiser substitutes for whitespace when a block over-ran.
///
/// Name-based assertions cannot see this. `emitted_names` strips at `@`, so a
/// component whose version swallowed the next three lines still has the right
/// name — which is how three of this file's tests originally passed against
/// the defect they were written to catch.
fn assert_no_leaked_text(doc: &Value) {
    let offenders: Vec<String> = emitted_purls(doc)
        .into_iter()
        .filter(|p| {
            p.contains("build-depends")
                || p.contains("build-tool-depends")
                || p.contains("default-language")
                || p.contains("hs-source-dirs")
                || p.contains("main-is")
                || p.contains("--")
                || p.contains("if_")
                || p.contains("impl(")
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "identifiers carrying text from outside their own block: {offenders:#?}",
    );
}

/// US2's property, deliberately separate from `assert_no_leaked_text`.
///
/// A block that terminated correctly can still emit a constraint in the
/// version slot — that is a different defect with a different fix, and
/// folding the two into one assertion would make US1's tests depend on US2
/// being implemented.
#[allow(dead_code)] // used by the US2 phase
fn assert_identifiers_versionless(doc: &Value) {
    let offenders: Vec<String> = emitted_purls(doc)
        .into_iter()
        .filter(|p| p.contains('@'))
        .collect();
    assert!(
        offenders.is_empty(),
        "identifiers carrying a version segment where none was resolved: {offenders:#?}",
    );
}

/// The #891 reproducer: both cabal layouts in one file.
const REPRO: &str = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1
build-type:     Simple

library
  hs-source-dirs:
      src
  build-tool-depends:
      waybill-fixture-tool:waybill-fixture-tool
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
    , waybill-fixture-vec >=0.12 && <0.14
  default-language: Haskell2010

executable waybill-fixture-demo
  main-is:             Main.hs
  build-depends:       waybill-fixture-core >=4.14 && <4.15
                     , waybill-fixture-cmt
  -- hs-source-dirs:
  default-language:    Haskell2010
"#;

#[test]
fn t005_the_measurement_helper_reads_both_layouts() {
    // The helper is the basis of every accuracy assertion, so it is checked
    // against the reproducer before anything relies on it. If this is wrong,
    // every test built on it is wrong in the same direction.
    let declared = declared_names(REPRO);
    let expect: BTreeSet<String> = [
        "waybill-fixture-cmt",
        "waybill-fixture-core",
        "waybill-fixture-tool",
        "waybill-fixture-vec",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        declared, expect,
        "the independent reader must find exactly the four declared names \
         across both layouts",
    );
}

// -------------------------------------------------------------------
// User Story 1 — nothing is invented, nothing is dropped.
// -------------------------------------------------------------------

#[test]
fn t007_no_emitted_name_is_absent_from_the_file() {
    // FR-004, contract A-1. The accuracy invariant, measured against the
    // file rather than against an expected list someone typed.
    let (doc, _d) = scan_cabal(REPRO);
    let (fabricated, _missing) = accuracy_and_completeness(REPRO, &doc);
    assert!(
        fabricated.is_empty(),
        "emitted names absent from the .cabal file: {fabricated:#?}\nall emitted: {:#?}",
        emitted_purls(&doc),
    );
}

#[test]
fn t007b_no_declared_name_is_missing() {
    // The other direction (FR-001a, SC-002a). Separate test so a failure
    // says which of the two invariants broke.
    let (doc, _d) = scan_cabal(REPRO);
    let (_fabricated, missing) = accuracy_and_completeness(REPRO, &doc);
    assert!(
        missing.is_empty(),
        "declared names never emitted: {missing:#?}",
    );
}

#[test]
fn t008_block_terminates_at_the_next_field_hpack_layout() {
    // FR-001, contract A-2, research R1. The measured rule: a block ends at
    // the first line indented at most as far as the FIELD line. Note the
    // entries here sit at indent 6 then 4 — less indented than the first
    // entry, and still part of the block. An implementation keyed on the
    // first entry's indent fails this.
    let body = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
    , waybill-fixture-vec >=0.12 && <0.14
  default-language: Haskell2010
"#;
    let (doc, _d) = scan_cabal(body);
    let names = emitted_names(&doc);
    assert!(
        names.contains("waybill-fixture-vec"),
        "the last entry must survive; got {names:#?}",
    );
    assert!(
        !emitted_purls(&doc)
            .iter()
            .any(|p| p.contains("default-language") || p.contains("Haskell2010")),
        "no identifier may contain the terminating field: {:#?}",
        emitted_purls(&doc),
    );
}

#[test]
fn t009_block_terminates_in_the_cabal_init_layout_and_comma_style_is_irrelevant() {
    // FR-001, DD-5, R1 Layout B. First entry inline with the field name,
    // continuations aligned under the value. Also asserts leading-comma and
    // trailing-comma lists yield the same set.
    let leading = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

executable waybill-fixture-demo
  build-depends:       waybill-fixture-core >=4.14 && <4.15
                     , waybill-fixture-cmt
  -- hs-source-dirs:
  default-language:    Haskell2010
"#;
    let trailing = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

executable waybill-fixture-demo
  build-depends:       waybill-fixture-core >=4.14 && <4.15,
                       waybill-fixture-cmt
  -- hs-source-dirs:
  default-language:    Haskell2010
"#;
    let (doc_l, _a) = scan_cabal(leading);
    let (doc_t, _b) = scan_cabal(trailing);
    assert!(
        emitted_names(&doc_l).contains("waybill-fixture-cmt"),
        "leading-comma continuation lost; got {:#?}",
        emitted_names(&doc_l),
    );
    assert_eq!(
        emitted_names(&doc_l),
        emitted_names(&doc_t),
        "comma placement must not change the dependency set",
    );
    // Names alone do not prove the block terminated — a swallowed
    // `default-language:` lands in the version, which `emitted_names` hides.
    assert_no_leaked_text(&doc_l);
    assert_no_leaked_text(&doc_t);
}

#[test]
fn t010_build_tool_block_does_not_absorb_the_next_block() {
    // FR-003. The two fields are adjacent with no blank line between them —
    // the shape that produced defect 4.
    let body = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-tool-depends:
      waybill-fixture-tool:waybill-fixture-tool
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
  default-language: Haskell2010
"#;
    let (doc, _d) = scan_cabal(body);
    assert!(
        !emitted_purls(&doc)
            .iter()
            .any(|p| p.contains("build-depends")),
        "a field name leaked into an identifier: {:#?}",
        emitted_purls(&doc),
    );
    let names = emitted_names(&doc);
    assert!(
        names.contains("waybill-fixture-core"),
        "the following block must still be read; got {names:#?}",
    );
    // The build tool is declared, so it must exist. Before the fix its PURL
    // was malformed enough to fail validation and be dropped silently, which
    // made the leak assertion above pass vacuously.
    assert!(
        names.contains("waybill-fixture-tool"),
        "the declared build tool must be emitted, not dropped; got {names:#?}",
    );
    assert_no_leaked_text(&doc);
}

#[test]
fn t011_a_comment_inside_a_block_contributes_nothing() {
    // FR-002. Deliberately placed DEEPER than the field, because R1 measured
    // that a comment at field indentation is already removed by the
    // termination rule — a comment there would prove nothing about comment
    // handling.
    let body = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
      -- a note between entries
    , waybill-fixture-vec >=0.12 && <0.14
  default-language: Haskell2010
"#;
    let (doc, _d) = scan_cabal(body);
    let (fabricated, missing) = accuracy_and_completeness(body, &doc);
    assert!(
        fabricated.is_empty() && missing.is_empty(),
        "comment inside a block disturbed parsing; fabricated={fabricated:#?} missing={missing:#?}",
    );
    assert!(
        !emitted_purls(&doc).iter().any(|p| p.contains("note")),
        "comment text leaked into an identifier: {:#?}",
        emitted_purls(&doc),
    );
}

/// Repeated fields grouped under comment headings, plus one inside a
/// conditional. This is the fifth defect's shape and the dominant one by
/// volume on real files.
const MULTI: &str = r#"cabal-version: 1.12
name:           waybill-fixture-multi
version:        0.1

library
  -- Group one
  build-depends:
    , waybill-fixture-alpha >=1.0 && <2
  -- Group two
  build-depends:
    , waybill-fixture-beta ^>=2.1
  if !impl(ghc >=9.4)
    build-depends: waybill-fixture-cond >=0.1 && <0.2
  -- Group three
  build-depends:
    , waybill-fixture-gamma ==3.*
  default-language: Haskell2010
"#;

#[test]
fn t012_every_repeated_dependency_list_in_a_section_is_read() {
    // FR-001a, SC-002a. Measured on `aeson.cabal`: 10 of 48 declared
    // dependencies were never emitted because of this.
    let (doc, _d) = scan_cabal(MULTI);
    let names = emitted_names(&doc);
    for want in [
        "waybill-fixture-alpha",
        "waybill-fixture-beta",
        "waybill-fixture-gamma",
    ] {
        assert!(
            names.contains(want),
            "{want} is declared in its own build-depends field and must be emitted; got {names:#?}",
        );
    }
    // Before the fix all three names appeared, with the later fields glued
    // into the preceding entry's version — so a name-only assertion passed
    // against the defect.
    assert_no_leaked_text(&doc);
}

#[test]
fn t013_a_dependency_list_inside_a_conditional_is_read() {
    // FR-001b. On the corpus target, `generically`, `integer-gmp` and
    // `nothunks` are declared only inside conditionals.
    let (doc, _d) = scan_cabal(MULTI);
    assert!(
        emitted_names(&doc).contains("waybill-fixture-cond"),
        "a build-depends nested in an `if` block must be read; got {:#?}",
        emitted_names(&doc),
    );
}

#[test]
fn t013b_repeated_and_conditional_lists_satisfy_both_invariants() {
    // The pair, on the same fixture — nothing invented, nothing dropped.
    let (doc, _d) = scan_cabal(MULTI);
    let (fabricated, missing) = accuracy_and_completeness(MULTI, &doc);
    assert!(
        fabricated.is_empty() && missing.is_empty(),
        "fabricated={fabricated:#?}\nmissing={missing:#?}\nemitted={:#?}",
        emitted_purls(&doc),
    );
}

#[test]
fn t014_enumerated_edge_cases() {
    // SC-008: the remaining enumerated edge cases, batched.

    // A list that is the final field, with no trailing newline.
    let final_field = "cabal-version: 1.12\nname: waybill-fixture-app\nversion: 0.1\n\nlibrary\n  build-depends:\n      waybill-fixture-core >=1 && <2";
    let (doc, _a) = scan_cabal(final_field);
    assert!(
        emitted_names(&doc).contains("waybill-fixture-core"),
        "a list ending at EOF with no newline must still be read",
    );

    // An empty list — field present, nothing under it.
    let empty = r#"cabal-version: 1.12
name: waybill-fixture-app
version: 0.1

library
  build-depends:
  default-language: Haskell2010
"#;
    let (doc, _b) = scan_cabal(empty);
    let (fabricated, _) = accuracy_and_completeness(empty, &doc);
    assert!(
        fabricated.is_empty(),
        "an empty list must produce nothing, not a component named after the \
         following field: {fabricated:#?}",
    );

    // Windows line endings.
    let crlf = "cabal-version: 1.12\r\nname: waybill-fixture-app\r\nversion: 0.1\r\n\r\nlibrary\r\n  build-depends:\r\n      waybill-fixture-core >=1 && <2\r\n  default-language: Haskell2010\r\n";
    let (doc, _c) = scan_cabal(crlf);
    assert!(
        emitted_names(&doc).contains("waybill-fixture-core"),
        "CRLF line endings must parse; got {:#?}",
        emitted_names(&doc),
    );
    assert!(
        !emitted_purls(&doc).iter().any(|p| p.contains('\r')),
        "a carriage return leaked into an identifier: {:#?}",
        emitted_purls(&doc),
    );
}

// -------------------------------------------------------------------
// User Story 2 — a dependency is identified by what it is.
// -------------------------------------------------------------------

const CONSTRAINED: &str = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-depends:
      waybill-fixture-core >=4.11 && <4.22
    , waybill-fixture-bare
  default-language: Haskell2010

test-suite spec
  build-depends:
      waybill-fixture-core >=4.14 && <4.15
  default-language: Haskell2010
"#;

#[test]
fn t023_identifiers_carry_no_version_segment() {
    // FR-005, SC-003, EC-1, contract A-3. A constraint is not a version, and
    // a placeholder token standing in for one is the same error in milder
    // form.
    let (doc, _d) = scan_cabal(CONSTRAINED);
    assert_identifiers_versionless(&doc);
    assert!(
        emitted_purls(&doc).contains(&"pkg:hackage/waybill-fixture-core".to_string()),
        "got {:#?}",
        emitted_purls(&doc),
    );
}

#[test]
fn t024_the_constraint_is_recoverable_verbatim() {
    // FR-006, FR-009, SC-004, contract A-4. The identifier no longer carries
    // it, so this record is its only carrier.
    let p = props(&scan_cabal(CONSTRAINED).0, "pkg:hackage/waybill-fixture-core");
    let ranges = p
        .get("waybill:requirement-ranges")
        .expect("the constraint record must be present");
    assert!(
        ranges.contains(">=4.11 && <4.22"),
        "constraint must appear verbatim; got {ranges}",
    );
}

#[test]
fn t025_a_package_declared_twice_keeps_both_constraints() {
    // FR-007, EC-3. `core` is declared with different constraints in two
    // stanzas. Asserting the MERGE, not the count: once identifiers drop the
    // version the two declarations collide on one component by construction,
    // so a count-only assertion would pass even with one constraint dropped.
    let (doc, _d) = scan_cabal(CONSTRAINED);
    let cores: Vec<String> = emitted_purls(&doc)
        .into_iter()
        .filter(|p| p == "pkg:hackage/waybill-fixture-core")
        .collect();
    assert_eq!(cores.len(), 1, "expected one component; got {cores:#?}");
    let p = props(&doc, "pkg:hackage/waybill-fixture-core");
    let ranges = p.get("waybill:requirement-ranges").unwrap();
    for want in [">=4.11 && <4.22", ">=4.14 && <4.15"] {
        assert!(
            ranges.contains(want),
            "both declared constraints must survive; {want} missing from {ranges}",
        );
    }
}

#[test]
fn t026_constrained_and_unconstrained_identifiers_have_the_same_shape() {
    // FR-008. The two must not be treated as different packages.
    let (doc, _d) = scan_cabal(CONSTRAINED);
    let purls = emitted_purls(&doc);
    for want in [
        "pkg:hackage/waybill-fixture-core",
        "pkg:hackage/waybill-fixture-bare",
    ] {
        assert!(
            purls.contains(&want.to_string()),
            "{want} must be emitted in the same shape; got {purls:#?}",
        );
    }
}

// -------------------------------------------------------------------
// User Story 3 — a build tool is not a library.
// -------------------------------------------------------------------

const TOOLS: &str = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-tool-depends:
      waybill-fixture-tool:waybill-fixture-exe
  build-depends:
      waybill-fixture-lib >=1 && <2
  default-language: Haskell2010
"#;

#[test]
fn t033_a_build_tool_identifier_names_the_package_alone() {
    // FR-010a, DD-4, contract A-6. `package:executable` is not a package
    // name, and no registry resolves it.
    let (doc, _d) = scan_cabal(TOOLS);
    let purls = emitted_purls(&doc);
    assert!(
        purls.contains(&"pkg:hackage/waybill-fixture-tool".to_string()),
        "expected the package half alone; got {purls:#?}",
    );
    assert!(
        !purls.iter().any(|p| p.contains(':') && !p.starts_with("pkg:")),
        "no identifier may carry a package:executable pair: {purls:#?}",
    );
}

#[test]
fn t034_a_build_tool_is_build_time_and_a_library_dependency_is_not() {
    // FR-010, EC-4.
    let (doc, _d) = scan_cabal(TOOLS);
    let scope_of = |purl: &str| -> Option<String> {
        doc["components"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|c| c["purl"].as_str() == Some(purl))
            .and_then(|c| c["scope"].as_str())
            .map(String::from)
    };
    assert_eq!(
        scope_of("pkg:hackage/waybill-fixture-tool").as_deref(),
        Some("excluded"),
        "a build tool must be marked build-time",
    );
    assert_ne!(
        scope_of("pkg:hackage/waybill-fixture-lib").as_deref(),
        Some("excluded"),
        "a library dependency must not be",
    );
}

#[test]
fn t035_the_declared_executable_is_recoverable() {
    // FR-010b. The identifier deliberately drops it, so it must survive
    // elsewhere or the pair the project declared is lost.
    let p = props(&scan_cabal(TOOLS).0, "pkg:hackage/waybill-fixture-tool");
    assert_eq!(
        p.get("waybill:cabal-build-tool-executable").map(String::as_str),
        Some("waybill-fixture-exe"),
        "got {p:#?}",
    );
}

#[test]
fn t036_a_package_declared_as_both_is_not_exclusively_build_time() {
    // EC-5. Over-reporting a runtime dependency is recoverable; hiding one
    // from a runtime filter is not.
    let body = r#"cabal-version: 1.12
name:           waybill-fixture-app
version:        0.1

library
  build-tool-depends:
      waybill-fixture-dual:waybill-fixture-dual
  build-depends:
      waybill-fixture-dual >=1 && <2
  default-language: Haskell2010
"#;
    let (doc, _d) = scan_cabal(body);
    let dual: Vec<&Value> = doc["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["purl"].as_str() == Some("pkg:hackage/waybill-fixture-dual"))
        .collect();
    assert_eq!(dual.len(), 1, "expected one component for the name");
    assert_ne!(
        dual[0]["scope"].as_str(),
        Some("excluded"),
        "a package that is also a library dependency must not be hidden from \
         a runtime filter",
    );
}
