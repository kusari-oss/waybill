# m236 unresolved-reason fixtures

Per-reader minimal fixtures. Each fixture produces at least one design-tier
component when scanned, so the m236 integration test can assert every one
carries `waybill:unresolved-reason` with the reader-specific string per
`specs/236-unresolved-reason/contracts/per-reader-strings.md`.

Every fixture uses **synthetic package names** (`waybill-fixture-*`,
`com.example.waybillfixture:*`, etc.) per the
`feedback_fixture_synthetic_package_names` project convention. Never use
real coordinates; Kusari Inspector's advisory scan will trip.

Milestone 236 — closes issue #659.
