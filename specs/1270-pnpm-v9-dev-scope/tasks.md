# Tasks: pnpm v9 development scope

## Setup
- [x] T001 Record reproduction and policy in specs/1270-pnpm-v9-dev-scope/research.md.
- [x] T002 Define scope contract in specs/1270-pnpm-v9-dev-scope/contracts/scope.md.

## User Story 1
- [x] T003 [US1] Add generated fixture and failing CLI regression in waybill-cli/tests/pnpm_v9_scope.rs.
- [x] T004 [US1] Derive importer scopes in waybill-cli/src/scan_fs/package_db/npm/pnpm_lock.rs.
- [x] T005 [US1] Add legacy, multi-importer, alias, peer and runtime-closure controls in pnpm_lock.rs.
- [x] T006 [US1] Verify identical before/after regression and three-format inventories.

## Final validation
- [x] T007 Record change in CHANGELOG.md and run scripts/pre-pr.sh with real status.
- [x] T008 Freeze local patch and test evidence for independent review, without publication.

Dependencies: T003 precedes T004; T004 precedes T005/T006; T007 follows targeted proof. Work is sequential. No independent agent work is required.
