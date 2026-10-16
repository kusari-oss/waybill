//! FR-020 / FR-020a — the two terms must be distinguishable by name
//! alone, so the "property of any SBOM, or expectation about this
//! target?" decision can be made without reading each check's body.
//!
//! This is a source-level gate for the same reason the FR-016 one is:
//! the claim is about what the names *are*, which no runtime
//! observation can establish.

use std::path::PathBuf;

fn harness_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpus_harness_195")
}

fn read(name: &str) -> String {
    let p = harness_dir().join(name);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every public check in `expectations.rs` is named as an expectation,
/// with exactly one documented exception.
#[test]
fn every_public_check_in_expectations_is_named_as_one() {
    let src = read("expectations.rs");
    let mut offenders = Vec::new();
    for line in src.lines() {
        let Some(rest) = line.trim().strip_prefix("pub fn ") else { continue };
        let name = rest.split('(').next().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        // `layer0_document_integrity` is universal, so by the rule it is
        // an invariant. It stays in this file because spec Assumption 6
        // left layer 0 un-re-homed; its name says `layer0`, not a
        // target, so the name-alone test still passes.
        if name == "layer0_document_integrity" {
            continue;
        }
        if !name.ends_with("_expectations") {
            offenders.push(name.to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "public checks in expectations.rs must be named `<target>_expectations` so FR-020a's \
         name-alone test holds: {offenders:?}"
    );
}

/// The `I<n>` namespace belongs to the catalog alone (FR-001a).
///
/// Layer 0's three checks were once `i2-…` and `i3-…` with diagnostics
/// reading "invariant I2" / "invariant I3". Once the catalog existed
/// those numbers named different properties, so a maintainer following a
/// layer-0 diagnostic landed on the wrong row. They are `L2` / `L3` now
/// and this test is what stops the collision coming back.
#[test]
fn expectations_do_not_borrow_the_catalog_s_number_namespace() {
    let src = read("expectations.rs");
    let mut offenders = Vec::new();
    for (n, line) in src.lines().enumerate() {
        let Some(idx) = line.find("check_name: \"") else { continue };
        let value = &line[idx + "check_name: \"".len()..];
        let value = value.split('"').next().unwrap_or("");
        let mut chars = value.chars();
        if chars.next() == Some('i') && chars.next().is_some_and(|c| c.is_ascii_digit()) {
            offenders.push(format!("{}: `{value}`", n + 1));
        }
    }
    assert!(
        offenders.is_empty(),
        "a check named `i<n>-…` reads as catalog row I<n> and is not one. Layer-0 checks are \
         `l<n>-…`; catalog rows live in docs/reference/sbom-invariants.md:\n  {}",
        offenders.join("\n  ")
    );
}

/// And the inverse: a catalogued invariant must not be named after a
/// target, which would make it an expectation wearing an invariant's
/// number.
#[test]
fn no_catalogued_invariant_is_named_after_a_corpus_target() {
    let targets: Vec<&str> = super::super::manifest::TARGETS.iter().map(|t| t.name).collect();
    for inv in super::REGISTRY {
        for t in &targets {
            let bare = t.replace('-', "");
            assert!(
                !inv.statement.to_lowercase().contains(t)
                    && !inv.statement.to_lowercase().contains(&bare),
                "{} names the target `{t}` in its statement; a claim about one target is an \
                 expectation, not an invariant (FR-020)",
                inv.id
            );
        }
    }
}
