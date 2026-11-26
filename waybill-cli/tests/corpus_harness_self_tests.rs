//! Spec 1657 (#1712): the corpus harness's own tests, compiled into one
//! binary. They used to sit inside `corpus_harness_195/`, which 17 test
//! files include, so every one of them compiled and ran all 144.

#[path = "corpus_harness_195/mod.rs"]
mod corpus_harness_195;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/cache/nixpkgs_pin_tests.rs"]
mod cache_nixpkgs_pin_tests;

#[cfg(unix)]
#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/cache/run_with_deadline_tests.rs"]
mod cache_run_with_deadline_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/expectations/i3_tests.rs"]
mod expectations_i3_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/expectations/recall_proxy_tests.rs"]
mod expectations_recall_proxy_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/catalog/tests.rs"]
mod invariants_catalog_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/checks/closure_resolved_tier/tests.rs"]
mod invariants_checks_closure_resolved_tier_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/checks/closure_role_scope/tests.rs"]
mod invariants_checks_closure_role_scope_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/checks/orphan_explained/tests.rs"]
mod invariants_checks_orphan_explained_tests;

#[allow(clippy::unwrap_used)]
#[path = "corpus_harness_self_tests/invariants/checks/scope_propagation/tests.rs"]
mod invariants_checks_scope_propagation_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/declarations/tests.rs"]
mod invariants_declarations_tests;

#[path = "corpus_harness_self_tests/invariants/line_stability.rs"]
mod invariants_line_stability;

#[path = "corpus_harness_self_tests/invariants/mutation_tests.rs"]
mod invariants_mutation_tests;

#[path = "corpus_harness_self_tests/invariants/no_new_io.rs"]
mod invariants_no_new_io;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/report/scope_tests.rs"]
mod invariants_report_scope_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/report/tests.rs"]
mod invariants_report_tests;

#[path = "corpus_harness_self_tests/invariants/terminology.rs"]
mod invariants_terminology;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/invariants/tests.rs"]
mod invariants_tests;

#[path = "corpus_harness_self_tests/js_filter/tests.rs"]
mod js_filter_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/layer2_golden/m865_masking_tests.rs"]
mod layer2_golden_m865_masking_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/layer2_golden/regen_verdict_tests.rs"]
mod layer2_golden_regen_verdict_tests;

#[cfg_attr(test, allow(clippy::unwrap_used))]
#[path = "corpus_harness_self_tests/layer2_golden/s1542_difference_tests.rs"]
mod layer2_golden_s1542_difference_tests;

#[path = "corpus_harness_self_tests/manifest_audit_tests.rs"]
mod manifest_audit_tests;
