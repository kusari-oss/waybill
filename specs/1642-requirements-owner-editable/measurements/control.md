# T006, T010: controls on `main`'s binary

Every case in `waybill-cli/tests/requirements_owner.rs` was run against `main`'s binary (`79b3350b`, built before any source change and saved as `waybill-main-79b3350b`), through `WAYBILL_REQUIREMENTS_OWNER_BIN`. B-1's golden was generated from the same binary (`WAYBILL_UPDATE_REQUIREMENTS_OWNER_GOLDENS=1`).

Result: `test result: FAILED. 144 passed; 8 failed` (the 135 other passes are the corpus-harness module's own unit tests, compiled into this target as in `declared_url_credentials.rs`).

| case | expected on `main` | result on `main` | failing assertion |
|---|---|---|---|
| O-1 | fail | FAILED | no edge from `waybill-fixture-tool` into `waybill-fixture-shared` |
| O-2 | pass (guard) | ok | |
| O-3 | fail | FAILED | dev package C18 `["tool/uv.lock"]`, want `["tool/devtool", "tool/uv.lock"]` |
| O-4 | fail | FAILED | `waybill-fixture-other` attached to `waybill-fixture-tool`, want `waybill-fixture-zdevtool` |
| O-5 | pass (guard) | ok | |
| O-6 | fail | FAILED | member `waybill-fixture-a` C120 `["."]`, want `["packages/a"]` |
| O-7 | fail | FAILED | `waybill-fixture-shared` attached to `waybill-fixture-devtool`, want `waybill-fixture-tool` |
| O-8 | pass (guard) | ok | |
| O-10 | fail | FAILED | edge from the dev package (virtual source) |
| O-11/O-12 | pass (guard) | ok | |
| A-1 | fail | FAILED | `waybill-fixture-shared` scope `excluded`, want `required` |
| A-2 | pass (guard) | ok | |
| A-3 | pass (guard) | ok | |
| A-4 | fail | FAILED | `waybill-fixture-shared` scope `excluded`, want `required`; the guard (submodule and dev edges present) passed first |
| A-5 | pass (guard) | ok | |
| A-6 | pass (guard) | ok | |
| B-1 | pass (golden from `main`) | ok | |

Harness corrections made while establishing the controls, before any product change:
- `waybill-fixture-shared>=1.0` gives an unversioned PURL, so lookups match a bare `pkg:type/name` with or without a version.
- A main module chosen as root is listed as `metadata.component` and again in `components` (#1633, on `main`). Lookups dedupe by `bom-ref`; the duplicate-`bom-ref` check covers `components` only.
