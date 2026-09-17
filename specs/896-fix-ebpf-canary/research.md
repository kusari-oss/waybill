# Phase 0 Research: A trustworthy eBPF canary signal

**Feature**: `896-fix-ebpf-canary` | **Issue**: [#685](https://github.com/kusari-oss/waybill/issues/685)
**Date**: 2026-09-17

Every empirical claim below is traceable to a run log, a file, or a git commit,
per the project's "measure external behaviour before designing around it" rule.
Nothing here is derived from another number.

---

## R1 — The failure is the canary's own environment, and there have been *two* of them

**Decision**: Treat the missing `rust-src` component as the confirmed immediate
cause, and treat "the canary has never once been green" as the framing fact
that governs the rest of the design.

**Evidence** (all from `gh run list --workflow=ebpf-canary.yml`):

| Fact | Value | Source |
|---|---|---|
| Total canary runs ever | 36 | run list, 2026-08-13 → 2026-09-17 |
| Runs concluding `success` | **0** | same |
| First run | 2026-08-13T06:35:17Z, id `31674387553` | same |
| Latest run | 2026-09-17T06:25:09Z, id `35189734662` | same |
| Open report | #685, created 2026-08-13T06:36:09Z | `gh issue list --label canary` |

The streak is not "35 red nights after a green baseline". The canary has
**never produced a green run**. There is therefore no observation anywhere in
its history that the pinned version builds *under the canary's own
environment* — which is precisely why FR-002 exists.

**Two distinct causes inside the one undifferentiated streak**:

- Run 1 (2026-08-13) failed at bpf-linker install:
  `Error: could not find llvm-config in directories specified by environment`
  — the composite action was then on the `cargo install` path, which builds
  bpf-linker from source and needs system LLVM.
- Runs 2–36 (2026-08-14 → 2026-09-17) fail at the eBPF build step:
  `".../nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.lock"
  does not exist, unable to build with the standard library, try:
  rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu`
  (verified on both id `31776715099` (2026-08-14) and id `35189734662`
  (2026-09-17) — the first and last of that era).

**What changed between them**: commit `e1024ab5` (2026-08-13, PR #686) switched
`install-bpf-linker` to `install-method: binary` by default. That is exactly
downstream mitigation **(b)** documented at `docs/development/ebpf-toolchain.md:104`.
The binary path is gated behind `if: inputs.install-method == 'cargo'` on the
toolchain step (`.github/actions/install-bpf-linker/action.yml:57`), so the
`rustup component add rust-src` that lived inside the composite stopped
running. The action's own comment states the assumption it thereby took on:

> *"The binary path doesn't need it (the calling workflow installs its own
> toolchain for the actual eBPF build)."* — `action.yml:54-56`

That assumption is true of the two other callers and false of the canary. The
canary is the only caller that installs **stable only**
(`ebpf-canary.yml:70-73`) and then relies on `cargo +nightly` inside xtask.

**Why this matters to the design**: both causes were canary-environment faults,
both were reported under the title *"[canary] bpf-linker eBPF build
regression"*, and both pointed the reader at `aya-rs/bpf-linker`. A rule based
on *which step failed* would have attributed cause 1 to the canary (install
step) and cause 2 to upstream (build step) — half right, and wrong on the one
that ran 35 times. This is the observation behind FR-003b.

**Alternatives considered**: re-deriving the cause from first principles. Rejected —
spec Assumptions already establish it, and the compiler prints the fix verbatim.

---

## R2 — Why `waybill-ebpf/rust-toolchain.toml` does not rescue this

`waybill-ebpf/rust-toolchain.toml` already declares what is needed:

```toml
[toolchain]
channel = "nightly"
components = ["rust-src"]
```

Measured locally (macOS, this workspace):

```
$ cd waybill-ebpf && rustup show active-toolchain
nightly-aarch64-apple-darwin (overridden by '.../waybill-ebpf/rust-toolchain.toml')
```

So the file *is* honoured — but only when nothing overrides it. `xtask`'s
`build_ebpf` (`xtask/src/main.rs:51-66`) invokes:

```rust
Command::new("cargo").current_dir(dir).args(["+nightly", "build", ...])
```

`+nightly` is rustup's highest-precedence override and bypasses the toolchain
file entirely — channel *and* `components`. The observed CI failure is the
direct confirmation: nightly is present on the runner, `rust-src` is not.

**Measured during implementation — R2 was incomplete.** Two probes in the same
job (run `35259780273`):

| Probe | Where | nightly components |
|---|---|---|
| A | immediately after `rustup toolchain install nightly --profile minimal` | `cargo`, `rust-std`, `rustc` — **no rust-src** |
| B | immediately before the eBPF build | the same three **plus `rust-src`** |

Nothing between them asks for it. The installer is **`Swatinem/rust-cache`**:
it runs cargo in each directory listed under `workspaces:`, and this job lists
`waybill-ebpf`. A rustup-proxied command run *inside* that directory — with no
`+toolchain` override — resolves through `waybill-ebpf/rust-toolchain.toml` and
rustup installs the components it declares.

So the toolchain file does work, but only for commands that resolve through it.
`cargo +nightly`, which is what xtask uses, never does. That is the precise
shape of the original defect, and it means the explicit `components: rust-src`
is belt **and** braces rather than redundant: without it the canary would be
depending on a cache step's incidental side effect.

It also explains why the first `break_env` switch produced a green run. Merely
declining to *request* the component is not a break, because something else
installs it a few steps later. The switch now removes it immediately before the
build, after every installer has run.

**Decision**: fix the canary's environment explicitly, matching the two working
lanes. Do **not** change `xtask` in this feature.

**Alternative considered — drop `+nightly` from `build_ebpf`** so the toolchain
file governs and every build site provisions itself. Genuinely more durable: it
fixes the class rather than the instance. Rejected *for this feature* because
`build_ebpf` is shared by `ci.yml`, `release.yml` and local `scripts/verify-ebpf.sh`,
it cannot be verified on a macOS dev machine (the eBPF target is Linux-only),
and a regression there breaks the release path — a materially larger blast
radius than the canary this feature is about. Worth filing as follow-up.

---

## R3 — The environment divergence, enumerated (FR-010 / SC-002)

Three sites build the same artifact. Comparison is by reading all three files:

| Preparation step | `ci.yml` lint-and-test-ebpf | `release.yml` build-ebpf | `ebpf-canary.yml` canary |
|---|---|---|---|
| apt: `clang llvm libelf-dev pkg-config` | ✅ (+`libssl-dev`, retry-wrapped) | ✅ | ❌ |
| nightly toolchain **with `rust-src`** | ✅ `ci.yml:644-647` | ✅ `release.yml:72-75` | ❌ |
| stable toolchain | ✅ | ❌ | ✅ `ebpf-canary.yml:70-73` |
| `Swatinem/rust-cache` | ✅ | ✅ | ❌ |
| remove `rustup-init` (dispatch race) | ✅ | ✅ | ❌ |
| `install-bpf-linker` composite | ✅ | ✅ | ✅ |
| `cargo run -p xtask -- ebpf` | ✅ | ✅ | ✅ |

The canary is missing **four of six** preparation steps. Only the `rust-src`
row is currently load-bearing (it is what fails), but `rustup-init` removal
guards a known non-deterministic flake documented in both other lanes, and the
apt deps are required by the `cargo` install-method fallback.

**Decision for the divergence check (FR-010)**: a text-level assertion, in the
canary workflow itself, that the canary's toolchain-install step declares the
same components as `ci.yml`'s. Concretely: extract the `components:` value
under each file's nightly `dtolnay/rust-toolchain` step and compare. This is
the same posture as the m115/m117 walker-audit gate (`grep` + `sort` + `diff`,
POSIX-only, zero new tooling) which the project already runs in CI.

**Rejected**: comparing at the job level via a reusable workflow. A shared
setup action would be structurally better, but it changes `ci.yml` and
`release.yml` — outside the spec's "no change to what the project builds"
boundary, and a failure there costs a release rather than a nightly.

---

## R4 — Artifact path and freshness (FR-002a)

`build_ebpf` runs cargo with `current_dir` = `waybill-ebpf`, so the object
lands at:

```
waybill-ebpf/target/bpfel-unknown-none/release/waybill-ebpf
```

Corroborated at three independent sites: `release.yml:133` (upload path,
`if-no-files-found: error`), `waybill-cli/src/trace/loader.rs:248`, and
`scripts/ebpf-integration-test.sh:31`.

**Decision**: assert the file exists and is non-empty after each build, and
guard the spec's "artifact left over from an earlier run" edge case by
removing it *before* the build rather than by timestamp comparison. A
`rm -f` pre-step is deterministic; an mtime check is a race.

`release.yml` already demonstrates the weaker form of this check
(`if-no-files-found: error` on upload). The canary has no equivalent today —
its build step's exit status is its only signal.

---

## R5 — Running the pinned control build (FR-003a)

The control run needs the composite invoked twice in one job. Reading
`action.yml:67-84`, the binary path is safe to re-invoke:

- The idempotency short-circuit only fires when `target != "latest"` **and**
  the installed version already equals the target. A pinned-after-latest
  invocation therefore re-downloads rather than short-circuiting.
- Installation is `tar --zstd -xf … -C "$HOME/.cargo/bin" bpf-linker` over the
  existing file — a byte replacement, no state carried between invocations.
- Passing `version: ''` (the default) makes it read `.github/env/bpf-linker.env`,
  so the control needs no hardcoded version.

**Ordering decision**: run **latest first, pinned second**, and run the control
only when the latest build fails. Rationale: latest-first keeps the subject of
the canary unpolluted by a prior install; conditioning the control on failure
keeps the green path at one build, so the common case does not double in cost.

**Build isolation decision**: `cargo clean -p waybill-ebpf` (or a distinct
`CARGO_TARGET_DIR`) between the two builds. Without it the second build can
reuse objects linked by the first linker version and report a result that
belongs to neither. Distinct `CARGO_TARGET_DIR` is preferred — it is one env
var, leaves both artifacts inspectable for the report, and cannot partially
succeed the way a `clean` can.

**Attribution table** (this is FR-003a/FR-004a/SC-004a in one place):

| latest build | pinned control | Attributed cause | Report title |
|---|---|---|---|
| pass | *not run* | — (green) | close open reports |
| fail | pass | watched component | upstream-regression |
| fail | fail | canary | canary-fault |
| fail before reaching build | *not run* | canary | canary-fault |

Note the third row is the one that flips today's live failure from "upstream"
to "canary", and it does so without reading a single line of error text.

---

## R6 — Two independently-deduped report titles (FR-004a/FR-004b)

Today's dedupe (`ebpf-canary.yml:176-187`) is: list open issues filtered by
labels `canary,ebpf,regression`, then `find(i => i.title === title)` — an
exact title match. Comment if found, create if not. `report-success` closes by
the same match.

**Decision**: keep the mechanism verbatim and parameterise the title. Two
constants:

- `[canary] bpf-linker eBPF build regression` — unchanged, so #685's history
  and any external links survive.
- a second, distinct title for canary-fault failures.

Because the match is exact-title, two titles coexist in the same label set with
no further work — satisfying FR-004b (both open simultaneously) for free.

**One thing the current code gets wrong that this must not inherit**:
`report-success` closes *the* matching issue. With two titles it must close
only the one matching the kind that recovered, or a green run would close an
outstanding upstream-regression report that nothing has actually fixed.

**Rejected**: one title plus a label to distinguish kind. Fails SC-003's
explicit requirement that a maintainer can tell them apart *from the issue
list without opening either* — the title is what that list shows.

---

## R7 — Where "days elapsed" comes from (FR-007/FR-007a)

**Decision**: the `created_at` of the open report for that kind. The dedupe
mechanism already creates exactly one issue per kind at the streak's first
failure and appends thereafter; its creation timestamp *is* the streak start,
with no new state to store.

Verified against the live case: #685 `createdAt` = 2026-08-13T06:36:09Z, and
the first failing run is 2026-08-13T06:35:17Z — 52 seconds apart. The issue
timestamp tracks the streak start to within the duration of one run.

Elapsed days as of today: **35**. The documented window is **30**
(`docs/development/ebpf-toolchain.md:97,103`). So replaying #685's history
through this rule produces an escalated report — SC-006a, satisfied by
construction.

This also discharges FR-007a: `created_at` cannot be stalled by anyone failing
to file upstream. m234's rule is gated on upstream *responsiveness* — `ebpf-canary.yml:166`
reads "if unresponsive within the 30-day fallback window". No upstream issue
was ever filed, so by that reading the clock never started.

**Rejected**: counting comments on the issue. The spec's own edge case rules it
out — a streak spanning a period when the canary did not run gives a comment
count that disagrees with elapsed days, and elapsed days is the governing
measure.

---

## R8 — Streak reset on a change of cause (FR-009/SC-007)

Falls out of R6 + R7 with no extra machinery: a change of attributed cause
means a different title, which means a different issue, which means a
different `created_at`. A canary-fault streak cannot inherit an
upstream-regression streak's age.

The recovery half needs the R6 correction: closing must be per-kind. A green
run closes both kinds (both are, by then, actually resolved — the build
succeeded against latest, which requires the canary to work). A red run of one
kind must not close the other.

---

## R9 — How this gets verified before it ships

The canary is a GitHub Actions workflow; none of it is reachable from
`cargo test`. Verification posture, in decreasing order of strength:

1. **`workflow_dispatch` with an explicit `version` input** — already supported
   (`ebpf-canary.yml:16-21`). Dispatching with the pinned version exercises
   SC-001 directly on a real runner. This is the primary gate.
2. **`dry_run: true`** — already supported (`ebpf-canary.yml:107`), suppresses
   issue writes. Lets the build path be exercised without touching #685.
3. **Deliberate-break test for SC-003** — a dispatch input that skips the
   `rust-src` install reproduces the live failure on demand and must produce a
   canary-fault report. Without this, SC-003 is only testable by waiting for
   the next accident.
4. **Static assertions** — the FR-010 divergence check (R3) runs in-workflow
   and fails the run on drift.

**Constraint inherited from the project**: `public-corpus.yml` and this
workflow both use a shared concurrency group posture; dispatch **one at a
time** and wait for each to finish rather than firing several.

**Decision**: gate merge on (1) a green dispatch against the pinned version and
(2) a dispatch of the deliberate-break path producing a canary-fault report
under the new title. Both are observations on a real runner, not local
simulations.

---

## Open items carried into Phase 1

None. No `NEEDS CLARIFICATION` remains: the spec's four clarifications fixed
the design questions, and R1–R9 fix the mechanical ones against observed
evidence.

## Follow-up worth filing separately (out of scope here)

- Remove `+nightly` from `xtask::build_ebpf` so `rust-toolchain.toml` governs
  every eBPF build site (R2). Fixes the class; needs a Linux verification pass
  across all three lanes.
- Factor the eBPF build environment into one composite action consumed by all
  three lanes, making R3's divergence table structurally impossible (R3).
