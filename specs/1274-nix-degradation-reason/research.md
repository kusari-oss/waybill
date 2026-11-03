# Phase 0 research: the nix-closure tier says why it degraded

**Milestone**: 1274 | **Spec**: [spec.md](./spec.md) | **Date**: 2026-11-03

Every external behaviour this feature depends on was measured. The probe is
committed at `measurements/reproduce-1283.sh`; the findings below that it does
not yet cover are recorded in `measurements/findings.md`.

---

## R1 — the spec's central assumption was wrong, and Phase 0 is why we know

The spec assumed:

> **`salient_stderr`'s rule is the right rule.** It was measured and
> documented in milestone 1034 and is not re-derived here; this feature
> applies it where it is missing. If the rule is wrong, it is wrong in both
> places and that is a separate finding.

**It is wrong in both places.** Measured against the real stderr of a
`callCabal2nix` evaluation (1,382 bytes, 26 lines):

| line | content |
|---|---|
| 0 | `warning: Git tree '…' has uncommitted changes` |
| 1 | `error:` — a bare opener |
| 2–24 | Nix's stack trace: `… in the left operand of the update (//) operator`, file positions, code excerpts |
| **25** | `error: cannot build '…cabal2nix-….drv' during evaluation because the option 'allow-import-from-derivation' is disabled` |

`salient_stderr` takes `position()` — the **first** line matching its error
predicate — then keeps 300 characters forward. That lands on line 1 and fills
the window with the stack trace. The explanatory clause sits at byte **1320 of
1382**, which is **1,155 characters past** the end of the window.

So the rule is right about the half it was built for (skip the leading
warnings) and wrong about the other half: **Nix prints the stack trace first
and the cause last.** Applying it unchanged to the closure path — which is
what FR-007 and FR-008 as written describe — would move the defect rather than
fix it.

This also narrows what milestone 1034 actually achieved. Its doc comment
quotes this exact cabal2nix message as the thing the operator never saw, so
the fix was aimed here; it works when the failure is short enough that the
cause falls inside the window, and does not when Nix emits a trace first.

### Decision: select the **last** matching line, not the first

One word in the rule — `position` → `rposition`. Measured on both cases:

| stderr | first error line | **last error line** |
|---|---|---|
| `callCabal2nix` refusal (26 lines) | classifies **generic** ❌ | classifies **refusal** ✓ |
| `nixConfig` request + missing attribute (9 lines) | generic ✓ | generic ✓ |

`rposition` is strictly better rather than a trade: where there is a single
error line the two are identical, and where there is a trace it picks the
cause. It also leaves FR-005a's negative case correct — the false-positive
stderr's last error line is `attribute '…' missing`, which carries no
disabled-clause, so the classifier declines.

**Alternatives considered:**

- *Widen the window to cover 1,382 bytes.* Rejected: the detail goes into a
  document annotation under FR-002a, and a kilobyte of Nix stack trace per
  degraded scan is not a thing to put in an SBOM. It also does not help —
  without a selection rule the window would have to grow with every new Nix
  message shape.
- *Keep the first error line and append the last.* Rejected as the default
  because it doubles the payload to carry a bare `error:` opener. Where a
  trace genuinely matters it is in the log, which FR-003 keeps.
- *Match the known cause and fall back to the last line.* Rejected: that
  makes the detail's selection depend on the classifier's vocabulary, so a
  cause nobody has catalogued yet would report worse than one that is. The
  selection rule should not know what it is looking for.

## R2 — the classifier's anchor, measured on a real false positive

FR-005a requires both the consequence clause and that it be on an error line.
Phase 0 confirmed the false positive it guards against is real rather than
imagined. A flake whose `nixConfig` requests the setting and then fails for an
unrelated cause emits:

```
warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
error: attribute 'this-attribute-does-not-exist' missing
```

Matching the bare setting name anywhere in stderr therefore reports a missing
attribute as a policy refusal. Real flakes carry such settings — `slack-web`
sets this one, per `eval/invoke.rs`'s own comment — so this is on the path of
ordinary use.

**Decision**: match `because the option 'allow-import-from-derivation' is
disabled` on the selected error line. Two independent conditions, as FR-005a
requires: the clause appears only in the refusal, and the line selection
already excludes warnings.

**Note on interaction with R1**: the two decisions are not independent. With
`position()` the classifier would be fed the bare `error:` opener and could
never match the clause no matter how the anchor were written. R1 has to land
for R2 to be testable at all.

## R3 — the carrier already exists, so no catalog row is added

`waybill:nix-closure-degraded` is catalog row **C190**, with all three
extractors present (`c190_cdx`, `c190_spdx23`, `c190_spdx3` in
`parity/extractors/`) and emission in `cyclonedx/metadata.rs`,
`spdx/annotations.rs` and `spdx/v3_annotations.rs`.

**Decision**: extend C190's value from a bare string to a small object, per
FR-002a. FR-002b's claim holds — extending a row that already has three
extractors adds no row, so FR-014's condition is not triggered.

The documentation row describes the value as a "document-scope string", so
**the catalog row's prose has to change with the shape.** That is the
`mapping_doc_bidirectional` gate's territory and is a task, not a surprise.

## R4 — what the shape change costs

**Decision**: `{"reason": "...", "detail": "..."}`, keys sorted by
`serde_json::Map`'s `BTreeMap` so a golden can pin the string — the same
property `ClosureSummary` and `EnrichmentSummary` rely on.

Blast radius, measured rather than assumed — and it is not where the spec
implied:

**Corpus goldens: zero.** No committed golden under
`tests/fixtures/public_corpus/` contains `nix-closure-degraded`. Only a target
scanned with `ScanMode::NixClosure` can carry it, which is `nix-closure-moat`
alone among the 17, and its evaluation succeeds so it records no degradation.
FR-016's baseline exists to prove that rather than to assert it.

**Integration tests: six assertions.** `tests/nix_config_closure.rs` asserts
C190's value directly in five of its fourteen tests:

```
assert_eq!(c190(&s).as_deref(), Some("several-system-configurations"));
assert_eq!(c190(&s).as_deref(), Some("no-evaluable-attribute"));
assert_eq!(c190(&s).as_deref(), Some("offline-requested"));
assert_eq!(c190(&s).as_deref(), Some("ifd-refusal-unverified"));
assert_eq!(c190(&s), None);   // x2
```

So the real FR-016 population is **6 assertions in one suite**, not goldens.
They must be *corrected* — reading the `reason` field — rather than pinned,
and the two `None` cases still mean "no degradation recorded" and need no
change beyond the helper's return type.

That suite is also where FR-013 is enforced: those four strings are existing
reasons whose wire values must not change. Putting them in the object's
`reason` field satisfies FR-013 by construction, and the corrected assertions
are what prove it.

**A consumer keying on the bare string breaks.** Inside this repository the
only other consumers are the three C190 parity extractors, which read the
value generically and are unaffected. Outside it, the value is published,
which is what makes this a wire change worth stating in the spec rather than
a refactor.

## R5 — nix's wording is a text contract, so it is pinned and re-runnable

Measured with **Determinate Nix 3.20.0** on `aarch64-darwin` against nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`. CI installs nix through
`DeterminateSystems/nix-installer-action@3138316` (v23), so the CI and local
toolchains are the same family; the probe prints the version it ran against so
a mismatch is visible rather than silent.

**Decision**: FR-005b's requirement — a reworded message degrades to the
generic reason — is satisfied structurally by matching a literal clause and
having no fallback guesswork. The probe is committed so the match can be
re-verified when Nix changes, and `measurements/` is where the next person
looks when it does.

**Not done, deliberately**: the wording was not verified across multiple Nix
versions. One version is what this repository builds against, a second would
not establish a range, and the requirement is already "degrade on a rewording"
rather than "match every version". Recorded so the limit is visible.

## R6 — no new surface

**Decision**: no new Cargo dependency, no subprocess beyond the `nix`
invocation the tier already makes, no new CLI flag, no network. The change is
a selection rule, a classifier branch, and a value shape.

FR-011 forbids making the refusal configurable, and FR-010 forbids changing
whether any flake evaluates — so the control fixture (a handwritten flake that
evaluates 1,170 derivations into 304 components) must come out byte-identical,
which is SC-003.
