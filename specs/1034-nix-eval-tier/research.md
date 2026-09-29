# Phase 0 Research: Opt-in `nix eval` resolution tier

**Feature**: `1034-nix-eval-tier` (issue **#971 part A**)
**Date**: 2026-09-28
**Host for all probes**: Determinate Nix 3.20.0 (nix 2.34.6), `aarch64-darwin`

## How to read this document

The spec binds this phase with a research constraint, because two prior
documented decisions about Nix in this project were wrong and both cost
defects: m926 §R1 cited Constitution Principle I to avoid `nix` entirely, and
m143 §R7 declared `cabal.project` a presence-only signal.

Accordingly every claim below is labelled:

- **MEASURED** — a probe was run; the command and its output are shown.
- **REASONED** — a conclusion drawn from measured facts. Not itself a claim
  about how `nix` behaves.
- **UNMEASURED** — flagged, with what would settle it. Nothing in this document
  depends on one of these.

Three findings below **changed the design** relative to the spec's assumptions.
They are marked ⚠.

---

## R1 — Constitution Principle I does not bar invoking `nix`

**Decision**: Invoking `nix` as a subprocess is permitted. m926 §R1's contrary
conclusion is retired.

**MEASURED** (reading the constitution at `.specify/memory/constitution.md`,
v3.0.0): Principle I governs two things — the language waybill's *own* code is
written in ("This half of the principle is absolute"), and the **linkage** of
third-party dependencies ("Every such dependency MUST link statically",
"released binaries MUST NOT acquire a dynamic-link dependency beyond the
target's platform baseline"). Strict Boundary 3 restates it as "No first-party
C, no dynamic linkage against host C libraries."

**REASONED**: Spawning a process links nothing and authors no C. The
constitution's operative concern — that `waybill` stays a single self-contained
binary that runs wherever it is copied — is untouched by a subprocess that the
operator opted into and whose absence degrades.

**Precedent** (MEASURED, by reading the tree): waybill already shells out to
`git` (m053, m090), `go` (m055, m112, m173), `./gradlew` (m235), `helm` (m203),
`docker`/`podman` (m195, m206) and `spdx3-validate` (m078). The project's
operative reading of Principle I has never included subprocess invocation.

**Alternatives considered**: reimplementing Nix evaluation in Rust — rejected;
it reproduces the exact failure mode this feature exists to fix, which is that a
reconstruction of what Nix computes is not what Nix computes.

---

## R2 ⚠ — The tier must run in **pure** mode, and that forces the system to be an explicit parameter

**Decision**: Evaluate without `--impure`, and pass the target system as a
literal. Do **not** copy the existing oracle's invocation.

**MEASURED**: a pinned 40-character revision evaluates in pure mode:

```
$ nix eval --json --option allow-import-from-derivation false \
    --expr 'let pkgs = (builtins.getFlake "github:NixOS/nixpkgs/cbb5cf35...").legacyPackages.aarch64-darwin;
            in pkgs.haskellPackages.aeson.version'
"2.2.4.1"
        0.64s total (warm store)
```

**MEASURED**: `builtins.currentSystem` is **not available** in pure mode:

```
$ nix eval --raw --expr 'builtins.currentSystem'
error: attribute 'currentSystem' missing
$ nix eval --raw --expr 'builtins.currentSystem' --impure
aarch64-darwin
```

**REASONED, and this is the design consequence**: spec FR-014 (the system is an
explicit parameter) is not merely a nicety for reproducibility — it is a
*precondition* for FR-009 (pure mode). An evaluation that asks Nix what system
it is on has already left pure mode. The host system must therefore be
determined outside the evaluation that touches repository-controlled
expressions.

⚠ **Changes the design**: the existing oracle at `xtask/src/nix_oracle/mod.rs`
passes `--impure` (lines 172, 187) and does not disable IFD. It is a
developer-run review tool against known targets, which is fine for what it is,
but it **must not be used as the template** for the production tier. The plan
must not simply lift its invocation.

---

## R3 ⚠ — An unsupported `nix` option is a *silent* no-op

**Decision**: The tier MUST verify the import-from-derivation refusal is in
effect *before* evaluating, and MUST degrade if it cannot confirm it. Passing
the option is not sufficient evidence that it applied.

**MEASURED**: nix warns and continues, with exit code 0, when given a setting it
does not know:

```
$ nix eval --expr '1' --impure --option definitely-not-a-real-option true
warning: unknown setting 'definitely-not-a-real-option'
1
$ echo $?
0
```

⚠ **Changes the design**: a `nix` old enough not to support
`allow-import-from-derivation` would accept the flag, ignore it, and evaluate
with IFD **enabled** — while every observable signal (exit code, output) looks
exactly like success. This is the failure mode CLAUDE.md names: *a check that
did not run looks identical to a check that passed.* The spec's FR-008 would be
silently unenforced.

**MEASURED** — a pre-flight check that actually verifies it:

```
$ nix config show --option allow-import-from-derivation false | grep allow-import
allow-import-from-derivation = false          # supported: override is reflected
$ nix config show --option definitely-not-a-real-option false | grep definitely
warning: unknown setting 'definitely-not-a-real-option'   # unsupported: never appears as a setting
```

**REASONED**: the tier should run this check once per scan and require the
literal `allow-import-from-derivation = false` in the output before it evaluates
anything. This also **retires spec Assumption A-6** (the minimum `nix` version
honouring the option): capability detection makes the version floor a
non-question, and is strictly better than a version comparison, which would
break on forks and vendor builds. Determinate Nix 3.20.0 reports itself as
`nix 2.34.6` — a version-string parser would already have two numbering schemes
to reconcile.

**Alternatives considered**: (a) parse `nix --version` and gate on a floor —
rejected per the forks/vendor-builds point above; (b) trust the option —
rejected, it is the silent-failure mode; (c) after evaluating, check whether
anything was built — rejected, that detects the breach after it has happened.

---

## R4 ⚠ — `nix` bounds recursion depth but **not** evaluation time

**Decision**: waybill must impose the wall-clock bound itself (spec FR-012).
Nix will not do it.

**MEASURED** — recursion *is* bounded, and cheaply:

```
$ nix eval --expr 'let f = x: f (x + 1); in f 0' --impure
error: stack overflow; max-call-depth exceeded          # rc=1, after 1s
$ nix config show | grep -E 'max-call-depth|^timeout'
max-call-depth = 10000
timeout = 0
```

**MEASURED** — shallow, non-recursive evaluation is **not** bounded. A
`foldl'` over a 40 000-element list, each step folding another 40 000-element
list (1.6 × 10⁹ additions, constant depth):

```
  t=5s alive … t=45s alive
RESULT: STILL RUNNING at 45s -> nix imposed NO time bound; killed externally
```

**REASONED**: `max-call-depth` catches runaway *recursion* only. `timeout = 0`
is the **build** timeout, not an evaluation timeout, and this tier never builds.
A hostile or merely heavy flake can therefore occupy the evaluator indefinitely,
so FR-012's bound must be waybill's own — a subprocess wall-clock budget, the
pattern already at `golang/go_mod_graph.rs:81` and `golang/mod_why.rs:240`.

⚠ **Changes the design**: spec Assumption A-7 asked whether `nix` bounds
evaluation. It does not. FR-012 is therefore load-bearing rather than defensive,
and the plan must treat the budget as a correctness requirement, not a nicety.

**Default value**: still to be set by measurement — see R5.

---

## R5 — Budget the fetch separately from the evaluation

**Decision**: The FR-012 wall-clock bound covers **evaluation**. Acquiring the
pinned revision is a separate concern with a separate bound.

**MEASURED**: nixpkgs source occupies **300–335 MB per revision** in the Nix
store (`du -sh` over five such store paths). The current file-parsing path
fetches **16 MB per revision** (`hackage-packages.nix` 16 MB, plus six
`configuration-*.nix` files totalling ~150 KB).

**MEASURED** (earlier, carried from the spec): evaluation of 423 package names
at a pinned revision costs 0.5 s warm / 7.7 s cold-eval-cache. **MEASURED**
today: a single-attribute pure evaluation with IFD refused, warm store, 0.64 s.

**REASONED**: a single budget spanning both would make the timeout a bandwidth
test — the same repository would pass on a warm machine and fail on a cold one,
for reasons having nothing to do with the flake. Two budgets, two reason codes
(FR-013 already distinguishes "pinned revision unfetchable" from "evaluation
exceeded its bound").

**UNMEASURED**: the wall-clock cost of acquiring a revision on a store that has
never seen it. Not measured here because doing so locally would require garbage
-collecting nixpkgs out of the operator's store, which is destructive to their
machine for a number that a clean CI runner can produce safely. **Task T-R5**
below measures it on a fresh runner.

**UNMEASURED**: the evaluation cost of a *project's own* flake (as opposed to
nixpkgs attributes). Every timing above evaluates nixpkgs. **Task T-R6**.

---

## R6 — Degradation aligns with Principle XI/XII, not with Principle III

**Decision**: Degrade on every failure; do not fail the scan.

**MEASURED** (reading the constitution): Principle III ("Fail Closed") is scoped
to the eBPF trace path by its own text — *"If the eBPF trace fails to attach,
loses events, or observes zero dependency activity…"*. Principle XI states the
opposite requirement for enrichment: *"If an enrichment source is unavailable,
the SBOM MUST still be emitted with the enrichment fields omitted and a
transparency annotation (Principle X) noting the gap."* Principle XII constraint
3 repeats it: *"External source unavailability MUST NOT prevent SBOM
generation."*

**REASONED**: this tier is enrichment of the `sbom scan` path, not the trace
path. XI and XII govern. The spec's US2 is therefore constitutionally required
rather than a convenience, and FR-013's reason codes are the Principle X
transparency annotation that XI explicitly demands alongside the degradation.

---

## R7 — No new dependencies

**Decision**: Zero new Cargo dependencies at any layer.

**MEASURED** (reading the tree): the subprocess-with-wall-clock-budget pattern
already exists at `waybill-cli/src/scan_fs/package_db/golang/go_mod_graph.rs:81`
(`std::process::Command` + `std::thread` + `std::sync::mpsc::recv_timeout`) and
is reused at `golang/mod_why.rs:240` and by the m203 helm renderer. `serde_json`
parses `nix eval --json` output. `clap` carries the flag. `tracing` carries the
diagnostics. The nix module already exists at
`waybill-cli/src/scan_fs/package_db/nix/` with revision handling in
`lockfile.rs` and a per-revision cache in `haskell_packages/cache.rs`.

**External runtime dependency**: the `nix` binary, opt-in, absent by default,
degrading when missing. Same posture as `helm` for `--helm-render` (m203).

---

## R8 — Open design question for the plan, not for the operator

**The flake attribute path.** MEASURED (carried from the spec):
haskell-language-server's flake exposes no `default` package output — only
`docs` and devShells. So there is no attribute path that can be assumed.
Candidate strategies (to be decided in Phase 1 from the corpus, not asserted
here): enumerate `packages.<system>` and take all; read `flake.lock` for the
nixpkgs revision and evaluate *that* rather than the project flake (what the
current file-parsing path effectively does, and what R2's measurement used);
or try a short ordered list of attribute paths and degrade if none evaluates.

The third is the shape used by `--gradle-resolve`'s configuration list (m235).
No recommendation is recorded here because no probe has been run against a
project flake yet — see Task T-R6.

---

## Research tasks carried into Phase 1 / implementation

| ID | What | Why it cannot be answered here |
|----|------|-------------------------------|
| **T-R5** | Wall-clock and byte cost of acquiring a pinned nixpkgs revision on a store that has never held it. Run on a clean CI runner; record alongside this file. | Measuring locally requires GC-ing the operator's store — destructive |
| **T-R6** | Evaluation cost and attribute-path behaviour of a *project's own* flake, on the two corpus Haskell targets. | No probe run yet; R8's decision depends on it |
| **T-R7** | Whether the IFD pre-flight (R3) holds for `nix` builds other than Determinate 3.20.0 — at minimum the `nix` on the CI runner image. | Only one nix build available locally |

Each must produce a committed probe, per CLAUDE.md: *"Keep the probe. Commit it
next to the spec so the finding is reproducible."*

---

## Summary of changes to the spec's assumptions

| Spec assumption | Outcome |
|---|---|
| **A-6** — minimum `nix` version honouring `allow-import-from-derivation` is unknown and must be probed | **Retired.** R3 replaces a version floor with a capability pre-flight, which is strictly better and does not need the floor. |
| **A-7** — whether `nix` bounds evaluation time or memory is unknown | **Answered: it does not.** R4. `max-call-depth` bounds recursion only; `timeout = 0` is the build timeout. FR-012 is load-bearing. |
| Cold-store cost unknown | **Still unknown**, deliberately. R5, Task T-R5. Nothing in the design depends on the number, because fetch and evaluation are budgeted separately. |
