# Measurements — milestone 1372 (#1366)

**Date**: 2026-11-06
**Host**: aarch64-darwin
**Closure**: `nix-closure-moat` at commit `d069055`, nixpkgs pinned `a799d3e3`
**Pre-change binary**: `main` at `afe3794` (milestones 1366 and 1367 merged)

> **These figures are aarch64-darwin.** The corpus goldens pin x86_64-linux,
> which is a **different closure**. Milestone 1366 measured 74 PyPI candidates
> on one and 75 on the other for exactly this reason. A count from one is
> wrong in membership when quoted against the other.

---

## T003 — the four counts every later assertion is checked against

Read from the capture, not quoted from the spec.

| | count |
| --- | --- |
| derivations classified `unreferenced` | **742** |
| …named by another derivation's `inputs.drvs` | **741** |
| — already emitting, so **relabelled** | **641** |
| — **newly** emitting, so admitted | **100** |
| still `unreferenced` after the change | **1** — `moat`, the closure root |

**The root is the control.** It is the one derivation nothing consumes. If it
stops being `unreferenced`, the signal is admitting on no evidence and every
other figure here is suspect.

Note V1 is already applied in this count: a derivation naming itself in its
own `inputs.drvs` is excluded. The figure is identical with and without that
exclusion on this closure, so V1 is defensive here and needs a constructed
test (T010), not the corpus.

## T002 — document baseline

| | |
| --- | --- |
| components | **942** |
| closure members | **539** |
| edges | **5,131** |

Role histogram, **per component**:

| role | components |
| --- | --- |
| `unreferenced` | **543** |
| `artifact-input` | 235 |
| `build-tooling` | 96 |
| `both` | 43 |

## T004 — the two deltas to watch

| | before |
| --- | --- |
| edges | **5,131** |
| lifecycle scope | `development` 10, `build` 1, unset 931 |

Research R5 and R7 predict both move. They are recorded separately so the
lifecycle delta is not read as the role delta, and so **issue #1367's edge
undershoot is not later diagnosed against a number this feature moved**.

---

## The U1 ambiguity, confirmed in numbers

`/speckit-analyze` flagged that C210's counting unit was undefined and that
`data-model.md` said *"components"* while giving a derivation count. Measured:

```
100 newly-admitted DERIVATIONS  →  45 distinct (name, version) COMPONENTS
```

The two differ by **55**, because build variants share a `(name, version)` and
merge. Shipping `admitted: 100` under the label "components" would have been
wrong by more than half.

The same gap shows up in the baseline above: **742** derivations are
`unreferenced` while **543** components carry that role. Any figure in this
file states its unit for that reason.

**Resolved**: both C210 counts are **derivations**, because the classifier
works in derivations and the role is a per-derivation fact. The catalog row
says so, so a reader comparing `admitted` against the component count knows
they are not meant to match.

---

## The delivered result

| | before | after |
| --- | --- | --- |
| derivations `unreferenced` | 742 | **1** (`moat`, the root) |
| derivations emitted | 1,174 | **1,274** |
| components | 942 | **977** |
| distinct identities gained / lost | — | **+35 / 0** |
| closure members | 539 | **539** |
| edges | 5,131 | **6,627** |
| components with C211 `build-graph` | — | **590** |

**Phase 4 required no production code**, exactly as the plan predicted: the
100 derivations are admitted by `role_alone_emits` as a consequence of the
role change alone, and `emits_as_component` was never edited.

### The six tools (SC-002)

`gzip 1.14`, `gawk 5.4.0`, `gnused 4.9`, `less 692`, `patch 2.8`,
`coreutils 9.11` — all present. Five were absent entirely before;
`coreutils` existed as a different derivation's component and gained a second
role source.

### Role transitions, per component (T035)

| transition | components |
| --- | --- |
| `unreferenced` → `build-tooling` | **543** |
| `artifact-input` → `both` | **3** |

**The three are not an FR-003a violation, and were checked rather than
assumed.** `locale`, `coreutils` and `python3.13-bootstrap-packaging` are each
one component formed from several derivations. One variant was declared an
artifact input; another was `unreferenced` and is now `build-tooling`; the
merge unions the two into `both`. At the **derivation** level — which is what
FR-003a governs — `artifact-input` is unchanged at 298 and `both` at 63, so no
declared role was overridden. This is the third place in this milestone where
the derivation/component distinction changes the answer.

### Two predictions the measurement corrected

**Research R5 was wrong about lifecycle scope.** It predicted 741 role changes
would move scope with them. Measured: the scope histogram is unchanged apart
from the 35 new components landing unset — `development` 10 and `build` 1 both
before and after. Milestone 1277's role→scope derivation is narrower than R5
assumed, so the two numbers are not related the way the research claimed.

**FR-007's correction was conservative rather than necessary.** The
requirement was rewritten during `/speckit-clarify` to permit losing an edge
when a newly admitted component sits on its path, because the projection walks
*through* non-emitting derivations. Measured: **0 edges lost, 1,496 gained**,
and every lost-edge check comes back empty. The rewritten form is still the
right one — it is the checkable property, and the loss is possible in
principle — but the loss does not occur on this closure.

### The edge delta, recorded for #1367

5,131 → **6,627**, purely additive. Against sbomnix's 8,230 the ratio moves
from **1.60× to 1.24×**.

**Issue #1367's edge undershoot must not be diagnosed against the old figure.**
Roughly 1,496 of the gap it describes was this: derivations that were in the
build graph, were not emitted, and therefore could not be edge endpoints.

---

## T039 — the mutation check

Each mutation applied alone, reverted before the next. `--bins m1372` is the
six unit tests; `closure_edges` is the integration suite milestone 1280 owns.

| mutation | `--bins m1372` | `closure_edges` |
| --- | --- | --- |
| the graph signal is a no-op | **3 failed** | **1 failed** |
| the graph applied to **all four** arms, not just `(false,false)` | **1 failed** | **2 failed** |
| the V1 self-reference guard dropped | **1 failed** | ok (V1 is not exercised there) |
| reverted | ok, 6 passed | ok, 15 passed |

**The second mutation is the one that mattered.** It is the only evidence
SC-004a — a declared attribute always wins — is covered rather than vacuously
true, because on this closure no derivation is both declared *and* graph-named
in a way that would distinguish the two. The task said so in advance: *"if the
second mutation leaves everything green, SC-004a is not covered."* It did not.

The third going red only in the unit target is correct and expected: V1 cannot
be reached from the real closure (T003 recorded the figure as identical with
and without the exclusion), which is why it has a constructed test.

## T040 — a scan with no closure is unchanged (SC-006)

`waybill sbom scan --path .` over this repository, which carries no `flake.nix`
— pre-change binary at `afe37941` built in a worktree, post-change binary at
the branch tip, both debug.

| format | sizes | differences outside volatile fields |
| --- | --- | --- |
| CycloneDX | 5,722 components, 5,723 dependency entries | **0** |
| SPDX 2.3 | 5,723 packages, 16,056 relationships | **0** |
| SPDX 3 | — | **0**, with no volatile fields to mask |

Volatile fields masked: `serialNumber`, `timestamp`, `created`,
`documentNamespace`, `annotationDate`. Nothing else was excluded, and the
comparison is structural rather than over bytes so a field appearing on only
one side is reported rather than absorbed into a length difference.

### An unrelated non-determinism, recorded rather than attributed

The **first** pre-change run reported `components[72].supplier.name` as
`crates.io` where every later run of **either** binary reported
`Aya Contributors`. The one-binary control settles the attribution: the
pre-change binary produced both values across its own runs, so this is not
milestone 1372. The cause is not established — a warm registry cache on the
first run is the obvious candidate and the obvious candidate has been wrong
repeatedly in this area — so it is recorded as an observation and nothing more.

## T038 — the residual sbomnix name gap (SC-008)

Same harness, same sbomnix `1.9.0+g95480c2` from the store, same moat pin
`d069055`, same aarch64-darwin host, same flags as every prior run in
`specs/1276-boot-library-scope/measurements/sbomnix-comparison.md`. Confirmation
is **off** (`--no-deps-dev`), as that script has always run it, so the 74
milestone-1366 `pkg:pypi` identities are not present here and the components
they would carry appear as `pkg:generic`.

| | sbomnix | before | **after** |
| --- | --- | --- | --- |
| components | 1,274 | 942 | **977** |
| `dependsOn` edges | 8,230 | 5,131 | **6,627** |
| with a license | 301 | 225 | **234** |
| with a hash | **0** | 136 | **137** |
| with no PURL at all | **106** | 0 | **0** |
| distinct names | 819 | 810 | **842** |
| shared with sbomnix | — | 712 | **741** |
| sbomnix-only, raw | — | 107 | **78** |
| **sbomnix-only, normalised** | — | **32** | **2** |
| waybill-only | — | 98 | **101** |
| GHC boot libraries | 0 of 6 | 6 of 6 | **6 of 6** |

**The residual name gap is 2**: `ps-adv_cmds` and `sysctl-system_cmds`. SC-008
expected near-zero on the grounds that the 23 named gap names were all in this
population; they were.

**waybill now carries more distinct names than sbomnix** — 842 to 819 — for the
first time in this comparison's history.

### The normalisation is now a committed probe, and it was controlled

Milestone 1276 normalised nix's language prefixes by hand to turn 107 into 32.
Comparing *this* run's normalised figure against that one is only meaningful if
both used the same rule, so the rule is now
`measurements/name-gap-probe.py`, and it was run against the **pre-change**
document first:

```
sbomnix names 819 | waybill names 810 | shared 712
sbomnix-only raw 107 | sbomnix-only normalised 32 | waybill-only 98
```

Every figure reproduces milestone 1276's hand-normalised table exactly. Only
then was the same probe pointed at the post-change document. Without that
control the 32 → 2 claim would be comparing two different normalisations and
would mean nothing.

## T041 / T044 — the gate, and the goldens that did not move

```
GATE_NAME=prepr   GATE_EXIT=0   GATE_RESULT=pass
positive signal: 1
targets ok: 371   non-ok: 0   failure blocks: 0
totals: 7451 passed, 0 failed
local goldens moved: 0
```

**Zero local goldens moved**, which is research R9's prediction and the inverse
of milestone 1367, where 34 churned. It is also the stated failure signal: a
local golden moving would have meant C210 or C211 is emitted unconditionally —
a bug rather than a refresh. None did.

---

## Open — not yet measured

| item | task |
| --- | --- |
| corpus goldens | T042, T043 |
