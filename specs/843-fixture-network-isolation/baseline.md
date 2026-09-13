# Baseline — feature 843

Measured 2026-09-12 on the machine doing the work, against a pristine
`git archive HEAD` export at `/tmp/wb_base`. Every figure is a **median
of three**, driven from a Python script rather than a shell loop.

Both of those are requirements of this feature rather than habits:
single-sample comparison against this floor produced two wrong
attributions during clarification and three during milestone 839, and
shell word-splitting silently returned empty results three times during
research.

---

## T001 — Pre-change floor

Scan with enrichment disabled (`--no-deps-dev --no-clearly-defined`).

| arm | samples | median |
|---|---|---|
| fully `--offline` | 1.41 / 0.50 / 0.51 | **0.51s** |
| network on | 22.30 / 5.38 / 5.39 | **5.39s** |
| + Go module proxy refused | 1.17 / 1.16 / 1.15 | **1.16s** |
| + cargo registry refused | 5.16 / 5.56 / 5.56 | **5.56s** |

- **Go proxy attributable: 4.23s** — 78% of the floor.
- **cargo attributable: −0.17s**, i.e. nothing. FR-001's second clause
  is already satisfied: `waybill-cli/src/scan_fs/package_db/cargo.rs:146-153`
  invokes `cargo metadata` with `--offline` under a timeout. Verified
  rather than assumed, per FR-001a-pre.
- Residual above the offline base: **0.65s** (FR-001b).

**The instability is visible in the samples themselves.** The network
arm's first reading was **22.30s** against a 5.39s median — a 4×
outlier, in the very run taken to establish the baseline. A
single-sample A/B taken at that moment would have produced another
wrong attribution. This is the defect, captured in its own measurement.

## T002 — `go` subprocess inventory

Shimmed the binary on `PATH` and logged every invocation. Counts only —
research R2 established that adding timing to the shim inflates them.

| subcommand | count |
|---|---|
| `go version` | **27** |
| `go mod graph` | 26 |
| `go mod why` | 17 |
| `go list all` | 17 |
| **total** | **87** |

Two observations:

- **27 `go version` calls** is a capability probe re-run per workspace.
  It is unrelated to the network and unrelated to this feature; filed
  separately per T018.
- 87 invocations for a tree containing 27 `go.mod` files. The residual
  0.65s above the offline base is this volume, not network — which is
  what FR-001b asks to be confirmed rather than assumed.

**Denominator for SC-003**: the quantity that must reach zero is
*failed resolutions that leave the machine*, not total `go` invocations.
Most of these 87 are local and will be unaffected.

---

# T006–T009 — the planned technique does not work

Attempted, measured, reverted. Recorded because the finding is the
useful output of the attempt.

## What was tried

A local `replace` directive on every unreplaced `require`, pointing at a
sibling module where one existed and at a missing local path otherwise
(research R3 measured both at 0.01s).

## What it achieved, and what it broke

| state | floor | tests |
|---|---|---|
| before | 5.28s | green |
| all 20 manifests replaced | **1.79s** (2.9×) | **3 failures** |
| 15 replaced, `golden_inputs/golang` reverted | 3.14s (1.7×) | **8 failures** |

Reverted to green: 298 suites, 0 failures.

## Why it cannot work as planned

**waybill's own `go.mod` parser honours `replace`.**
`waybill-cli/src/scan_fs/package_db/golang/legacy.rs:7` — "`replace` /
`exclude` directives that rewrite or drop" modules. So adding one does
not merely change how the *toolchain* resolves; it changes what
**waybill emits**. A module replaced by a local path stops being a
third-party component.

Every failure was a test asserting on a third-party Go component that
had stopped being emitted:

```
root should point at v0.40.0; got []
root's own v0.40.0 should survive; got {}
```

Research R4 concluded no fixture's unresolvability is "the subject of a
test", and that was true as written — none asserts *that resolution
fails*. But several assert on **what the failure produces**: the
`go.sum` fallback tier emitting a component at a particular version.
Changing how resolution fails changes that, and the audit was not
looking for it.

## Why the remaining network is not the toolchain

Even with every manifest replaced, 0.61s of network remained. It is not
`go`: it is **waybill's own proxy fetcher**, the m055 ladder — "`go mod
graph` failed; falling through to cache walk + proxy fetch". Making
`go mod graph` fail *faster* still lands in a tier that fetches `.mod`
files over HTTP. That tier is production behaviour working as designed,
and eliminating it means either making the modules genuinely resolve
(which is what breaks the tests) or changing production code.

## Techniques ruled out, with evidence

| technique | result |
|---|---|
| rename to an unroutable host | ~6% — the proxy is consulted before the host |
| `replace` to a sibling or missing path | works for `go`, breaks waybill's emission |
| committed `vendor/` directory | `go mod graph` still reaches the network; `go list -m all` fails outright in vendor mode |
| `GOPROXY=off` in the environment | works (5.26s → 1.16s) but only protects whoever sets it |

The `vendor/` reading initially looked like 0.01s and was a **cache
artifact** — the third single-sample mistake in a feature about
single-sample mistakes. Measured cold, it does not work.

## What survives

Phases 1 and 2: the baseline, the subprocess inventory, and
`docs/development/go-fixture-inventory.md`. All still accurate and all
independently useful — the inventory in particular records three
distinct wrong ways to find a fixture's consumers.
