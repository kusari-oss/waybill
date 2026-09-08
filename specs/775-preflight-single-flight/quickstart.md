# Quickstart — m775 single-flight preflight + go.work directive tolerance

**Audience**: reviewer or implementer verifying m775 acceptance locally after `/speckit.implement`.

**Duration**: ~20 min (build + end-to-end verification).

---

## Prerequisites

- Rust stable toolchain (pinned via `rust-toolchain.toml`).
- A Go toolchain on `PATH` — required for the classifier to run at all. Verify with `go version`.
- The Kubernetes reference fixture:
  `git clone --depth 1 https://github.com/kusari-sandbox/test-kubernetes.git /tmp/test-kubernetes`
  (39 `go.mod` files, one 34-member `go.work` scope, 5 loose workspaces.)
- Optional: `jq` for the byte-identity masking steps.

---

## Step 0 — Build

```sh
cargo build -p waybill --release
```

---

## Step 1 — Verify SC-001 (wall time)

```sh
time ./target/release/waybill --offline sbom scan \
  --path /tmp/test-kubernetes --no-deep-hash \
  --format cyclonedx-json --output /tmp/m775.cdx.json
```

Expected: **≤ 21s** on macOS aarch64 with warm caches (26.3s pre-milestone; scratch verification measured 18.73s).

Run it twice and take the better result — the first run may pay Go build-cache costs. Pre-milestone measurements on the same host were 26.29s / 28.83s.

**If the target is missed**: check Step 2's counter first. A count above 6 means single-flight is not engaging; a count at or below 6 with slow wall time means the remaining cost has moved elsewhere — re-run the per-phase decomposition in `docs/development/perf-methodology.md` rather than assuming this milestone regressed.

---

## Step 2 — Verify SC-003 (preflight invocation count) — the load-bearing check

```sh
RUST_LOG=info ./target/release/waybill --offline sbom scan \
  --path /tmp/test-kubernetes --no-deep-hash \
  --format cyclonedx-json --output /dev/null 2>/tmp/m775.log

grep "go-mod-why classification" /tmp/m775.log
```

Expected: the summary line reports a preflight-invocation count of **6 or fewer** (one for the `go.work` scope, one per loose workspace). Pre-milestone this was 22, though pre-milestone the field did not exist — it is introduced by this milestone (FR-015).

Every pre-existing field on that line (`analyzed`, `prod`, `test`, `not_needed`, `unresolved`, `unknown_marked`, `workspace_modules`, `skipped`, `elapsed_ms`) must still be present with unchanged meaning (FR-010).

**Independent cross-check** (optional — this is the technique that originally found the defect): shim the `go` binary to log invocations, put the shim first on `PATH`, run the scan, and count `go list all` lines. The shim count must match the reported counter. This validates that the counter counts real spawns rather than requests.

---

## Step 3 — Verify SC-004 (preflight subprocess wall time)

Using the shim from Step 2's cross-check, sum the wall time of `go list all` invocations. Expected: **≥ 90% reduction** from the 198.1s pre-milestone baseline (scratch measured 7.3s).

Note: summed `go mod why` wall may *increase* even as scan wall drops sharply — the scratch run measured 68.5s → 84.8s while total wall fell 29%. Summed wall across concurrent processes is not a cost metric and is not a regression signal. Scan wall time is the metric of record.

---

## Step 4 — Verify SC-002 (byte-identity)

```sh
./scripts/pre-pr.sh
```

Expected: zero lint errors, and every suite reporting all tests passed with none failed. Byte-identity is enforced by `cdx_regression`, `spdx_regression`, `spdx3_regression`, `golang_*`, `scan_go_*`, and the m669 corpus goldens.

**Expected golden churn**: US2 legitimately changes one annotation value — `waybill:go-workspace-mode` — on fixtures whose `go.work` contains `toolchain` or `godebug`. The k8s corpus golden is the known instance, moving from `malformed: unknown-directive` to the detected form.

Confirm the diff is confined to that annotation. Per memory `feedback_verify_golden_churn_normalized`, mask content-addressed IDs (`rel-`, `anno-`) and `LC_ALL=C sort` before diffing — otherwise SPDX-3 array reordering will fake semantic hits. **Any component, relationship, or other-annotation change is a defect, not expected churn.**

---

## Step 5 — Verify SC-008 (go.work directive tolerance)

```sh
jq -r '.metadata.properties[]? | select(.name | test("go-workspace-mode")) | "\(.name) = \(.value)"' /tmp/m775.cdx.json
```

Expected: the workspace as **detected with its member count**. Pre-milestone this read `malformed: unknown-directive` because k8s's `go.work` carries `godebug default=go1.26`.

Cross-check self-consistency (Contract 7): the same document's `waybill:workspaces-detected` property lists all 38 workspaces. Pre-milestone the two contradicted each other — one claimed the file was malformed while the other enumerated workspaces parsed from it.

---

## Step 6 — Verify SC-005 (determinism) and SC-009 (single-workspace)

Determinism — two independent scans, masked diff must be empty:

```sh
for i in 1 2; do ./target/release/waybill --offline sbom scan --path /tmp/test-kubernetes \
  --no-deep-hash --format cyclonedx-json --output /tmp/run-$i.json; done
for i in 1 2; do jq -S 'del(.serialNumber) | del(.metadata.timestamp)' /tmp/run-$i.json > /tmp/run-$i.masked; done
diff /tmp/run-1.masked /tmp/run-2.masked && echo "deterministic"
```

Single-workspace overhead — scan a single-`go.mod` fixture and confirm wall time is within ±3% of the pre-milestone binary. With one workspace there is no contention, so the single-flight cell is claimed once and never contended; any measurable regression means the cell is being created or locked on a path that should have taken the cache fast path.

---

## Step 7 — Run the new unit tests

```sh
cargo +stable test --workspace mod_why
cargo +stable test --workspace gowork
```

Coverage:
- Single-flight: N threads, one scope ⇒ work function invoked exactly once (Contract 1).
- Failure propagation: every waiter observes the claimant's failure outcome (Contract 2).
- Cross-scope concurrency: two scopes enter their preflights simultaneously via a barrier (Contract 3) — a serializing implementation deadlocks and fails by timeout.
- Counter accuracy: reported count equals actual spawns (Contract 5).
- Directive tolerance: `godebug` and `toolchain` accepted (Contracts 2, 3).
- Anti-over-correction: genuinely unknown tokens still yield `unknown-directive` (Contract 4).
- Parser agreement: both parsers agree across a directive corpus (Contract 1 of the vocabulary contract).

---

## Troubleshooting

**Counter reports more than 6.** Single-flight is not engaging. Most likely the cache-miss path creates a *new* cell per caller instead of cloning a shared one — check that the cell is fetched-or-inserted under the cache mutex and cloned out, rather than constructed locally.

**Two-scope concurrency test times out.** Something serializes across scopes — typically the cache mutex being held across the preflight, or a single global lock. FR-003/FR-004 forbid both.

**Counter reports 1 on a fixture with loose workspaces.** Loose workspaces must each spawn their own preflight (FR-006); a count of 1 suggests they are incorrectly sharing a scope.

**Golden diff touches components or relationships.** Not expected churn. US1 changes no emitted content and US2 changes exactly one annotation value. Investigate before regenerating.

**Wall time unchanged despite a correct counter.** The preflight was not the dominant cost on your host or fixture. Re-decompose per `docs/development/perf-methodology.md` — do not assume the milestone failed. The stampede's magnitude scales with worker-pool size, so low-core hosts see proportionally less improvement.

---

## Rollback triggers

Roll back if, with the counter correctly reporting ≤ 6:

1. Scan wall time does not improve materially on the reference fixture and host, **and** re-decomposition shows the preflight was never the dominant cost there. (This would mean the Step 3 verification was host-specific — treat it the way m772 and m773 were treated.)
2. Any golden diff extends beyond the single US2 annotation value.
3. Any concurrency test proves flaky under repeated runs — a coordination fix that is itself racy is worse than the stampede it replaces.

Rollback process follows m772/m773: revert the PR, record the incident in `docs/development/perf-methodology.md`, and update memory `feedback_perf_spec_needs_per_phase_decomposition`.
