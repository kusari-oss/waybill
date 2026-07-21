// Milestone 055 — FR-012 integration test pointer.
//
// The wiremock-backed integration tests (`ladder_step3_only_argo_fixture`,
// `offline_makes_no_network_calls`, `ladder_fall_through_with_404_proxy`)
// live in `waybill-cli/src/scan_fs/package_db/golang/graph_resolver.rs::
// wiremock_integration` rather than under `tests/`. The reason is that
// integration tests under `tests/` link against the LIBRARY crate
// (`waybill`), and the resolver code is in the BINARY crate's
// `scan_fs::*` tree. Exposing that tree via the lib would cascade-
// require lib-exposing every other binary-internal module
// (`trace`, `generate`, `resolve`, ...) — too large a structural
// change for milestone 055.
//
// Functionally the tests are equivalent — same wiremock setup, same
// assertions on the FR-009 ladder summary, same SC-001 / SC-005 /
// SC-006 / SC-007 coverage. Only the file location differs.
//
// Run them with:
//
//   cargo +stable test -p waybill --bin waybill \
//     scan_fs::package_db::golang::graph_resolver::wiremock_integration

#[test]
#[ignore = "Wiremock-backed integration tests live in graph_resolver::wiremock_integration; this file is a pointer (see file comment)."]
fn integration_tests_relocated_to_unit_test_module() {
    // Intentionally empty. See module comment above.
}
