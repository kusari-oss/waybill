# Contract: `--experimental-cross-ecosystem-edges` CLI flag

**Feature**: 218-cross-ecosystem-edges | **Related**: FR-000, FR-008

## Surface

Add ONE boolean argument to `waybill_cli::cli::scan_cmd::ScanArgs` via `clap::ArgAction::SetTrue`.

### Long form
```
waybill scan --path <dir> --experimental-cross-ecosystem-edges
```

### Env-var equivalent
```
WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=1 waybill scan --path <dir>
```

### Not permitted
- Short form (`-x` etc.): NOT ALLOWED. Experimental flags never take short forms per waybill convention.
- Negated form (`--no-experimental-cross-ecosystem-edges`): NOT ALLOWED. Default off; explicit opt-in is the ONLY interaction shape.
- Boolean value binding (`--experimental-cross-ecosystem-edges=true|false`): clap's `SetTrue` action forbids explicit value binding; presence-vs-absence is the sole semantic.

## Precedents (waybill flag naming conventions)

- **m173 `--warm-go-cache`** (env `WAYBILL_WARM_GO_CACHE`) — same `SetTrue` pattern for an opt-in resolver-behavior change.
- **m119 `--supplement-cdx <file>`** (env `WAYBILL_SUPPLEMENT_CDX`) — same env-var alias.
- **m112 `--go-build-inclusion`** — precedent for classifier-behavior opt-in without "experimental" prefix.

## Rationale for "experimental" prefix

Per FR-000: the annotation shape MAY evolve before graduation. Consumers who pin to the flag today accept that graduation will be called out in release notes. The prefix conveys "this may change" more directly than an unqualified `--cross-ecosystem-edges` name.

## Default behavior (flag off)

- Resolver at `scan_fs/mod.rs:794-810` behaves EXACTLY as post-m216 today.
- No new annotations emitted (byte-identity gate SC-009).
- No INFO log line emitted (FR-013 guards on flag state).
- `ScanArtifacts.cross_ecosystem_edges_report` is `None`.

## Behavior with flag on

- Resolver at `scan_fs/mod.rs:794-810` extends per FR-001/002/003/004.
- CDX / SPDX 2.3 / SPDX 3 emitters propagate the three new annotations per FR-005/006/007.
- FR-013 INFO log emits once at resolver-exit: `INFO  cross-ecosystem edges: resolved=N ambiguous=M unresolved=K`.
- `ScanArtifacts.cross_ecosystem_edges_report` is `Some(report)` even when the report is otherwise empty (a scan with no `pkg:generic/` main-modules still gets `Some(default())`).

## Verification

- SC-009 integration test: scan the fastlane fixture WITHOUT the flag; assert output byte-identical to a golden captured at post-m216 waybill (committed at `waybill-cli/tests/fixtures/cross_ecosystem/golden_flag_off.cdx.json`).
- SC-001/SC-002 integration tests: scan the fastlane fixture WITH the flag; assert edge count ≥ 221 (per R7 math) and count-of-`pkg:generic/`-sourced edges ≥ 24.
- Clap `--help` output contains the FR-014 doc-page reference.

## Failure modes

- Flag present + invalid combination (there is no invalid combination in v1 — the flag is standalone). No new error paths introduced.
- Env-var set to invalid value (e.g. `WAYBILL_EXPERIMENTAL_CROSS_ECOSYSTEM_EDGES=maybe`): clap's `SetTrue` action treats ANY non-empty env value as truthy. To disable, `unset` the env var or set to empty string. Documented in `docs/reference/cross-ecosystem-edges.md`.

## Backward compatibility

- Absence of the flag: pre-m218 SBOMs are indistinguishable from flag-off m218 SBOMs (SC-009 gate).
- Presence of the flag on a scan with zero `pkg:generic/` main-modules: emitted SBOM is byte-identical to a flag-off scan of the same input (FR-008 clarification: the trigger requires `pkg:generic/` source, so absent that, no new annotations fire).
