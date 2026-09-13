# Research: A scan of this repository must not depend on network reachability

Feature: `843-fixture-network-isolation` · Spec: [spec.md](./spec.md) · Issue #843

Every figure below is a median of three unless stated. That is not
boilerplate: this feature exists because the quantity under study is
unstable, and two wrong attributions were already made during
clarification by comparing single samples. See spec §Clarifications.

---

## R1 — Where the cost is

**Decision**: The target is Go module proxy access on behalf of fixture
modules. It is ~78% of the floor.

| configuration | median |
|---|---|
| fully `--offline` | 0.50s |
| network on, enrichment off | **5.26s** |
| + Go module proxy refused | **1.17s** |
| + `GOTOOLCHAIN=local` | 1.17s (no change) |
| + `GOSUMDB=off` | 1.17s (no change) |

- Attributable to the proxy: **4.09s**
- Residual above the offline base: **0.67s**

**Rejected**: cargo registry access. `cargo metadata` is already invoked
with `--offline` and already bounded by a timeout
(`cargo.rs:146-153`); disabling it changes nothing measurable. An
earlier claim that cargo was 77% of the cost came from a single 22.00s
outlier and is retracted in the spec.

---

## R2 — What the residual 0.67s is (FR-001b)

**Decision**: Subprocess volume, not any one call. Not addressed by this
feature; recorded so it is not mistaken for network next time.

Per-invocation arithmetic did not explain it — one `go mod graph` with
the proxy refused takes 5ms, so 20 invocations is 0.11s against a 0.67s
residual. Shimming the `go` binary (the technique the m775 note
prescribes for exactly this mismatch) found the scan makes **80**
invocations, not 20:

| subcommand | count |
|---|---|
| `go version` | **27** |
| `go mod graph` | 26 |
| `go list all` | 17 |
| `go mod why` | 10 |

**`go version` 27 times is a capability probe re-run per workspace.**
It is pure waste and independent of this feature — a separate issue.
(The shim's absolute timings are inflated by its own overhead; the
counts are the sound part.)

---

## R3 — How to make a fixture network-free

**Decision**: A local `replace` directive. Both outcomes are available
and both cost 0.01s:

| shape | result | cost |
|---|---|---|
| `replace => ../real-module` | resolves fully; `go mod graph` prints the graph | 0.01s |
| `replace => ../does-not-exist` | fails locally: `no such file or directory` | 0.01s |
| no replace, `example.com/...` | reaches the proxy | 1.40s |
| no replace, `*.invalid` | reaches the proxy anyway | 1.31s |

**Rejected**: renaming fixture modules to a reserved-for-testing domain,
which issue #843 proposed as "probably cheapest". It buys ~6% because
the toolchain consults the proxy before it ever contacts the module's
own host. The host name is not the cost.

**Rejected as a primary fix**: setting `GOPROXY=off` in harness
environments. It works (R1) but only where someone remembers to set it;
a contributor scanning by hand gets no benefit, and SC-002 would stay
false for them.

---

## R4 — Which fixtures are deliberate, which incidental (FR-005)

**Decision**: On the evidence, **none** of the Go fixtures' unresolvability
is the subject of a test. All are incidental.

Only three test files reference these fixture paths at all:

| file | what it asserts |
|---|---|
| `waybill-cli/tests/mod_why_scaling.rs` | fixture readability; concurrent-workspace scan succeeds |
| `waybill-cli/tests/goroot_skip.rs` | GOROOT stdlib is not emitted as a main module |
| `waybill-cli/tests/pants_go_reader.rs` | Pants target annotations |

None asserts on unresolved, fallback or degraded-coverage behaviour.

**This was nearly got wrong.** A first pass grepping for fixture *names*
returned ~25 files, because `workspace_mode` is also a type name in the
codebase. Searching by fixture *path* returns three. The broad grep
would have made US2 look like a large constraint; it is currently an
empty one.

**US2 is not thereby redundant.** It states the rule that keeps a future
deliberately-unresolvable fixture from being "fixed" into resolving.
The set being empty today is a fact about today.

---

## R5 — Golden files, and why they are not at risk

**Decision**: Golden tests are unaffected, because they already run
offline.

The committed CycloneDX golden for the Go fixtures carries:

```
waybill:go-transitive-coverage        = unknown
waybill:go-transitive-coverage-reason = offline-mode: transitive edges
                                        from proxy fetches unavailable
```

`offline-mode` is only set when the scan ran with `--offline`. So the
golden suite never reaches the network, and the network cost this
feature targets is paid by **scanning the tree** — benchmarks, corpus
harnesses, and contributors running a scan by hand — not by
`cargo test`.

**Consequence for approach**: a `replace` pointing at a *real* local
module would let resolution succeed even offline, changing
`go-transitive-coverage` away from `unknown` and churning goldens. A
`replace` pointing at a *missing* path keeps the failure and the
annotation while removing the network attempt. The second is the
zero-churn option and should be preferred wherever a golden covers the
fixture.

---

## R6 — Detecting regressions (FR-008)

**Decision**: Deferred to planning with a stated preference, not
resolved here — the mechanism is cheap to change and the choice does
not affect the rest of the design.

Preference: assert it where the property is, in a test that scans the
fixture tree and fails if any Go module resolution leaves the machine.
The `go`-shim technique from R2 is a working proof that invocations can
be observed; a narrower form (asserting the fixture tree contains no
`require` without a matching `replace`) is cheaper and needs no
subprocess interception.

The walker-audit gate at `waybill-cli/src/scan_fs/walk.audit-allowlist.txt` is the in-repo
precedent for "a grep that fails CI when a pattern reappears", and is
the shape to copy.
