# Phase 0 Research: Go Build-Inclusion Clarity

**Feature**: 112-go-build-inclusion | **Date**: 2026-06-11

## R1 — Consumer-visible marker representation (native-construct audit, Constitution V)

**Decision**: One new annotation key, `mikebom:build-inclusion`, open-enum
string values `unknown` | `not-needed`. Carried as a typed field
(`Option<BuildInclusion>`) on the component model (Constitution IV), rendered
per format at emission:

| Status | CycloneDX 1.6 | SPDX 2.3 | SPDX 3.0.1 |
|---|---|---|---|
| `unknown` | property `mikebom:build-inclusion: unknown`; `scope` field absent (consumer default = required, FR-002) | Package annotation `mikebom:build-inclusion: unknown` | Element annotation `mikebom:build-inclusion: unknown` |
| `not-needed` | **native** `scope: "excluded"` + property `mikebom:build-inclusion: not-needed` | Package annotation `mikebom:build-inclusion: not-needed` (parity bridge — see audit) | Element annotation `mikebom:build-inclusion: not-needed` (parity bridge) |
| test-only (via toolchain) | existing native path: `scope: "excluded"` + `mikebom:lifecycle-scope: test` | existing native `TEST_DEPENDENCY_OF` | existing native `LifecycleScopedRelationship` scope `test` |
| derivation | property `mikebom:build-inclusion-derivation: go-mod-why` (on not-needed); `mikebom:lifecycle-scope-derivation: go-mod-why` (on test-tagged, same key PR #332 introduced with value `test-only-closure`) | same annotations | same annotations |

**Native-construct audit (required by Constitution V bullet 5)**:

- *unknown*: CDX `scope` enum is `required|optional|excluded` — no unknown
  value; `optional` asserts a positive fact ("may be omitted") we cannot
  assert. CDX `evidence`/`confidence` describes identity confidence, not
  build inclusion. SPDX 2.3 has no per-package build-inclusion field;
  `NOASSERTION` is a field value, not attachable to an absent relationship
  semantic. SPDX 3 `LifecycleScopeType` (`design|build|development|runtime|
  test|other`) has no unknown, and `RelationshipCompleteness` qualifies
  relationship sets, not per-component inclusion. **No native construct
  exists → custom property justified.**
- *not-needed*: CDX `scope: excluded` IS native — used as the primary
  signal. SPDX 2.3 and SPDX 3 have no excluded-scope equivalent
  (`LifecycleScopeType` has no excluded value) → the
  `mikebom:build-inclusion: not-needed` annotation is the parity bridge,
  to be documented in `docs/reference/sbom-format-mapping.md` with a
  justification clause naming the missing native field.
- *test-only*: native constructs already exist in all three formats
  (milestone 052) and are reused unchanged; only the derivation
  discriminator (finer-grained provenance the standards don't express,
  Constitution X) is custom.

**Rationale**: Maximizes native-field usage; single annotation key keeps
the parity catalog delta to two rows (status + derivation).

**Alternatives considered**: (a) `scope: optional` for unknown — rejected,
asserts a fact we don't know; (b) annotation-only stringly implementation
without a typed field — rejected per Constitution IV; (c) reusing
`mikebom:not-linked` (C41) — rejected, that annotation means "binary
present and module not in BuildInfo", a different, stronger statement.

## R2 — Classification mechanism: `go mod why -m -vendor`

**Decision**: Shell out to `go mod why -m -vendor <module...>` in chunks of
20 modules (cyclonedx-gomod's `FilterModules` pattern, verified against
v1.10.0 source), cwd = the main module's directory. Parse `#`-headed
sections: body `(main module does not need module X)` → **not-needed**;
shortest-chain node with `.test` suffix → **test-only**; any other
non-empty chain → **prod-needed**. Note: the `-vendor` flag means
"exclude tests of dependencies" (it is NOT about `vendor/` directories) —
it is what makes dependency-declared test requirements (the viper→testify
case) come back as "does not need".

**Rationale**: Exactly the evidence source cyclonedx-gomod uses, giving
the parity target by construction (SC-002). Only the Go toolchain can
answer package-level reachability (build tags, pruning, test partitions).

**Alternatives considered**: (a) `go list -deps -json ./...` app-mode
precision — rejected for this milestone: requires a buildable package
graph (heavier failure surface) and answers per-binary, not per-module;
noted as future work. (b) Reimplementing package-graph analysis in Rust —
rejected: duplicating the Go build system is a correctness trap.

## R3 — Subprocess safety, env, and the 60-second budget

**Decision**: Reuse the existing `go_mod_graph.rs` pattern
(`run_go_mod_graph`, lines 81–158: spawn worker thread + `mpsc`
`recv_timeout`, `ErrorClass::Timeout`). New helper carries a **shared
60-second budget** (clarification 2026-06-11): each chunk gets
`budget - elapsed`; budget exhaustion abandons remaining chunks,
classified modules keep their results, unclassified ones fall back to
FR-001 marking. Environment:

- Normal mode: inherit environment (toolchain may download module graph
  data — same posture as the existing `go mod graph` step).
- `--offline` (already bridged to golang code via `MIKEBOM_OFFLINE` env
  var, main.rs:207–211): set `GOPROXY=off`, `GOFLAGS=-mod=mod`,
  `GOTOOLCHAIN=local` on the child — the toolchain answers from local
  cache or fails fast; failure degrades per FR-007. `GOTOOLCHAIN=local`
  also prevents the go.mod `toolchain` directive from triggering a
  toolchain download (edge case: newer-go-required → degrade).

**Rationale**: Proven in-repo pattern; budget semantics satisfy FR-007
without per-chunk tuning; env pinning satisfies FR-012 without trying to
detect network use.

**Empirical addendum (2026-06-11, go 1.26.2, /speckit.analyze)**:
`go mod why -m` exits 0 and reports modules as not needed when module
resolution fails mid-query — reproduced with cold cache + `GOPROXY=off`
AND with an unreachable proxy, for a directly-imported module; `vendor/`
does not prevent it (the module graph still needs go.mod data from the
cache/proxy). Three consequences folded into the contracts: (a) a
`go list all` reliability preflight gates each main-module analysis —
it exits 1 loudly in exactly the scenarios where `go mod why` lies, and
its failure maps to new skip reason `unresolvable-packages`; (b) the
not-needed parse string under the `-vendor` flag is
`(main module does not need to vendor module X)` — parsers match the
`(main module does not need` prefix to cover both phrasings; (c) the
spec's vendored-repo edge case is amended: vendor mode cannot enable
offline classification; it degrades via the preflight instead.

**Alternatives considered**: per-chunk 30s timeouts (could stack to
minutes — violates the clarified total budget); async tokio process
management (the package_db read path is synchronous; not worth the
refactor).

## R4 — Pipeline hook points

**Decision**: Two new passes in `read_all()`
(`mikebom-cli/src/scan_fs/package_db/mod.rs`), after
`apply_go_linked_filter` (line 546) and `apply_go_production_set_filter`
(line 554):

1. `apply_go_mod_why_classification(entries, workspace_roots, config)` —
   Part C. Needs the scan root and offline/disable flags plumbed as
   parameters (today neither filter receives them — new params, same
   plumbing direction as the planned T024/T025 offline threading; the
   `MIKEBOM_OFFLINE` env-var bridge is the interim source the existing
   code already uses). Runs once per main module (multi-module trees:
   per-main-module invocation, a module needed by ANY main module is
   not excluded — spec edge case).
2. `apply_go_build_inclusion_unknown_markers(entries)` — Part B. Pure
   function, runs LAST: any golang source-tier entry carrying
   `mikebom:resolver-step: go-sum-fallback` (legacy.rs:727) or
   `mikebom:orphan-reason: flat-attached-fallback` (legacy.rs:1538–1600)
   that (a) was not classified by pass 1, (b) is not BuildInfo-confirmed,
   and (c) is not the main module → `BuildInclusion::Unknown`.

Evidence-hierarchy enforcement (FR-010): a BuildInfo-confirmed module
(binary present, entry lacks `mikebom:not-linked`) is never marked
not-needed or unknown; `mikebom:not-linked: true` MAY compose with
`not-needed` (consistent signals).

**Rationale**: Keeps classification in the same layer as the existing
G3/G4 filters; ordering guarantees the unknown pass sees final state.

## R5 — Test-tag interplay with PR #332 and `--include-dev`/`--exclude-scope`

**Decision**: Toolchain test classification sets
`LifecycleScope::Test` exactly like existing taggers, so the existing
drop-unless-`--include-dev` behavior (mod.rs:486–500, gated through the
post-052 `--exclude-scope` machinery at main.rs:96–110) applies
unchanged (clarification 2026-06-11). If an entry is already test-tagged
(direct import or #332 closure), the toolchain result keeps the tag and
appends/overwrites the derivation annotation to `go-mod-why` only when
the toolchain agrees; the toolchain never DOWNGRADES an existing test tag
to required (spec edge case). `BuildInclusion::NotNeeded` entries are
NEVER dropped by scope filtering — the CDX builder emits
`scope: "excluded"` for them unconditionally (today builder.rs:599–605
emits scope only when `include_dev`; the new field bypasses that gate).

## R6 — Opt-out flag and hermetic-test determinism

**Decision**: New CLI flag `--no-go-mod-why` (plus env var
`MIKEBOM_NO_GO_MOD_WHY=1`, same dual pattern as `MIKEBOM_OFFLINE`).
Default: analysis ON when a `go` binary is found on PATH. Hermetic
strategy for the test suite:

- All existing golden/integration tests gain `MIKEBOM_NO_GO_MOD_WHY=1`
  via the shared test-env helper, so goldens stay deterministic on hosts
  with and without Go (FR-008/SC-004); Part B markers DO appear in the
  regenerated goldens (intentional churn, spec assumption).
- Dedicated Part C integration tests use a **stub `go` executable** on a
  prepended PATH emitting canned `go mod why` output (success, not-need,
  test-chain, exit-1, and sleep-forever variants) — hermetic on any
  host, covers SC-003 without a real toolchain. Stub tests are
  `#[cfg(unix)]` (shell-script stubs); the Windows lane relies on the
  flag-off goldens.
- One opt-in non-hermetic test (env-gated like the docker-daemon test)
  exercises a real toolchain end-to-end.

**Alternatives considered**: default-off opt-in flag — rejected by spec
assumption (mirrors `go mod graph` default-on posture).

## R7 — Parity catalog + docs updates

**Decision**: Two new rows in `docs/reference/sbom-format-mapping.md`
(auto-parsed by `parity/catalog.rs`): `mikebom:build-inclusion`
(values `unknown`/`not-needed`; SPDX rows carry the parity-bridge
justification naming the missing native excluded-scope field) and
`mikebom:build-inclusion-derivation` (value `go-mod-why`).
`mikebom:lifecycle-scope-derivation` gains the `go-mod-why` value in its
existing row (key introduced by PR #332). Parity extractors/tests
(`tests/parity_cmd.rs`, `transitive_parity_go.rs`) assert cross-format
presence.

## R8 — Module identity matching

**Decision**: `go mod why -m` reports module paths without versions;
match against `PackageDbEntry.name` (module path) for golang
source-tier entries. Entries whose name doesn't appear in any toolchain
answer (e.g., fallback-discovered modules outside the build list — `go
mod why` errors on unknown modules) are queried in a second pass and on
per-module error are left for the unknown-marker pass. The main-module
entry itself (`mikebom:component-role: main-module`) is never queried.

## Resolved Technical-Context unknowns

All NEEDS CLARIFICATION items: none remained after `/speckit.clarify`
(not-needed disposition; 60s budget) and the audits above (marker
representation; subprocess env; flag naming; hermetic strategy).
