# Phase 1 Data Model: Opt-in `nix eval` resolution tier

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)
**Date**: 2026-09-28

## Entities

### `NixSystem` (newtype, Principle IV)

The platform evaluation targets — e.g. `x86_64-linux`, `aarch64-darwin`.

- **Why a newtype**: it is a domain value that must never be confused with a
  free-form string, and research R2 makes it load-bearing rather than cosmetic —
  passing it explicitly is what allows evaluation to run in **pure** mode, since
  `builtins.currentSystem` is unavailable there.
- **Validation**: must match `<arch>-<os>` with both segments non-empty. An
  unparseable value is an argument error at CLI-parse time, not a scan failure.
- **Default**: the host's, determined *outside* the pure evaluation (one
  separate `nix eval --impure --raw --expr 'builtins.currentSystem'` that
  evaluates nothing repository-controlled, per research R2).

### `NixpkgsRevision` (existing)

Already modelled by the m926/m925 nix reader; read from the project's
`flake.lock` by `scan_fs/package_db/nix/lockfile.rs`. **Reused unchanged.** This
feature adds a second way to answer the question that module already knows how
to ask.

### `EvaluationOutcome`

What one `nix eval` invocation produced, or why it produced nothing.

| Field | Meaning |
|---|---|
| `revision` | the `NixpkgsRevision` evaluated against |
| `system` | the `NixSystem` the result describes |
| `versions` | component name → version, as Nix reports it |
| `pure` | whether evaluation ran without `--impure` (always true for the resolving call) |
| `ifd_refused_verified` | whether the R3 pre-flight confirmed the refusal was *in effect* |

**Invariant, and the reason this field exists**: the tier MUST NOT consume
`versions` unless `ifd_refused_verified` is true. Research R3 measured that an
unsupported `--option` is a warning with exit code 0 — so "we passed the flag"
is not evidence the flag applied, and a boolean recording the *verification*
rather than the *request* is what keeps FR-008 enforceable.

### `ResolutionDivergence`

One component where evaluation and file-parsing disagreed.

| Field | Meaning |
|---|---|
| `component` | which component |
| `evaluated` | the version Nix reported — **wins**, appears in `version` |
| `file_parsed` | the version file-parsing produced — superseded, retained |

**Not to be confused with** the existing catalogue row **C172
`waybill:nixpkgs-version-disagreement`**, which records a *locally-established*
version differing from the pinned revision's, and in which the **local value
wins**. This entity records two readings of *the same* nixpkgs revision — one by
evaluation, one by file-parsing — and the **evaluated value wins**. Opposite
precedence, different pair of sources. Emitting them under one row would make
"which side won" unrecoverable for a consumer. See C177 below.

### `DegradationReason` (`thiserror` enum, Principle IV)

The closed set required by spec FR-013. Seven variants, each independently
reachable (spec SC-006):

| Variant | Wire form | Provoked when |
|---|---|---|
| `OfflineRequested` | `offline-requested` | `--offline` is set; see below |
| `ToolAbsent` | `tool-absent` | no `nix` on `PATH` |
| `ToolUnusable` | `tool-unusable` | `nix` present but the daemon/store is not usable |
| `IfdRefusalUnverified` | `ifd-refusal-unverified` | the R3 pre-flight could not confirm the refusal is in effect |
| `RevisionUnfetchable` | `revision-unfetchable` | the pinned revision could not be acquired, or is not 40 hex characters |
| `NoEvaluableAttribute` | `no-evaluable-attribute` | no usable attribute path in the flake (the HLS case) |
| `EvaluationFailed` | `evaluation-failed` | `nix` exited non-zero |
| `BudgetExceeded` | `budget-exceeded` | the wall-clock budget expired (research R4: Nix will not do this for us) |

`OfflineRequested` exists because the tier cannot honour `--offline`:
evaluation resolves the pinned revision through `getFlake`, which fetches when
the Nix store lacks it (measured), and Nix's own `--offline` governs
substituters rather than flake inputs. The tier *would* succeed on a warm
store, so refusing costs a real capability — it is refused anyway, because a
promise of "no outbound network calls" kept only when a cache happens to be
warm is not a promise.

`IfdRefusalUnverified` is a *degradation*, not an error, and it is deliberately
distinct from `ToolUnusable`: an operator whose `nix` is too old to honour the
safety control needs a different remedy from one whose daemon is down.

## Emitted metadata — Principle V audit

Per Constitution Principle V and the project's standing rule (memory:
`feedback_native_fields_first`), each annotation below was checked against
native CycloneDX 1.6 / SPDX 2.3 / SPDX 3 constructs **before** proposing a
`waybill:` property.

Proposed rows continue the catalogue at `docs/reference/sbom-format-mapping.md`,
whose highest current row is **C176**. Every row needs a matching extractor in
`parity/extractors/mod.rs` or `every_catalog_row_has_an_extractor` and
`holistic_parity` fail (memory: `feedback_sbom_format_mapping_extractor_gate`) —
so rows and extractors land in the same change, never doc-first.

| Row | Field | Scope | Native carrier considered | Verdict |
|---|---|---|---|---|
| **C177** | `waybill:nix-eval-superseded-version` | per-component | CDX `pedigree.variants` describes a *modified* component, not two readings of an unmodified one. CDX `evidence.identity[].confidence` is numeric, not a superseded value. SPDX 2.3 and SPDX 3 have no equivalent. | **KEEP-NO-NATIVE.** Distinct from C172: opposite precedence and a different pair of sources (see `ResolutionDivergence`). |
| **C178** | `waybill:nix-eval-tier` | document-scope object: `{revision, system, resolved, superseded, evaluated-only, degraded-reason}` | No format has a slot for "which resolution mechanism produced these versions". CDX `metadata.tools[].note` is freeform prose, not machine-actionable. SPDX `creationInfo.creators` names the tool, not its per-invocation mode. | **KEEP-NO-NATIVE.** Deliberately **not** folded into C174 `waybill:nixpkgs-haskell-resolution`, which records the *file-parsing* pass: a consumer reading C174 must not be led to think it describes evaluation, and the two can both be present in one document. Same reasoning C176 gives for staying out of C174. |
| **C179** | `waybill:nix-eval-system` | document-scope string | CDX 1.6 `metadata.lifecycles[].phase` was checked directly and **rejected**: it is a fixed enum already carrying the CISA SBOM *type* (design/source/build/analyzed/deployed/runtime) in this codebase — see `waybill-cli/src/generate/lifecycle_phases.rs:40-67`, where it maps 1:1 to SPDX 3 `software_SbomType`. It cannot carry `x86_64-linux`, and overloading it would corrupt a field the emitter already populates with a different meaning. CDX `component.scope` is unrelated. SPDX 3 has no document-level platform slot. | **KEEP-NO-NATIVE**, on a checked basis. |
| **C180** | `waybill:nix-eval-degraded` | document-scope string, closed set = `DegradationReason` wire forms | Same absence as C173 `waybill:nixpkgs-haskell-degraded` documents for its own pass. | **KEEP-NO-NATIVE.** Deliberately **not** folded into C173, for the reason C173 itself gives for not folding into C158: a single slot cannot say *which* of two independent mechanisms degraded, and both can degrade in one scan. |
| **C181** | `waybill:nix-eval-origin` | per-component string, `evaluated` \| `file-parsed` | As C175 `waybill:nixpkgs-component-origin` records for declared-vs-transitive. No native direct/transitive or provenance-of-version field exists in any of the three. | **KEEP-NO-NATIVE.** Emitted on **every** component the tier touched, including those where the two sources agreed — marking only the divergent ones would make absence ambiguous, indistinguishable from a component the tier never examined. Same argument C175 makes, and the same trap. |

**Explicitly rejected as a native mapping**: putting the evaluated version in
`version` and the file-parsed one in CDX `hashes[]`, `externalReferences[]`, or
SPDX `Package.sourceInfo`. Each would carry a false statement about what the
field means — the failure mode C165 documents at length for `narHash`, where a
native carrier exists, looks right, and would mislead any consumer that trusted
it. Principle IX over Principle V, consistent with that precedent.

## State transitions

```
        flag off ──────────────────────────────► file-parsing only (byte-identical to today)
            │
        flag on
            │
            ▼
   ┌─ nix on PATH? ──── no ──► degrade(ToolAbsent)
   │        yes
   ▼
   ┌─ IFD-refusal pre-flight confirms in effect? ── no ──► degrade(IfdRefusalUnverified)
   │        yes                                                    ▲
   ▼                                                               │
   ┌─ revision acquired within its budget? ── no ──► degrade(RevisionUnfetchable)
   │        yes                                            R3: never skip this gate —
   ▼                                                       an unsupported option is a
   ┌─ attribute path evaluable? ── no ──► degrade(NoEvaluableAttribute)   silent no-op
   │        yes
   ▼
   ┌─ pure eval within wall-clock budget? ── no ──► degrade(BudgetExceeded)
   │        yes                            (R4: nix imposes no bound of its own)
   │         └─ non-zero exit ──────────► degrade(EvaluationFailed)
   ▼
   reconcile: evaluated wins; file-parsed retained (C177); origin marked (C181);
   document record emitted (C178, C179)
```

Every `degrade(...)` edge lands in the same place: file-parsing results are
emitted unchanged, C180 carries the reason, and the scan exits successfully.
