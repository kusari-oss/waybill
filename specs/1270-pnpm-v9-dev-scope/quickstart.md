# Verification

`cargo test -p waybill --test pnpm_v9_scope`

`cargo test -p waybill --bin waybill pnpm_v9_importer`

`RUST_LOG=info ./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh`

The fixture records actual package-manager generation. The CLI test invokes the real binary offline and compares package sets in CDX, SPDX 2.3 and SPDX 3. No network, installed dependency tree or mocked reader implementation is used by the tests.

Local verification: 38 pnpm unit controls and 5 real-CLI regressions passed. The complete mandatory gate returned exit 0 and the required positive completion marker. Set command-scoped RUST_LOG=info, the normal CLI default: an inherited warn override suppresses INFO messages asserted by unrelated repository tests. Preserve failed attempts when reporting validation. No goldens or gate scripts were changed.
