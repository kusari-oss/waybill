//! Milestone 195 — Public SBOM Regression Corpus.
//!
//! Opt-in cargo integration test suite that scans a set of public
//! upstream repositories and container images with the released
//! `waybill` binary and asserts a hybrid two-layer invariant model
//! per target: (1) coarse Rust-defined assertions with class-of-bug-
//! oriented diagnostics; (2) full-SBOM byte-identity golden diff.
//!
//! Gated behind `WAYBILL_RUN_PUBLIC_CORPUS=1` — the default
//! `cargo test` / `./scripts/pre-pr.sh` invocation MUST NOT clone
//! corpus repos or pull corpus images. Nightly CI and manual
//! `workflow_dispatch` are the primary invocation paths per Q2 spec
//! clarification.
//!
//! See `specs/195-public-corpus-fixtures/` for the full spec /
//! plan / research / data-model / contracts / quickstart.

#![cfg_attr(test, allow(clippy::unwrap_used))]

#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

use corpus_harness_195::harness::{
    env_gate, scan_target, skip_oci_gate, update_goldens_gate, FailureFormat,
};
use corpus_harness_195::layer2_golden::compare_golden;
use corpus_harness_195::manifest::{Ecosystem, SourceKind, TARGETS};

/// Skeleton test — proves the `public_corpus` test binary compiles
/// end-to-end from the very first commit and that the env-gate
/// helper is wired.
#[test]
fn env_gate_skips_when_unset() {
    // Paranoia test — if `WAYBILL_RUN_PUBLIC_CORPUS` is not exactly
    // "1", the harness MUST NOT invoke `scan_target`. This test runs
    // even in the default cargo lane; it MUST NOT do any network I/O.
    let expected_gated = std::env::var("WAYBILL_RUN_PUBLIC_CORPUS").as_deref() == Ok("1");
    assert_eq!(env_gate(), expected_gated);
}

// -----------------------------------------------------------------------
// Per-target tests
// -----------------------------------------------------------------------

fn find_target(name: &str) -> &'static corpus_harness_195::manifest::CorpusTarget {
    TARGETS
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("m195: no target named {name} in TARGETS manifest"))
}

/// One post-scan layer: a check over the emitted documents that
/// either passes or produces a diagnostic. Boxed so the sequence can
/// be driven as data — see `run_layers`.
type Layer<'a> = Box<dyn FnMut() -> Result<(), String> + 'a>;

/// The post-scan sequence: the layers that precede the golden write,
/// then the write itself.
///
/// #1286 — in regen mode the write MUST be reachable from a failing
/// earlier layer. It was not. Every pre-write layer panicked, so a
/// target that failed its invariant gate never reached the write, and
/// the `corpus-goldens-regen` artifact then held the *committed* bytes
/// for exactly the target whose goldens needed regenerating: 51 files
/// present, `Upload regenerated goldens = success`, 0 of 51 differing
/// — a result indistinguishable from a change that moves no goldens.
/// Run 40401082720 against 40402280880, same branch, one commit apart.
///
/// It also made the refresh circular. A stale declaration blocked the
/// regeneration that would have made the declaration unnecessary, so
/// #1272 needed two dispatches and two commits to get through it.
///
/// Outside regen mode nothing changes: the first failing layer stops
/// the sequence, the write never runs, and the reported message is
/// that layer's own, verbatim.
///
/// This is a driver over closures rather than a straight-line sequence
/// inside `run_target` because the property at stake is an ordering
/// one, and an ordering inside `run_target` can only be observed by
/// dispatching the corpus lane. An ordering asserted only by a
/// dispatch is asserted nowhere a reviewer can run.
fn run_layers(regen: bool, pre: &mut [Layer<'_>], write: &mut Layer<'_>) -> Result<(), String> {
    let mut deferred: Option<String> = None;
    for layer in pre.iter_mut() {
        if let Err(diagnostic) = layer() {
            if !regen {
                return Err(diagnostic);
            }
            // Stop at the first failure even in regen mode. The later
            // layers read the same documents, and running them after one
            // has already reported those documents malformed widens the
            // exposure to an expectation that panics instead of
            // returning — which would skip the write again, the very
            // thing this exists to prevent.
            deferred = Some(diagnostic);
            break;
        }
    }
    write()?;
    match deferred {
        None => Ok(()),
        Some(diagnostic) => Err(format!(
            "{diagnostic}\n\nThis target's goldens WERE regenerated before this was reported, \
             so they are present in the `corpus-goldens-regen` artifact rather than missing \
             from it (#1286). Whether the artifact is complete overall is the \
             `Assert every target regenerated its goldens` step's answer, not this one's."
        )),
    }
}

/// The universal semantic-invariant layer (milestone 1155).
///
/// Every catalogued invariant is evaluated against this target's
/// freshly emitted documents, and all failing cells are reported
/// together rather than stopping at the first — a `Result` cannot
/// express a matrix, which is why this layer has its own outcome
/// type. An undeclared failure fails the lane; a declared one with
/// the declared kind passes, whatever its count.
///
/// The same matrix also runs hermetically over the committed goldens
/// in the default `cargo test` lane
/// (`invariants::tests::golden_matrix_is_evaluable_and_gated`). The
/// two are complementary: the goldens catch a refresh that imports a
/// defect, this path catches a defect before a refresh can hide it.
fn invariant_layer(target_name: &str, sboms: &corpus_harness_195::harness::EmittedSboms) -> Result<(), String> {
    use corpus_harness_195::invariants;
    let cells = invariants::evaluate_target(target_name, sboms);
    let declarations = invariants::declarations::load();
    let errors = invariants::declarations::validate(&declarations);
    if !errors.is_empty() {
        return Err(format!(
            "invariant-declarations.toml is invalid, so no cell can be trusted:\n  {}",
            errors.join("\n  ")
        ));
    }
    // `build_for_target`, not `build`: FR-006a's applicability floor
    // is a corpus-wide rule, and an invariant can legitimately be
    // not-applicable here while applying elsewhere. Reporting it
    // per-target printed "N lane failure(s)" for targets that had
    // none, on every target.
    let report = invariants::report::build_for_target(cells, &declarations);
    println!("{}", report.console());
    let failures = report.lane_failures();
    if failures.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} invariant cell(s) failed for {}:\n  {}\n\nEach is either a defect to fix \
         or a cell to declare in \
         waybill-cli/tests/corpus_harness_195/invariant-declarations.toml with a \
         written reason. See docs/reference/sbom-invariants.md for the diagnostic.",
        failures.len(),
        target_name,
        failures.join("\n  ")
    ))
}

/// Layer 2 — full-SBOM byte-identity golden diff, or the golden write
/// when regen mode is on.
///
/// Every format is compared before reporting, and all failures are
/// reported together. Panicking on the first one aborts the test
/// before the later formats are ever compared, which has two costs:
/// a reviewer fixing CDX drift only discovers SPDX drift on the next
/// run, and — the reason this changed — the lane cannot demonstrate
/// that the SPDX comparisons execute at all, because a CDX failure
/// always preempts them. Milestone 840 T024 requires exactly that
/// demonstration: a format whose comparison silently no-ops must not
/// be able to hide behind the other two.
fn layer2_golden(
    target_name: &str,
    sboms: &corpus_harness_195::harness::EmittedSboms,
    regen: bool,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for fmt in [FailureFormat::Cdx, FailureFormat::Spdx23, FailureFormat::Spdx3] {
        if let Err(fail) = compare_golden(target_name, fmt, sboms) {
            failures.push(format!("{fail}"));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "{} of 3 formats drifted for {}:\n\n{}",
            failures.len(),
            target_name,
            failures.join("\n\n"),
        ));
    }
    if regen {
        // #1286 — the positive signal, per target. A target that never
        // reached this point cannot print it, which is what lets the lane
        // name a target whose goldens are missing from the artifact
        // instead of shipping an artifact that only looks complete.
        println!("{}", regen_written_line(target_name));
    }
    Ok(())
}

/// The one line `.github/workflows/public-corpus.yml` greps to learn
/// which targets regenerated their goldens.
///
/// A function rather than an inline `format!` so the wording lives in
/// exactly one place, and so
/// `issue_1286_write_ordering::the_workflow_greps_the_line_this_harness_prints`
/// can assert that the workflow's pattern still matches what this
/// produces. Reword it freely; that test says whether the workflow
/// followed.
fn regen_written_line(target_name: &str) -> String {
    format!("corpus-regen: {target_name}: goldens written for all 3 formats")
}

fn run_target(name: &str) {
    if !env_gate() {
        println!("skipping {name}: WAYBILL_RUN_PUBLIC_CORPUS not set");
        return;
    }
    let target = find_target(name);
    // WAYBILL_CORPUS_SKIP_OCI gate for image targets.
    if matches!(&target.source, SourceKind::OciImage { .. }) && skip_oci_gate() {
        println!("skipping {name}: WAYBILL_CORPUS_SKIP_OCI set");
        return;
    }
    let sboms = match scan_target(target) {
        Ok(s) => s,
        Err(infra_err) => panic!("{infra_err}"),
    };
    let regen = update_goldens_gate();
    let mut pre: Vec<Layer> = vec![
        // Layer 0 — document integrity, identical for every target (#980).
        //
        // Runs before the per-target tripwires because what it catches is not
        // ecosystem-specific: any pass that rewrites a component identity after
        // the dependency edges are built disconnects that component, in every
        // format, without either format raising an error. Schema validation
        // cannot see this class — CycloneDX has no referential-integrity rule
        // for `bom-ref`, and SPDX drops the relationship rather than dangling it.
        Box::new(|| {
            corpus_harness_195::expectations::layer0_document_integrity(target.name, &sboms)
                .map_err(|fail| format!("{fail}"))
        }),
        // Per-target expectations — fast-fail with class-of-bug diagnostic.
        Box::new(|| (target.expectations)(&sboms).map_err(|fail| format!("{fail}"))),
        Box::new(|| invariant_layer(target.name, &sboms)),
    ];
    let mut write: Layer = Box::new(|| layer2_golden(target.name, &sboms, regen));
    if let Err(diagnostic) = run_layers(regen, &mut pre, &mut write) {
        panic!("{diagnostic}");
    }
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_go_cobra() {
    run_target("go-cobra");
}

// #879 — the first multi-module Go target (29 modules). See
// `expectations::go_opentelemetry_expectations`.
#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_go_opentelemetry() {
    run_target("go-opentelemetry");
}

// The `--nix-closure` target. Runs without `--offline` — the one exception,
// see `manifest::ScanMode::NixClosure`. Needs `nix` on PATH.
#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_nix_closure_moat() {
    run_target("nix-closure-moat");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_rust_ripgrep() {
    run_target("rust-ripgrep");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_npm_express() {
    run_target("npm-express");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_python_flask() {
    run_target("python-flask");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_maven_guice() {
    run_target("maven-guice");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_image_postgres16() {
    run_target("image-postgres16");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_example_python() {
    run_target("pants-example-python");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_example_django() {
    run_target("pants-example-django");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_example_jvm() {
    run_target("pants-example-jvm");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_clojure_polyglot() {
    run_target("pants-clojure-polyglot");
}

// pants-example-jvm intentionally omitted — see manifest.rs comment.

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_example_golang() {
    run_target("pants-example-golang");
}

#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_pants_example_javascript() {
    run_target("pants-example-javascript");
}

// #898 — the first Haskell target in either corpus. Its absence is what let
// the #891 defects live, and the gap recurred as #937, #936, #938 and #943
// inside one week, each found by scanning a repository by hand rather than by
// a gate. See `expectations::haskell_aeson_expectations` for the four tripwires.
#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_haskell_aeson() {
    run_target("haskell-aeson");
}

// #969 — the first corpus target that exercises nixpkgs-backed Haskell
// version resolution. `haskell-aeson` has no `flake.lock`, so the whole #947
// path was uncovered: all four of its defects were found by hand or by CI,
// none by a test. This target's `flake.lock` has `original.ref` set with no
// `original.rev`, and `locked.rev` present — the ordinary shape, and exactly
// what the #947 gate rejected. It would have failed on its first CI run.
// See `expectations::haskell_language_server_expectations` for the five
// tripwires.
#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_haskell_language_server() {
    run_target("haskell-language-server");
}

// The SECOND nixpkgs-resolving Haskell target. Until it existed,
// `haskell-language-server` was the only one, so every claim about nixpkgs
// Haskell resolution — including "agrees with `nix eval` on every component"
// after #1032 and #1033 — described one project rather than waybill.
//
// Chosen for a different pinned nixpkgs revision, a different `cabal.project`
// shape (`packages: code/*/*.cabal`, the glob arm of #1032 that no other
// target exercises), and a large transitive population, which is where
// #1033's defect lived and where waybill's disagreement detector still does
// not reach. See `expectations::haskell_security_advisories_expectations`.
#[test]
#[ignore = "public-corpus target: needs WAYBILL_RUN_PUBLIC_CORPUS=1 and a populated corpus cache. Ignored rather than silently early-returning: a gated test that prints `skipping` and reports `ok` is indistinguishable from one that compared goldens, and a green local run then implies corpus coverage it does not have. CI passes --include-ignored. See #918."]
fn corpus_haskell_security_advisories() {
    run_target("haskell-security-advisories");
}

// -----------------------------------------------------------------------
// US4 — byte-identity across two consecutive runs (opt-in-within-opt-in)
// -----------------------------------------------------------------------

#[test]
fn byte_identity_across_two_runs() {
    if !env_gate() {
        println!("skipping: WAYBILL_RUN_PUBLIC_CORPUS not set");
        return;
    }
    if std::env::var("WAYBILL_RUN_BYTE_IDENTITY_SUITE").as_deref() != Ok("1") {
        println!("skipping: WAYBILL_RUN_BYTE_IDENTITY_SUITE not set (this test doubles corpus wall-clock)");
        return;
    }
    // Pick one lightweight target (go-cobra) — running all 6 twice
    // exceeds SC-005's 30-min budget.
    let target = find_target("go-cobra");
    let first = scan_target(target).expect("first scan");
    let second = scan_target(target).expect("second scan");
    let a = std::fs::read(&first.paths.cdx).expect("read first cdx");
    let b = std::fs::read(&second.paths.cdx).expect("read second cdx");
    assert_eq!(
        a, b,
        "m195 SC-006 violation — two consecutive scans of {} produced non-byte-identical CDX output",
        target.name
    );
}

// Silence unused-import warnings when the manifest is not fully populated.
#[allow(dead_code)]
fn _unused_hint_for_ecosystem() -> Ecosystem {
    Ecosystem::Go
}

// -----------------------------------------------------------------------
// #1286 — the ordering property, asserted where a reviewer can run it
// -----------------------------------------------------------------------
//
// These run in the default `cargo test` lane. They do no I/O and need no
// corpus cache: the layers are fakes that record whether they were called.
// What they pin is the order `run_layers` calls them in, which is the
// whole of the defect — the golden write was unreachable from a failing
// earlier layer, and that was observable only by dispatching the corpus
// lane and then diffing the uploaded artifact against the committed tree.
mod issue_1286_write_ordering {
    use super::{run_layers, Layer};
    use std::cell::Cell;

    /// A layer that records whether it ran and returns a fixed outcome.
    struct Fake {
        ran: Cell<bool>,
        outcome: Option<&'static str>,
    }

    impl Fake {
        fn pass() -> Self {
            Self { ran: Cell::new(false), outcome: None }
        }
        fn fail(msg: &'static str) -> Self {
            Self { ran: Cell::new(false), outcome: Some(msg) }
        }
        fn layer(&self) -> Layer<'_> {
            Box::new(move || {
                self.ran.set(true);
                match self.outcome {
                    None => Ok(()),
                    Some(m) => Err(m.to_string()),
                }
            })
        }
    }

    /// Outside regen mode, a failing layer stops the sequence: the write
    /// never runs and the message is that layer's own, unchanged. This is
    /// the pre-#1286 behaviour, and it must stay exactly that — the fix is
    /// scoped to regen mode.
    #[test]
    fn verify_mode_stops_at_the_failing_layer_and_does_not_write() {
        let (a, b) = (Fake::pass(), Fake::fail("layer b says no"));
        let write = Fake::pass();
        let mut pre: Vec<Layer> = vec![a.layer(), b.layer()];
        let mut w = write.layer();

        let verdict = run_layers(false, &mut pre, &mut w);

        assert_eq!(
            verdict.expect_err("a failing layer must fail the lane"),
            "layer b says no",
            "the diagnostic must be the layer's own, with nothing appended"
        );
        assert!(a.ran.get() && b.ran.get(), "both layers up to the failure must run");
        assert!(!write.ran.get(), "the write must not run when a layer failed in verify mode");
    }

    /// The defect. In regen mode a failing layer must still reach the
    /// write, and the reported diagnostic must say so — otherwise the
    /// artifact is complete but the maintainer has no way to know it.
    #[test]
    fn regen_mode_writes_goldens_before_reporting_the_failure() {
        let failing = Fake::fail("I3 / rust-ripgrep: StaleDeclaration");
        let write = Fake::pass();
        let mut pre: Vec<Layer> = vec![failing.layer()];
        let mut w = write.layer();

        let verdict = run_layers(true, &mut pre, &mut w);

        assert!(write.ran.get(), "#1286: regen mode must reach the golden write");
        let diagnostic = verdict.expect_err("the failure must still fail the lane");
        assert!(
            diagnostic.contains("I3 / rust-ripgrep: StaleDeclaration"),
            "the original diagnostic must survive: {diagnostic}"
        );
        assert!(
            diagnostic.contains("WERE regenerated"),
            "the diagnostic must state that the goldens were written, since the artifact \
             looks identical either way: {diagnostic}"
        );
    }

    /// The positive control. Without it, a `run_layers` that always
    /// returned `Err` after writing would satisfy the test above, and a
    /// regen dispatch would fail on every target while reporting that the
    /// goldens were written.
    #[test]
    fn regen_mode_with_no_failures_writes_and_passes() {
        let (a, b) = (Fake::pass(), Fake::pass());
        let write = Fake::pass();
        let mut pre: Vec<Layer> = vec![a.layer(), b.layer()];
        let mut w = write.layer();

        let verdict = run_layers(true, &mut pre, &mut w);

        assert!(verdict.is_ok(), "a clean regen run must pass: {verdict:?}");
        assert!(a.ran.get() && b.ran.get() && write.ran.get(), "every layer must run");
    }

    /// Regen mode stops at the FIRST failing layer, same as verify mode.
    /// The exposure to a later layer that panics instead of returning is
    /// therefore unchanged by this fix — a panic there would skip the
    /// write again.
    #[test]
    fn regen_mode_does_not_run_layers_after_the_first_failure() {
        let (first, second) = (Fake::fail("first"), Fake::pass());
        let write = Fake::pass();
        let mut pre: Vec<Layer> = vec![first.layer(), second.layer()];
        let mut w = write.layer();

        let verdict = run_layers(true, &mut pre, &mut w);

        assert!(verdict.is_err());
        assert!(first.ran.get(), "the first layer must run");
        assert!(!second.ran.get(), "no layer after the first failure may run");
        assert!(write.ran.get(), "the write must still run");
    }

    /// A write that itself fails is reported as the write's own failure,
    /// not wrapped in the deferred-failure note — a golden drift and a
    /// stale declaration are different problems and the message must not
    /// claim goldens were written when they were not.
    #[test]
    fn a_failing_write_reports_its_own_diagnostic() {
        let failing = Fake::fail("stale declaration");
        let write = Fake::fail("3 of 3 formats drifted");
        let mut pre: Vec<Layer> = vec![failing.layer()];
        let mut w = write.layer();

        let verdict = run_layers(true, &mut pre, &mut w);

        let diagnostic = verdict.expect_err("a failing write must fail the lane");
        assert_eq!(
            diagnostic, "3 of 3 formats drifted",
            "the write's own diagnostic, with no claim that goldens were written"
        );
    }
}

// -----------------------------------------------------------------------
// #1286 — the harness and the workflow must agree on one literal
// -----------------------------------------------------------------------
mod issue_1286_log_contract {
    use super::regen_written_line;

    /// The workflow learns which targets regenerated by grepping the log
    /// for a line this harness prints. Nothing connected the two: a
    /// reword here left the workflow matching nothing, which it reports as
    /// "every target is missing" — loud, but pointing at the wrong thing.
    ///
    /// So read the pattern out of the workflow and run it against a line
    /// this harness actually produces. Extracting the pattern rather than
    /// restating it is the point; a copy of the regex in this file would
    /// drift from the workflow exactly as the `println!` did.
    #[test]
    fn the_workflow_greps_the_line_this_harness_prints() {
        let workflow = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("waybill-cli has a parent")
            .join(".github/workflows/public-corpus.yml");
        let yaml = std::fs::read_to_string(&workflow)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", workflow.display()));

        // The `grep -oE '<pattern>'` line that mentions corpus-regen.
        let pattern = yaml
            .lines()
            .find(|l| l.contains("grep -oE") && l.contains("corpus-regen"))
            .and_then(|l| {
                let after = l.split_once("grep -oE '")?.1;
                after.split_once('\'').map(|(p, _)| p.to_string())
            })
            .unwrap_or_else(|| {
                panic!(
                    "no `grep -oE '<pattern>'` line mentioning corpus-regen in {}. \
                     Either the workflow stopped reading this line — in which case the lane \
                     can no longer tell an incomplete goldens artifact from a complete one \
                     (#1286) — or the step was reworded and this test needs to follow.",
                    workflow.display()
                )
            });

        let re = regex::Regex::new(&pattern)
            .unwrap_or_else(|e| panic!("the workflow's pattern {pattern:?} is not a regex: {e}"));

        // A real target name, hyphenated, as `TARGETS` spells them.
        let line = regen_written_line("rust-ripgrep");
        assert!(
            re.is_match(&line),
            "the workflow greps {pattern:?}, which does not match the line this harness \
             prints: {line:?}. The lane would then report every target as missing its \
             goldens. Keep them in step."
        );

        // And the captured name must be the target's, not a prefix of it:
        // `[a-z0-9-]+` is greedy but a pattern anchored too loosely could
        // match `corpus-regen: rust` out of `rust-ripgrep`.
        let matched = re.find(&line).expect("just asserted it matches").as_str();
        assert!(
            matched.contains("rust-ripgrep"),
            "the pattern matched {matched:?}, which does not carry the whole target name — \
             the workflow's sed would then extract a truncated name and report a target \
             that does not exist as missing"
        );
    }
}
