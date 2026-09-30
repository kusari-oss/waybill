# Tasks: Nix derivation closure as SBOM content

**Input**: Design documents from `/specs/1035-nix-closure-sbom/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/
**Issues**: **#1034** (closure depth), **#1040** (nixpkgs vulnerability signals)

**Tests**: **Included.** SC-001 through SC-009 are test-shaped, `./scripts/pre-pr.sh`
is a mandatory gate, and the parity-extractor gate fails the build in both
directions.

**Organization**: By user story. Note the ordering exception under Phase 5.

## State of play (2026-09-29)

**Phases 1–2 are done and committed; 10 of 50 tasks.** Resume at **T011**, the
first task of Phase 3.

**The tree passes `cargo clippy -D warnings`** as of the Phase 3/4 commit.
It did not before: every type was dead until emission gave the module a
caller, which is why the chain parse → classify → query → *emit* had to reach
its last link before the gate could pass.

What exists and passes (12 unit tests):

| | |
|---|---|
| `derivation.rs` | parsing, and the store-path basename join |
| `classify.rs` | roles from nix's own `nativeBuildInputs`/`buildInputs` |
| `mod.rs` | bounded query, attribute selection, argv guard reuse |
| `scan_cmd.rs` | `--nix-closure`, `--nix-closure-attr` |

**Reproducing the measurements.** Nothing in this repo holds a closure dump —
they are 5.7–7.4 MB and were taken in a scratch directory that does not
survive. Regenerate against any Nix-built Haskell project:

```sh
cd <project> && nix derivation show -r .#default > /tmp/closure.json
python3 specs/1034-nix-eval-tier/measurements/classify-derivation-closure.py /tmp/closure.json
python3 specs/1035-nix-closure-sbom/measurements/patch-attribution.py /tmp/closure.json
WAYBILL_TEST_CLOSURE_JSON=/tmp/closure.json cargo test -p waybill --bins nix::closure::classify -- --nocapture
```

The last one cross-checks the Rust classifier against the Python one. The
figures throughout these documents came from two public Nix-built Haskell
libraries; the baselines are cited as measurements, not as thresholds a
different project must hit.

**Two things a reader should not have to rediscover.** This branch was cut
from main before PR #1044 merged, so main is merged in at `b2972c6b` to pick
up the argv guard that `plan.md` and `contracts/closure-invocation.md` both
call load-bearing. And the store-path basename join fails *silently* — see
the two tests in `derivation.rs` that pin it from both directions.

---

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelizable — different files, no dependency on incomplete work
- **[Story]**: US1–US5 per spec.md

## Path Conventions

Per plan.md: a new `closure/` module beside milestone 1034's `eval/`, reusing
its pre-flight, argv guard, budget and degradation reasons directly.

---

## Phase 1: Setup

- [X] T001 Create `waybill-cli/src/scan_fs/package_db/nix/closure/{mod,derivation,classify,patches,emit}.rs` with `pub(crate)` stubs, registered from `nix/mod.rs`
- [X] T002 Add `--nix-closure` and `--nix-closure-attr` to `ScanArgs` in `waybill-cli/src/cli/scan_cmd.rs` per contracts/cli-flags.md, with `requires` on the companion
- [X] T003 [P] Build with `cargo build --all-targets` to confirm the new clap fields compile in test helpers, not only the bin — a literal `ScanArgs` initializer exists at `scan_cmd.rs` and `--bins` alone will not catch it

---

## Phase 2: Foundational (Blocking Prerequisites)

**⚠️ No user story work begins until this phase completes.**

- [X] T004 Implement the closure query in `closure/mod.rs`, reusing milestone 1034's `eval::invoke::run_bounded` and pre-flight unchanged; address the flake through the **CLI flakeref form** (`nix derivation show -r <path>#<attr>`), never `builtins.getFlake` — research R4 measured that `getFlake` on a local path requires `--impure`, which the argv guard refuses
- [X] T005 Wire the wall-clock budget around the closure subprocess in `closure/mod.rs`, reusing milestone 1034's pattern. One budget covers acquisition and query, for m1034's measured reason: `getFlake` fetches during evaluation and there is no seam to bound separately
- [X] T006 Implement attribute selection in `closure/mod.rs`: `packages.<system>.default`, operator override, degrade naming available attributes when `default` is absent (FR-015a/b)
- [X] T007 Implement `ClosureMember` parsing in `closure/derivation.rs`. **Output paths are stored without the `/nix/store/` prefix while `env` fields carry it** — compare basenames. Getting this wrong classifies every member as unreferenced, silently; the first classifier reported 1,275 of 1,275 that way
- [X] T008 [P] Implement `DerivationRole` in `closure/classify.rs` from nix's own `nativeBuildInputs`/`buildInputs` families — no name heuristics
- [X] T009 [P] Unit-test the basename join with a fixture where the prefix differs, asserting a non-zero classification — a test that only checks "it parsed" passes on the broken version
- [X] T010 [P] Unit-test role assignment against the measured split, asserting counts in the right order of magnitude rather than exact equality

**Checkpoint**: a closure can be fetched, parsed and classified. Nothing is emitted.

---

## Phase 3: User Story 5 — The operator knows the risk changed (Priority: P1) 🎯 safety gate

**Ordering**: first among the P1s. This path evaluates the scanned project's own
flake, which milestone 1034 does not, and the docs currently tell operators the
opposite. Shipping emission before the statement would leave a live correction
outstanding.

**Independent Test**: assert the argv guard applies on this path, and that the
help and docs state repository-authored expressions are evaluated.

- [X] T011 [P] [US5] Write the failing test: the closure query's argv passes `argv_is_safe`, in `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs` tests
- [X] T012 [US5] Route every closure invocation through the milestone-1034 argv guard in `closure/mod.rs`
- [X] T013 [US5] Verify the guard has teeth here: add `--accept-flake-config` to the closure argv, confirm T011 fails, restore. A flake can request `allow-import-from-derivation` via `nixConfig` and slack-web does
- [X] T014 [US5] Write the failing test: `--offline --nix-closure` starts **no** nix process and degrades with `offline-requested`, mirroring m1034's test. The closure query resolves the flake through nix, which fetches when the store lacks it — a promise of no outbound calls kept only when a cache happens to be warm is not one
- [X] T015 [US5] Write the `--nix-closure` help text stating it evaluates the **project's own flake**, with the sandbox-or-trusted-flake guidance, in `waybill-cli/src/cli/scan_cmd.rs`
- [X] T016 [US5] Correct `docs/reference/nix-evaluation.md`, which states the project's own flake is not evaluated — true of `--nix-eval`, false here (FR-017)

**Checkpoint**: the risk is stated before anything acts on it.

---

## Phase 4: User Story 1 — The SBOM lists what the build consumed (Priority: P1)

**Independent Test**: scan a Nix-built project and assert closure components
appear beyond the manifest-derived set, each traceable to a classified derivation.

- [ ] T017 [P] [US1] Write the failing test: at least 200 components carrying `waybill:closure-role` appear on a measured project (SC-001; baselines are 216 and 218)
- [ ] T018 [P] [US1] Write the failing test: the manifest-derived set survives intact alongside the closure set (FR-003a) — GHC boot libraries and executable-stanza dependencies are absent from the closure by construction, and dropping them would discard the Haskell standard distribution
- [X] T019 [US1] Emit artifact-input and build-tooling members as components in `closure/emit.rs`, supplementing rather than replacing
- [X] T020 [US1] Emit C182 `waybill:closure-role` on every closure-derived component
- [X] T021 [US1] Suppress `Unreferenced` members **except** any that apply a patch (FR-004a) — research R8: `jq` and `lua` are `neither` and carry 6 of moat's 18 CVEs
- [ ] T022 [US1] Emit C184 `waybill:nix-closure` at document scope: attribute, derivation count, per-role counts (FR-018)
- [ ] T023 [P] [US1] Write the failing test for SC-009: scanning one project twice with `default` and with a named attribute produces **different** documents. An override that silently ignored its argument passes every other test here
- [ ] T024 [US1] Assert the flag-off path starts no nix process and emits none of C182–C184

**Checkpoint**: SC-001 and SC-002 satisfiable. US5 + US1 is the minimum shippable pair.

---

## Phase 5: User Story 3 — The evidence says how much to trust it (Priority: P1)

**Ordering exception**: US3 precedes US2 although US2 is the visible feature.
The grade must exist before anything asserts a CVE association, or the first
implementation emits ungraded claims and the grade becomes a retrofit. Same
shape as milestone 1034's safety gate preceding its resolution story.

**Independent Test**: assert no CVE association can be emitted without a grade.

- [X] T025 [P] [US3] Write the failing test: an ungraded CVE association cannot be constructed, in `closure/patches.rs`
- [X] T026 [US3] Implement `EvidenceGrade` in `closure/patches.rs` as an enum with the single variant `FilenameDerived` — not a `bool` and not an `Option`, so a future stronger provenance is distinguishable rather than indistinguishable from a filename match
- [X] T027 [US3] Make the grade non-optional in the type that carries a CVE association, so FR-012a is enforced by construction rather than by a check
- [ ] T028 [US3] Emit C183 `waybill:patch-evidence-grade`

**Checkpoint**: nothing can claim a CVE without saying how it knows.

---

## Phase 6: User Story 2 — Backported patches are visible (Priority: P1)

**Independent Test**: scan a project whose closure carries a CVE-named patch and
assert it appears in `pedigree.patches[]` on the component that applies it.

- [ ] T029 [P] [US2] Write the failing test: `CVE-2019-13232` appears in `pedigree.patches[].resolves[]` on **`unzip`** for both measured projects, and the document validates against the CycloneDX 1.6 schema (SC-004)
- [ ] T030 [P] [US2] Write the failing test: at least 18 and 14 distinct CVEs are recovered (SC-006a). A run recovering 3 and 4 means the implementation scanned derivation names instead of joining — a silent fivefold undercount
- [X] T031 [US2] Implement patch attribution in `closure/patches.rs` via each derivation's own `env.patches` field, resolving store-path basenames (research R7)
- [X] T032 [US2] Extract CVE identifiers from patch basenames with the existing `regex` dep
- [ ] T033 [US2] Implement CycloneDX `pedigree.patches[]` emission in `waybill-cli/src/generate/cyclonedx/pedigree.rs` **and register it in `cyclonedx/mod.rs`** — the first use of `pedigree` in waybill, so this is new emission machinery rather than a new field on an existing path, and an unregistered module compiles to nothing
- [ ] T034 [US2] Emit patches with no CVE as `type: "backport"` with no `resolves` entry — silence would make partial coverage look like absence
- [ ] T035 [US2] Emit the patch total and no-CVE count at document scope (SC-006b). On slack-web that is 320 and 279: ~87% of backports name no CVE, and without the count a consumer reads missing VEX as missing backport
- [ ] T036 [US2] Bridge the patch facts into SPDX 2.3 and SPDX 3, which have no `pedigree` equivalent — the one place the formats differ in capability rather than spelling

**Checkpoint**: SC-004 and SC-006 satisfiable.

---

## Phase 7: User Story 4 — A backport yields both claims (Priority: P2)

**Independent Test**: assert two VEX statements per backport, with different
subjects and both graded.

- [ ] T037 [P] [US4] Write the failing test for SC-005a: `affected` subject to the version and `not_affected` subject to this build, and neither emitted without the other
- [ ] T038 [US4] Extend the OpenVEX emitter in `waybill-cli/src/generate/openvex/` to produce both statements, replacing the blanket `under_investigation` for patch-derived findings only
- [ ] T039 [US4] Carry the evidence grade onto both statements (FR-012a)
- [ ] T040 [US4] Assert a lone `not_affected` cannot be emitted — that is the overclaim FR-011 exists to prevent, and it would let a consumer suppress a real finding on filename evidence

---

## Phase 8: Polish & Cross-Cutting Concerns

- [ ] T041 Add catalogue rows C182–C184 to `docs/reference/sbom-format-mapping.md` **in the same change as** their extractors in `waybill-cli/src/parity/extractors/` — the gate fails in both directions, as milestone 1034 found
- [ ] T042 Edit the catalogue by **exact string match**, never anchored regex, and diff the whole field afterwards
- [ ] T043 [P] Write extractors for C182–C184 across all three formats
- [ ] T044 [P] Implement research task **T-R2**: whether closure composition holds outside Haskell; commit the probe
- [ ] T045 [P] Implement research task **T-R3**: cold-store cost on a clean runner; commit the probe
- [ ] T046 Document the feature in `docs/reference/nix-evaluation.md` alongside `--nix-eval`, including the ~87% no-CVE coverage limit
- [ ] T047 Verify SC-003: with the flag off, every committed corpus golden is unchanged
- [ ] T048 Regenerate corpus goldens **in CI, not locally**, only if T047 shows legitimate churn
- [ ] T049 Run the walker-audit grep locally if any new `fn walk`-shaped function appeared — it is not in `pre-pr.sh`
- [ ] T050 Run `./scripts/pre-pr.sh > /tmp/prepr.log 2>&1; echo "EXIT=$?"` — read the **script's** status, never a pipeline's — then assert `>>> all pre-PR checks passed.` and the per-target `test result: ok. N passed; 0 failed`

---

## Dependencies & Execution Order

```
Phase 1 (Setup)
   │
Phase 2 (Foundational — query, parse, classify)
   │
Phase 3 (US5 safety statement) ◄── first: this path evaluates the project's flake
   │
Phase 4 (US1 components) ── MVP completes here, with US5
   │
Phase 5 (US3 evidence grade) ◄── MUST precede US2
   │
Phase 6 (US2 pedigree)
   │
Phase 7 (US4 VEX)
   │
Phase 8 (Polish)
```

**Hard dependencies**

- Phase 5 before Phase 6. The grade must exist before anything asserts a CVE
  association; retrofitting it would mean shipping ungraded claims first.
- Phase 3 before Phase 4, for the same reason milestone 1034 gated its
  resolution story behind its safety gate.
- T041 and T043 land together; either alone fails the parity gate.
- T048 depends on T047 showing *legitimate* churn. Unexplained churn is a defect
  to investigate, not goldens to refresh.

**Parallel opportunities**

- Phase 2: T007, T008, T009
- Phase 3: T011 (T012–T013 follow)
- Phase 4: T017, T018
- Phase 6: T029, T030
- Phase 8: T043, T044, T045

---

## Implementation Strategy

**MVP = Phases 1–4.** US5 and US1: the closure is emitted, and the operator has
been told what enabling it does. US1 alone is not shippable — it would evaluate
repository-authored expressions while the docs say otherwise.

**Second increment**: Phases 5 and 6 together. US2 without US3 emits ungraded
CVE claims, which is the accuracy failure FR-009 exists to prevent.

**Third**: Phase 7.

**Do not start Phase 8's golden regeneration** until Phase 6 lands — patch
emission changes component content, and goldens regenerated before it would
churn twice.

---

## Notes

- Milestone 1034's `eval/` module supplies the pre-flight, argv guard, budget
  and degradation reasons. Reuse them; do not reimplement.
- `xtask/src/nix_oracle/mod.rs` passes `--impure`. It is a developer tool
  against known targets and is **not** a template for this path.
- Test modules using `.unwrap()` need
  `#[cfg(test)] #[cfg_attr(test, allow(clippy::unwrap_used))]`.
- Anything mutating `PATH` or nix environment routes through
  `crate::testing::EnvGuard::acquire()`, or it races the podman and
  cargo-metadata suites.
