# Quickstart: verifying the eBPF canary

**Feature**: `896-fix-ebpf-canary` (#685) | **Date**: 2026-09-17

None of this workflow is reachable from `cargo test`. Verification is by
dispatching the workflow on a real runner and reading what it produced.

**Dispatch one at a time and wait for each to conclude** — the repository's
canary workflows share a concurrency posture, and overlapping dispatches will
queue or cancel each other.

---

## 0. See the defect before changing anything

```bash
gh run list --workflow=ebpf-canary.yml --limit 40 \
  --json conclusion,createdAt --jq '.[] | "\(.createdAt[:10]) \(.conclusion)"'
```

Expect 36 rows, every one `failure`, back to 2026-08-13. Then:

```bash
gh run view <latest-id> --log-failed | grep -B2 'rustup component add'
```

Expect the compiler printing the fix verbatim:

```
error: ".../nightly-x86_64-unknown-linux-gnu/lib/rustlib/src/rust/library/Cargo.lock"
       does not exist, unable to build with the standard library, try:
        rustup component add rust-src --toolchain nightly-x86_64-unknown-linux-gnu
```

This is the pre-change baseline. Any test that passes here has no teeth.

---

## 1. SC-001 — the canary builds the version the project already ships

```bash
pinned=$(grep -E '^BPF_LINKER_VERSION=' .github/env/bpf-linker.env | cut -d= -f2)
gh workflow run ebpf-canary.yml -f version="$pinned" -f dry_run=true
```

`dry_run=true` keeps it away from #685 while the build path is still being
proven — **but only since m896**. Before this feature, `dry_run` guarded
`report-failure` alone and `report-success` ran on a bare `if: success()`, so a
green dispatch from any branch closed the open report. Run `35257496241` closed
#685 that way during this feature's own verification. Wait, then:

```bash
gh run list --workflow=ebpf-canary.yml --limit 1 --json conclusion,databaseId
```

**Pass**: `success`. **Fail**: anything else — read `--log-failed` and fix
before proceeding; every later check assumes this one passes.

---

## 2. SC-008 — a build that produces nothing is red

Between the build step and the verification step, remove the artifact:

```bash
rm -f waybill-ebpf/target/*/bpfel-unknown-none/release/waybill-ebpf
```

**Pass**: the run concludes red and the report names the missing artifact.
**Fail**: the run goes green — the check is reading the wrong path, or reading
a leftover from an earlier step.

---

## 3. SC-003 / SC-004a — a self-inflicted failure says so

```bash
gh workflow run ebpf-canary.yml -f version=latest -f <break-input>=true
```

with `dry_run` **off**, so the report is actually written. Then:

```bash
gh issue list --label canary --state open --json number,title,createdAt
```

**Pass**: a new issue under the canary-fault title, alongside (not replacing)
#685. Its body contains the failing step and the error text, and contains no
instruction to file at `aya-rs/bpf-linker`:

```bash
gh issue view <n> --json body --jq .body | grep -ci 'bpf-linker/issues'   # expect 0
```

**Fail**: the report lands under `[canary] bpf-linker eBPF build regression`,
or mentions filing upstream. That is the 35-night failure reproducing.

---

## 4. SC-004 — a genuine component regression is attributed to the component

This one cannot be manufactured on demand; the control build has to pass while
the latest build fails. Two ways to see it:

- Wait for it to happen naturally (an expected outcome, per the spec's
  Assumptions — the canary has never once built, so whether `latest` has a real
  problem is still unknown).
- Dispatch with `version` set to a bpf-linker release known to be broken
  against this workspace, if one is identified.

**Pass**: report under the upstream-regression title naming both the failing
version and the passing control version.

---

## 5. SC-006 / SC-006a — escalation does not wait on anyone

Evaluate the rule against the live issue rather than simulating a streak:

```bash
gh issue view 685 --json createdAt --jq .createdAt     # 2026-08-13T06:36:09Z
python3 -c "
import datetime
start = datetime.datetime(2026, 8, 13, 6, 36, 9, tzinfo=datetime.timezone.utc)
print((datetime.datetime.now(datetime.timezone.utc) - start).days, 'days elapsed')"
```

35 days against a 30-day window (`docs/development/ebpf-toolchain.md:97`).

**Pass**: the report for this streak states the elapsed days and declares the
fallback due. **Fail**: an unchanged 36th comment — which is exactly what
m234's responsiveness-gated window produces, because no upstream issue was
ever filed.

---

## 6. SC-002 — the environments cannot silently diverge again

Change the canary's nightly `components:` to something other than `ci.yml`'s
and dispatch.

**Pass**: the run fails at the divergence check, before the build, and the
failure is attributed to the canary. **Fail**: the run proceeds to build —
the check is not reading what it claims to read.

---

## 7. SC-007 — recovery closes the right report

After a green run:

```bash
gh issue list --label canary --state open --json number,title
```

**Pass**: reports of both kinds are closed, each with a closing comment naming
the green run. A later failure opens a fresh report at elapsed 0.
**Fail**: a green run closes an outstanding upstream-regression report that
nothing actually fixed — the mirror image of the bug being fixed.

---

## What "done" looks like

| Check | Expectation |
|---|---|
| Pinned-version dispatch | green |
| Scheduled run the following night | green, or red with a *correctly attributed* cause |
| #685 | closed by a green run, or updated with an escalated, evidence-carrying report |
| Issue list | two distinguishable titles, tellable apart without opening either |

The canary has produced zero green runs in its lifetime. The first one is the
deliverable.
