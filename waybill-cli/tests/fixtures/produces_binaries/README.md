# Test fixtures — `waybill:produces-binaries` (milestone 116)

These fixtures exercise the per-ecosystem main-module extractors that emit
`waybill:produces-binaries` declarations (issue #225 Option B). Each
subdirectory contains a minimal synthetic project whose ecosystem manifest
or filesystem layout declares one or more produced binary names; the
corresponding integration test
(`waybill-cli/tests/produces_binaries_<ecosystem>.rs`) scans the fixture,
parses the emitted SBOM, and asserts the expected declaration shape.

Per-ecosystem subdirs are populated in the PR that ships the extractor:

| Subdir | Populated in | Spec FR |
|---|---|---|
| `cargo/` | milestone 116 PR-A (US1) | FR-005 |
| `npm/` | milestone 116 PR-B (US2) | FR-006 |
| `pip/` | milestone 116 PR-B (US2) | FR-007 |
| `gem/` | milestone 116 PR-B (US2) | FR-008 |
| `maven/` | milestone 116 PR-B (US2) | FR-009 |
| `golang/` | milestone 116 PR-C (US3) | FR-010 |

See `specs/116-produces-binaries/` for the full design.
