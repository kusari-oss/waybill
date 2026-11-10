# Research: milestone 1377

Every decision here rests on a measurement in `measurements/` or a code
citation on `main@49a24a63`. Paths are under `waybill-cli/src/` unless stated.

## R1 — Where declared CPEs come from, and how members are joined to them

**Decision.** Read `meta.identifiers.cpe` and `version` in the same `nix eval`
milestone 1281 already runs for licenses (`scan_fs/package_db/nix/eval/licenses.rs::licenses_expr`),
with `cpe` inside its own `builtins.tryEval`. Join a closure member to a row by
exact `.drv` basename first (1281's join); where that fails, by `pname` +
`version` equality with the row's attribute.

**Rationale.** Measured (`measurements/meta-eval-cost/`):
- Cost is within noise: 0.80 s → 0.82 s median, N = 3, on openssh's 358 names. No new evaluation (FR-010).
- License coverage unchanged: 114 → 114 rows.
- The drvPath join reaches 15 of the 17 members carrying a nixpkgs-declared CPE, plus the root.
- It misses stdenv's `bash` and one of two `perl` derivations. The name+version join recovers both, and in both cases `pkgs.<pname>`'s declared CPE is identical to the CPE of the variant. A CPE names vendor, product and version, not a build.

**Alternatives.**
- drvPath only: leaves bash on the curated `5.3p9` encoding, which grype matches to nothing, so SC-003 fails by construction.
- name only: a version-blind join could attach one release's CPE to another.
- A second `nix eval`: costs a call and duplicates 1281's guards.
- The unguarded expression: no row was lost on this closure, but one throwing `identifiers` would take that row's licenses with it.

## R2 — Identifying the evaluated package

**Decision.** Use `ClassifiedClosure.graph.root` (`scan_fs/package_db/nix/closure/graph.rs::root_of`, ~213–225): the derivation no other closure member's `inputs.drvs` names.

**Rationale.** Measured: openssh's root, `32czlyyq…-openssh-10.3p1.drv`, equals `pkgs.openssh.drvPath`. `root_of` already exists, is logged, and anchors graph edges. When `root` is `None` (every node has a consumer), no subject is chosen by this rule and the ladder falls through (spec edge case).

**Consequence: the root must be emitted.** The root classifies `Unreferenced` and is emitted today only if it applies patches or lacks a `pname` (`closure/emit.rs`; test `emit.rs:768-776`). openssh happened to be emitted; moat's root reaches the document only through the Haskell manifest reader. When a closure resolves, the root derivation MUST be emitted. Duplicate-avoidance already exists: `cli/scan_cmd.rs::merge_closure_components` merges every closure component into an existing one on `(name, version)`, and folds the closure's annotations into it with `or_insert`. So moat's root (`moat`, `0.1`) merges into the reader's `pkg:hackage/moat@0.1`, and the evaluated-package tag arrives with it. SC-008 checks this, and a test asserts that the tag survives the merge.

## R3 — The root-selection rung

**Decision.**
- Add `ScanTargetKind::NixClosure`. It carries no data, so `Copy`/`Eq` hold. It is set in `cli/scan_cmd.rs` (~3598) when a closure resolved.
- Add `RootSelectionHeuristic::NixEvaluatedPackage`, emitted as `"nix-evaluated-package"`, confidence 0.99 (as `explicit-scan-target`: the operator named the installable).
- The closure tags the root's component with an internal, never-emitted annotation key, registered in `generate/root_selector.rs::is_internal_emission_key`.
- In `select_root`, after the operator override and before the main-module rungs, `kind == NixClosure` with exactly one tagged component returns `ResolvedRootSubject::MainModule(idx)` with heuristic `NixEvaluatedPackage`. `losers` holds every other main-module.

**Rationale.**
- `MainModule(idx)` already makes the subject a real component in all three formats. CycloneDX takes `metadata.component` from it, including its own CPE (`cyclonedx/metadata.rs` ~1275–1301). SPDX 2.3 describes the existing package (`spdx/document.rs` ~515). SPDX 3 roots at its IRI (`spdx/v3_document.rs` ~1107).
- `ExplicitImage` returns a *synthetic* subject, the wrong shape here.
- `builder.rs:841` and `:1313` test `kind != ExplicitImage`; a component subject must keep `subject_is_a_main_module = true`, which `NixClosure` does.

**C69 emission gate unchanged.** `waybill:root-selection-heuristic` is emitted only when `losers` is non-empty, for every rung (`cyclonedx/metadata.rs:879-890`, `spdx/annotations.rs:806-829`, `spdx/v3_annotations.rs:810-833`). So moat (which has losers today: it emits `repo-root-main-module`) will record `nix-evaluated-package`, and openssh (no main-modules) records none. This matches `explicit-scan-target`. FR-004 is refined to say so.

**Alternatives.**
- A data-carrying `ScanTargetKind` (loses `Copy`, touches every call site).
- Matching the root by name in the selector (fragile: three `perl`s).
- A new `ResolvedRootSubject` variant (every emitter's match would change for identical behaviour).

## R4 — Precedence, and keeping the curated table as fallback

**Decision.** Set a member's `cpes = [declared]` during closure emission, before `fill_missing_cpes` (`cli/scan_cmd.rs:4907`, body ~6701), which skips components whose `cpes` is non-empty and so supplies the curated CPE only where none was declared (FR-006). Where a declared CPE exists, compute what `generate/cpe.rs::synthesize_cpes` would have produced; if it differs, record it as overridden (FR-008). Never emit it as a second identifier (FR-007, clarification Q1).

**Merged members (FR-013).** `merge_closure_components` copies a closure member's licenses (only into an empty slot) and its annotations, but not its `cpes`. Two CPE sources are already on components when it runs (`cli/scan_cmd.rs:4755`):

- the curated table, assigned inside `read_all` (`scan_fs/mod.rs:1346`);
- the operator's `--supplement-cdx`, merged at `cli/scan_cmd.rs:4334`.

The merge therefore ranks sources:

1. a declared CPE replaces the existing one when that one equals `synthesize_cpes(existing)` (the curated value) or is empty;
2. otherwise the existing CPE is kept, and the closure's CPE provenance is not folded in, so provenance always describes the CPE actually emitted.

Before relying on equality alone, the implementation checks how supplement-asserted CPEs are marked: an operator could assert exactly the value the curated table would produce.

**Validation (Principle IV).** `ResolvedComponent.cpes` is `Vec<String>` (`waybill-common/src/resolution.rs:43`); there is no CPE type. A declared CPE is parsed into a validated CPE 2.3 formatted-string type, `DeclaredCpe`, which is the type at every closure-tier function boundary. It converts to `String` exactly once, at insertion into the shared field. A value that does not parse is not emitted and is counted (R6).

Decision 2026-11-09 (analysis C1, option a): the shared field's `Vec<String>` predates this milestone and is a standing Principle IV violation for every ecosystem (108 references in 49 files). It is filed as its own issue rather than widened, or fixed, here.

## R5 — Recording a CPE's source (FR-008): the Principle V audit

| format | native construct for an identifier's provenance | verdict |
| --- | --- | --- |
| CycloneDX 1.6 | `component.evidence.identity[]` with `field: "cpe"`, `concludedValue`, `methods[{technique, confidence, value}]`. waybill already emits the array for `purl` (`cyclonedx/evidence.rs` ~136–164); `field: "cpe"` is unused. | **native**: technique `manifest-analysis` (value `nixpkgs meta.identifiers.cpe`) for declared; `other` (value `waybill curated CPE table`) for curated |
| SPDX 2.3 | `externalRefs[].comment` on the `SECURITY`/`cpe23Type` ref. The field exists and is used for the source-package PURL (`spdx/packages.rs` ~519–526); the CPE ref sets `comment: None` (~538). | **native**: a fixed comment string per source, matched exactly by the parity extractor, as `SOURCE_PACKAGE_COMMENT` is (`os_source_package.rs:54`) |
| SPDX 3.0.1 | `ExternalIdentifier.comment` on the `cpe23` identifier. Used for the source-package PURL and `--component-id` (`spdx/v3_external_ids.rs` ~50–61, `v3_id_type_map.rs:149`). `issuingAuthority` names who issues the identifier (NVD), not who assigned it here. | **native**: `comment`, same strings |

**Finer-grained detail no format expresses** is the join basis (`drv` | `name-version`) and the curated CPE a declared one overrode. It goes in one per-component annotation, `waybill:cpe-provenance` `{basis, overrode}`, emitted only on nixpkgs-declared CPEs, in all three formats, and documented as row **C212** with a justification clause naming the missing native field. Principle V permits this; it is the C207 pattern (`docs/reference/sbom-format-mapping.md:247`).

**Rejected.**
- Reusing C67 `waybill:assertion-conflict`: its contract is supplement-vs-scanner, with closed enums `winner ∈ {scanner, supplement}`.
- An annotation-only design: native carriers exist in all three formats, so Principle V forbids it.
- Dual CPE identifiers: clarification Q1.

**Scope.** Only closure components, and only in a scan that resolved a closure. Curated CPEs elsewhere stay byte-identical (FR-011).

## R6 — The coverage count (FR-009)

**Decision.** Extend C184's `waybill:nix-closure` JSON (`closure/summary.rs` ~169–210) with
`identifiers: {wanted, received, by-drv, by-name-version, declared, rejected}`, mirroring its existing
`licenses{wanted, received, withheld-ambiguous, degraded}`.

**Audit.**
- CycloneDX `compositions[].aggregate` speaks to the completeness of components and dependencies, not to how many members had metadata evaluated.
- SPDX 2.3 and 3.0.1 have no carrier for it.

So the verdict is KEEP-NO-NATIVE. This extends an existing row, so no new row is needed.

## R7 — The subject without a CPE (FR-012)

`cyclonedx/metadata.rs` ~1275–1301 falls back to the `waybill:` placeholder when a `MainModule` subject's `cpes` is empty. Under `NixEvaluatedPackage` it MUST emit no `cpe` instead. SPDX 2.3 and SPDX 3 describe the existing package on the `MainModule` path and synthesise nothing (`spdx/document.rs` ~515–518, `spdx/v3_document.rs` ~1107–1114), so they already comply. Tests must assert both.

## R8 — What changes in committed output

- `public_corpus/nix-closure-moat` (3 files): CPE provenance on closure members, C184 `identifiers`. The subject does not change: the harness passes `--root-name`, which outranks the new rung. SC-008 bounds this.
- Closure integration tests that pin a document (`tests/closure_*.rs`, `nix_config_closure.rs`) use a stub `nix` fed fixture JSON. Their stub must return the new row fields, or they see no declared CPEs.
- Everything produced without a resolved closure stays byte-identical (SC-007), checked by regenerating every golden and diffing.

## R9 — Probes re-runnable

The spike probes hard-code `target/nix-vuln-spike/`. They take their paths from environment variables before they are used to judge SC-001 to SC-006 and SC-009.
