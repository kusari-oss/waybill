//! Milestone 198 — versionless PURL round-trip fuzz test.
//!
//! Exercises the `Purl` newtype's parse → re-serialize round-trip across
//! ~1200 synthetic versionless PURL inputs (~110 per ecosystem × 11
//! ecosystems). Catches per-ecosystem corner cases the targeted per-
//! reader unit tests miss — URL-encoded segments, max-length names,
//! scoped-name grammars, ecosystem-specific normalization quirks.
//!
//! Round-trip invariant (research §R1): `Purl::new(s).as_str() == s`
//! for any input `s` where (1) `s` parses successfully, (2) `s`'s
//! qualifier keys are already sorted lex. For versionless PURLs (no
//! qualifiers, the common case), the second condition is trivially
//! true.
//!
//! Zero new Cargo deps. Hand-rolled catalog per spec FR-005. Closes
//! GitHub issue #566.

use std::sync::atomic::{AtomicUsize, Ordering};

use mikebom_common::types::purl::Purl;

/// One catalog entry per ecosystem. See specs/198-purl-fuzz-test/
/// data-model.md Entity 1.
struct EcosystemFuzz {
    /// Human-readable ecosystem label for diagnostic output. Not
    /// necessarily identical to the PURL type (e.g., "scala" uses
    /// PURL type "maven").
    label: &'static str,
    /// The `pkg:<type>/...` identifier's type segment.
    purl_type: &'static str,
    /// Whether the ecosystem requires a namespace segment. When true,
    /// name templates below MUST embed the namespace as `<ns>/<name>`.
    #[allow(dead_code)]
    requires_namespace: bool,
    /// Name-shape templates. Each is exercised across ~10 rotations.
    templates: &'static [NameShape],
}

struct NameShape {
    /// Human-readable identifier for diagnostic emission.
    label: &'static str,
    /// The name (or `<ns>/<name>` for namespaced ecosystems) template.
    /// The rotation counter is appended without a delimiter (`foo` →
    /// `foo0`, `foo1`, ..., `foo9`).
    template: &'static str,
    /// When true, `Purl::new` is EXPECTED to return Err for this
    /// input; a successful parse would be a bug. Used for empty-name,
    /// unicode-in-rejecting-ecosystems, and max-length-plus-one
    /// boundary tests.
    expect_reject: bool,
}

// ---------------------------------------------------------------------
// Per-ecosystem shape templates (~11 templates each — deliberately
// non-DRY across ecosystems so per-ecosystem grammar quirks stay
// visible and reviewable).
// ---------------------------------------------------------------------

const NPM_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "express",                       expect_reject: false },
    NameShape { label: "hyphenated",        template: "body-parser",                   expect_reject: false },
    NameShape { label: "underscored",       template: "some_pkg",                      expect_reject: false },
    NameShape { label: "dotted",            template: "some.pkg",                      expect_reject: false },
    NameShape { label: "scoped",            template: "%40scope/mylib",                expect_reject: false },
    NameShape { label: "scoped-hyphen",     template: "%40my-scope/my-lib",            expect_reject: false },
    NameShape { label: "long-realistic",    template: "really-long-package-name-with-hyphens", expect_reject: false },
    NameShape { label: "numeric-suffix",    template: "pkg1",                          expect_reject: false },
    NameShape { label: "digit-run",         template: "abc123",                        expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const CARGO_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "serde",                         expect_reject: false },
    NameShape { label: "hyphenated",        template: "tokio-util",                    expect_reject: false },
    NameShape { label: "underscored",       template: "cargo_metadata",                expect_reject: false },
    NameShape { label: "dotted-not-typical", template: "some.crate",                   expect_reject: false },
    NameShape { label: "long",              template: "really-long-crate-name-thing",  expect_reject: false },
    NameShape { label: "numeric-suffix",    template: "clap4",                         expect_reject: false },
    NameShape { label: "digit-heavy",       template: "abc123def",                     expect_reject: false },
    NameShape { label: "double-hyphen",     template: "foo--bar",                      expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "single-underscore", template: "_",                             expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const MAVEN_SHAPES: &[NameShape] = &[
    NameShape { label: "simple",            template: "com.example/artifact",          expect_reject: false },
    NameShape { label: "deep-groupid",      template: "org.apache.commons/commons-lang3", expect_reject: false },
    NameShape { label: "single-seg-group",  template: "foo/bar",                       expect_reject: false },
    NameShape { label: "hyphenated-artifact", template: "com.example/my-artifact",     expect_reject: false },
    NameShape { label: "digits-in-name",    template: "com.example/spring5",           expect_reject: false },
    NameShape { label: "long-groupid",      template: "com.google.inject.extensions/guice-assistedinject", expect_reject: false },
    NameShape { label: "scala-artifact",    template: "org.scala-lang/scala-library",  expect_reject: false },
    NameShape { label: "guava",             template: "com.google.guava/guava",        expect_reject: false },
    NameShape { label: "junit-jupiter",     template: "org.junit.jupiter/junit-jupiter", expect_reject: false },
    NameShape { label: "spring-boot",       template: "org.springframework.boot/spring-boot-starter", expect_reject: false },
    NameShape { label: "hibernate",         template: "org.hibernate/hibernate-core",  expect_reject: false },
    NameShape { label: "unicode-artifact",  template: "com.example/lib-café",          expect_reject: false },
];

const GEM_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "rails",                         expect_reject: false },
    NameShape { label: "hyphenated",        template: "activesupport-core-ext",        expect_reject: false },
    NameShape { label: "underscored",       template: "some_gem",                      expect_reject: false },
    NameShape { label: "digit-suffix",      template: "gem2",                          expect_reject: false },
    NameShape { label: "long-realistic",    template: "really-long-gem-name-with-hyphens", expect_reject: false },
    NameShape { label: "double-hyphen",     template: "foo--bar",                      expect_reject: false },
    NameShape { label: "sinatra",           template: "sinatra",                       expect_reject: false },
    NameShape { label: "rspec-core",        template: "rspec-core",                    expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "dotted",            template: "some.gem",                      expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const PYPI_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "flask",                         expect_reject: false },
    NameShape { label: "hyphenated",        template: "django-rest-framework",         expect_reject: false },
    NameShape { label: "underscored",       template: "python_dateutil",               expect_reject: false },
    NameShape { label: "dotted",            template: "zope.interface",                expect_reject: false },
    NameShape { label: "long-realistic",    template: "really-long-python-package-name", expect_reject: false },
    NameShape { label: "digit-heavy",       template: "abc123",                        expect_reject: false },
    NameShape { label: "pep-503-normalized", template: "requests-toolbelt",            expect_reject: false },
    NameShape { label: "numpy",             template: "numpy",                         expect_reject: false },
    NameShape { label: "pandas",            template: "pandas",                        expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const COMPOSER_SHAPES: &[NameShape] = &[
    NameShape { label: "simple",            template: "vendor/pkg",                    expect_reject: false },
    NameShape { label: "hyphenated",        template: "acme/my-library",               expect_reject: false },
    NameShape { label: "symfony-core",      template: "symfony/console",               expect_reject: false },
    NameShape { label: "laravel",           template: "laravel/framework",             expect_reject: false },
    NameShape { label: "digit-suffix",      template: "vendor/pkg2",                   expect_reject: false },
    NameShape { label: "long-realistic",    template: "acme-corp/really-long-package-name", expect_reject: false },
    NameShape { label: "dotted-vendor",     template: "vendor.name/pkg",               expect_reject: false },
    NameShape { label: "double-hyphen",     template: "vendor/foo--bar",               expect_reject: false },
    NameShape { label: "psr-package",       template: "psr/log",                       expect_reject: false },
    NameShape { label: "monolog",           template: "monolog/monolog",               expect_reject: false },
    NameShape { label: "percent-encoded",   template: "vendor/foo%2Bbar",              expect_reject: false },
    NameShape { label: "unicode-pkg",       template: "vendor/lib-café",              expect_reject: false },
];

const PUB_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "http",                          expect_reject: false },
    NameShape { label: "underscored",       template: "flutter_bloc",                  expect_reject: false },
    NameShape { label: "hyphenated-uncommon", template: "some-pkg",                    expect_reject: false },
    NameShape { label: "long-realistic",    template: "really_long_dart_package_name", expect_reject: false },
    NameShape { label: "digit-suffix",      template: "pkg2",                          expect_reject: false },
    NameShape { label: "provider",          template: "provider",                      expect_reject: false },
    NameShape { label: "cupertino-icons",   template: "cupertino_icons",               expect_reject: false },
    NameShape { label: "path-lib",          template: "path",                          expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "dotted",            template: "some.pkg",                      expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const COCOAPODS_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "A",                             expect_reject: false },
    NameShape { label: "camel-case-common", template: "AFNetworking",                  expect_reject: false },
    NameShape { label: "prefixed",          template: "SDWebImage",                    expect_reject: false },
    NameShape { label: "digit-heavy",       template: "Alamofire5",                    expect_reject: false },
    NameShape { label: "hyphenated",        template: "my-pod",                        expect_reject: false },
    NameShape { label: "long-realistic",    template: "ReallyLongPodNameForTesting",   expect_reject: false },
    NameShape { label: "firebase-core",     template: "FirebaseCore",                  expect_reject: false },
    NameShape { label: "sentry",            template: "Sentry",                        expect_reject: false },
    NameShape { label: "reactive-cocoa",    template: "ReactiveCocoa",                 expect_reject: false },
    NameShape { label: "percent-encoded",   template: "Foo%2BBar",                     expect_reject: false },
    NameShape { label: "underscored",       template: "some_pod",                      expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const SCALA_SHAPES: &[NameShape] = &[
    NameShape { label: "cats-effect",       template: "org.typelevel/cats-effect_2.13", expect_reject: false },
    NameShape { label: "akka-http",         template: "com.typesafe.akka/akka-http_2.13", expect_reject: false },
    NameShape { label: "play-json",         template: "com.typesafe.play/play-json_2.13", expect_reject: false },
    NameShape { label: "scalatest",         template: "org.scalatest/scalatest_3",     expect_reject: false },
    NameShape { label: "zio",               template: "dev.zio/zio_3",                 expect_reject: false },
    NameShape { label: "spark-core",        template: "org.apache.spark/spark-core_2.12", expect_reject: false },
    NameShape { label: "circe-core",        template: "io.circe/circe-core_2.13",      expect_reject: false },
    NameShape { label: "cats-core",         template: "org.typelevel/cats-core_2.13",  expect_reject: false },
    NameShape { label: "http4s",            template: "org.http4s/http4s-core_2.13",   expect_reject: false },
    NameShape { label: "sbt-plugin",        template: "com.example/my-sbt-plugin_2.12", expect_reject: false },
    NameShape { label: "no-scala-suffix",   template: "com.example/library-plain",     expect_reject: false },
    NameShape { label: "unicode-artifact",  template: "org.example/lib-café",         expect_reject: false },
];

const HACKAGE_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "aeson",                         expect_reject: false },
    NameShape { label: "hyphenated",        template: "http-conduit",                  expect_reject: false },
    NameShape { label: "camel-case",        template: "QuickCheck",                    expect_reject: false },
    NameShape { label: "digit-suffix",      template: "text2",                         expect_reject: false },
    NameShape { label: "long-realistic",    template: "really-long-haskell-package-name", expect_reject: false },
    NameShape { label: "prime-in-name",     template: "lens",                          expect_reject: false },
    NameShape { label: "cabal-install",     template: "cabal-install",                 expect_reject: false },
    NameShape { label: "ghc",               template: "ghc",                           expect_reject: false },
    NameShape { label: "conduit",           template: "conduit",                       expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const HEX_SHAPES: &[NameShape] = &[
    NameShape { label: "single-char",       template: "a",                             expect_reject: false },
    NameShape { label: "short-common",      template: "phoenix",                       expect_reject: false },
    NameShape { label: "underscored",       template: "phoenix_live_view",             expect_reject: false },
    NameShape { label: "hyphenated-rare",   template: "some-pkg",                      expect_reject: false },
    NameShape { label: "long-realistic",    template: "really_long_elixir_hex_package", expect_reject: false },
    NameShape { label: "digit-heavy",       template: "abc123",                        expect_reject: false },
    NameShape { label: "ecto",              template: "ecto",                          expect_reject: false },
    NameShape { label: "plug",              template: "plug",                          expect_reject: false },
    NameShape { label: "cowboy",            template: "cowboy",                        expect_reject: false },
    NameShape { label: "org-scoped",        template: "hexpm/some_lib",                expect_reject: false },
    NameShape { label: "percent-encoded",   template: "foo%2Bbar",                     expect_reject: false },
    NameShape { label: "empty",             template: "",                              expect_reject: true },
];

const CATALOG: &[EcosystemFuzz] = &[
    EcosystemFuzz { label: "npm",       purl_type: "npm",       requires_namespace: false, templates: NPM_SHAPES },
    EcosystemFuzz { label: "cargo",     purl_type: "cargo",     requires_namespace: false, templates: CARGO_SHAPES },
    EcosystemFuzz { label: "maven",     purl_type: "maven",     requires_namespace: true,  templates: MAVEN_SHAPES },
    EcosystemFuzz { label: "gem",       purl_type: "gem",       requires_namespace: false, templates: GEM_SHAPES },
    EcosystemFuzz { label: "pypi",      purl_type: "pypi",      requires_namespace: false, templates: PYPI_SHAPES },
    EcosystemFuzz { label: "composer",  purl_type: "composer",  requires_namespace: true,  templates: COMPOSER_SHAPES },
    EcosystemFuzz { label: "pub",       purl_type: "pub",       requires_namespace: false, templates: PUB_SHAPES },
    EcosystemFuzz { label: "cocoapods", purl_type: "cocoapods", requires_namespace: false, templates: COCOAPODS_SHAPES },
    EcosystemFuzz { label: "scala",     purl_type: "maven",     requires_namespace: true,  templates: SCALA_SHAPES },
    EcosystemFuzz { label: "hackage",   purl_type: "hackage",   requires_namespace: false, templates: HACKAGE_SHAPES },
    EcosystemFuzz { label: "hex",       purl_type: "hex",       requires_namespace: false, templates: HEX_SHAPES },
];

const ROTATIONS_PER_TEMPLATE: u32 = 10;

/// Static per-ecosystem invocation counter — printed at test end for
/// SC-002 verification (≥ 100 invocations per ecosystem).
///
/// Fixed-size arrays indexed by ecosystem position; keeps the fuzz
/// binary allocation-free.
static COUNTERS: [AtomicUsize; 11] = [
    AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0),
    AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0),
    AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0),
    AtomicUsize::new(0), AtomicUsize::new(0),
];

fn run_one_ecosystem(idx: usize, eco: &EcosystemFuzz) {
    for shape in eco.templates {
        for rotation in 0..ROTATIONS_PER_TEMPLATE {
            let template_with_rotation = if shape.template.is_empty() {
                // Empty templates stay empty (verify Purl rejection).
                String::new()
            } else {
                format!("{}{}", shape.template, rotation)
            };
            let input = format!("pkg:{}/{}", eco.purl_type, template_with_rotation);
            COUNTERS[idx].fetch_add(1, Ordering::Relaxed);

            match Purl::new(&input) {
                Ok(parsed) => {
                    if shape.expect_reject {
                        panic!(
                            "purl parse unexpectedly succeeded\n  \
                             ecosystem: {}\n  shape: {}\n  rotation: {}\n  \
                             input: {}\n  parsed as: {}",
                            eco.label, shape.label, rotation, input, parsed.as_str()
                        );
                    }
                    // Round-trip byte-identity: Purl::as_str() returns
                    // the canonical form; for versionless (no
                    // qualifiers) it should equal the input verbatim.
                    let observed = parsed.as_str();
                    if observed != input {
                        panic!(
                            "purl round-trip drift\n  \
                             ecosystem: {}\n  shape: {}\n  rotation: {}\n  \
                             input:    {}\n  observed: {}",
                            eco.label, shape.label, rotation, input, observed
                        );
                    }
                    // Ecosystem accessor sanity — should equal the
                    // catalog's purl_type.
                    if parsed.ecosystem() != eco.purl_type {
                        panic!(
                            "purl ecosystem accessor drift\n  \
                             ecosystem: {}\n  shape: {}\n  rotation: {}\n  \
                             input:    {}\n  observed ecosystem: {}\n  \
                             expected ecosystem: {}",
                            eco.label, shape.label, rotation, input,
                            parsed.ecosystem(), eco.purl_type,
                        );
                    }
                }
                Err(e) => {
                    if !shape.expect_reject {
                        panic!(
                            "purl parse unexpectedly failed\n  \
                             ecosystem: {}\n  shape: {}\n  rotation: {}\n  \
                             input: {}\n  error: {}",
                            eco.label, shape.label, rotation, input, e
                        );
                    }
                }
            }
        }
    }
}

// -----------------------------------------------------------------
// Per-ecosystem `#[test]` decomposition (quickstart Reproducer 4).
// -----------------------------------------------------------------

#[test]
fn versionless_purl_fuzz_npm()       { run_one_ecosystem(0,  &CATALOG[0]);  print_counter(0);  }
#[test]
fn versionless_purl_fuzz_cargo()     { run_one_ecosystem(1,  &CATALOG[1]);  print_counter(1);  }
#[test]
fn versionless_purl_fuzz_maven()     { run_one_ecosystem(2,  &CATALOG[2]);  print_counter(2);  }
#[test]
fn versionless_purl_fuzz_gem()       { run_one_ecosystem(3,  &CATALOG[3]);  print_counter(3);  }
#[test]
fn versionless_purl_fuzz_pypi()      { run_one_ecosystem(4,  &CATALOG[4]);  print_counter(4);  }
#[test]
fn versionless_purl_fuzz_composer()  { run_one_ecosystem(5,  &CATALOG[5]);  print_counter(5);  }
#[test]
fn versionless_purl_fuzz_pub()       { run_one_ecosystem(6,  &CATALOG[6]);  print_counter(6);  }
#[test]
fn versionless_purl_fuzz_cocoapods() { run_one_ecosystem(7,  &CATALOG[7]);  print_counter(7);  }
#[test]
fn versionless_purl_fuzz_scala()     { run_one_ecosystem(8,  &CATALOG[8]);  print_counter(8);  }
#[test]
fn versionless_purl_fuzz_hackage()   { run_one_ecosystem(9,  &CATALOG[9]);  print_counter(9);  }
#[test]
fn versionless_purl_fuzz_hex()       { run_one_ecosystem(10, &CATALOG[10]); print_counter(10); }

/// Prints the current ecosystem's invocation count AND asserts the
/// FR-002 ≥ 100 floor. Self-contained per test — no cross-test
/// ordering dependency (cargo runs #[test]s in arbitrary parallel
/// order, so a global cross-test floor check is brittle).
fn print_counter(idx: usize) {
    let count = COUNTERS[idx].load(Ordering::Relaxed);
    println!(
        "[versionless-purl-fuzz] {}: {}",
        CATALOG[idx].label, count
    );
    assert!(
        count >= 100,
        "m198 FR-002 violation: ecosystem {} invocation count = {} < 100 floor",
        CATALOG[idx].label, count
    );
}
