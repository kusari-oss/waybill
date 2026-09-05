# Quickstart — m776 component source-provenance references

**Audience**: reviewer or implementer verifying m776 acceptance locally after `/speckit.implement`.

**Duration**: ~25 min.

---

## Prerequisites

- Rust stable toolchain (pinned via `rust-toolchain.toml`).
- **Network access** — US1 depends on enrichment, which is the default path. US2 is the offline complement and is verified separately in Step 5.
- The five-ecosystem measurement set used to establish the baselines. Corpus fixtures resolve under `~/.cache/waybill/corpus/`; the harness fetches them if absent.
- Optional: `jq` for the inspection steps, `sbomqs` if you want to reproduce the original coverage measurement.

---

## Step 0 — Build

```sh
cargo build -p waybill --release
```

---

## Step 1 — Verify SC-001 and SC-002 (the headline)

Scan the Python and JavaScript fixtures with enrichment enabled (the default — do **not** pass `--offline`):

```sh
./target/release/waybill sbom scan --path <py-uv fixture> \
  --no-deep-hash --format cyclonedx-json --output /tmp/m776-py.json

jq '[.components[] | select(.externalReferences != null
      and ([.externalReferences[].type] | any(. == "vcs")))] | length
    , (.components | length)' /tmp/m776-py.json
```

Expected: at least 80% of components carry a `vcs` reference. Baseline was ~1 of 109.

Repeat for the JavaScript fixture; baseline there was **0 of 369**.

**If coverage is well below 80%**: check the Step 2 summary first. A high unmapped-skip count means the upstream label vocabulary moved — see Troubleshooting. A high malformed count means upstream data quality, not a waybill defect.

---

## Step 2 — Verify the aggregate summary (FR-014a/b, SC-009a)

```sh
RUST_LOG=info ./target/release/waybill sbom scan --path <py-uv fixture> \
  --no-deep-hash --format cyclonedx-json --output /dev/null 2>&1 \
  | grep -i "reference"
```

Expected: **exactly one** summary line per scan, reporting references emitted per kind plus two distinct skip counters — unmapped-label and malformed-URL.

Cross-check that the reported per-kind counts equal what is actually in the document:

```sh
jq -r '[.components[].externalReferences[]?.type] | group_by(.)
       | map("\(.[0]): \(length)") | join("  ")' /tmp/m776-py.json
```

The two must agree. The summary reports what happened; it is not an independent estimate.

**A non-zero unmapped count is expected**, not a failure: `ORIGIN` appears on essentially every component and is deliberately unmapped (Clarifications Q1). Roughly one unmapped skip per component is the normal steady state. What matters is whether that number *changes* over time.

---

## Step 3 — Verify the label mapping is label-driven (Contract 2)

Confirm a `HOMEPAGE` link pointing at a repository host still emits `website`, not `vcs`. Many packages set their homepage to their GitHub page, so real fixtures exercise this:

```sh
jq -r '.components[] | select(.externalReferences != null)
       | .externalReferences[] | select(.type == "website")
       | .url' /tmp/m776-py.json | grep -i github | head -3
```

Seeing repository-host URLs under `website` is **correct**. Inferring `vcs` from URL shape is precisely the guess FR-003 forbids.

---

## Step 4 — Verify SC-010 (diff confined to added references)

Compare against a pre-milestone binary built from `main`:

```sh
git worktree add /tmp/waybill-main main && \
  (cd /tmp/waybill-main && cargo build -p waybill --release)
```

Scan the same fixture with both, mask document-identity fields, and diff. **Every difference must be an added `externalReferences` entry.** Any component, relationship, license, or annotation change is a defect, not expected churn.

Per memory `feedback_verify_golden_churn_normalized`, mask content-addressed identifiers and `LC_ALL=C sort` before diffing, or SPDX 3 array reordering will fake semantic hits.

---

## Step 5 — Verify US2 on the offline path (SC-004)

```sh
./target/release/waybill --offline sbom scan --path <fixture> \
  --no-deep-hash --format cyclonedx-json --output /tmp/m776-offline.json

jq -r '[.components[].externalReferences[]?.type] | group_by(.)
       | map("\(.[0]): \(length)") | join("  ")' /tmp/m776-offline.json
```

Expected: `distribution` references present for ecosystems whose registry URL is PURL-determined, **and** the pre-existing `website` references still present (FR-011). US1 contributes nothing here by design — enrichment is not queried under `--offline`.

Spot-check that a derived URL actually resolves. If it 404s, the arm was added on a pattern rather than a verified registry scheme (Contract 2 of the US2 contract) — that is a fabricated reference and must be removed.

---

## Step 6 — Verify SC-005 (determinism) and SC-006 (wall time)

Two scans of the same input, masked, must be byte-identical — upstream link order is not contractually stable, so this is the guard against relying on it.

For wall time, compare the largest fixture against the `main` binary from Step 4; expect within 3%. The added work is a map over in-memory data plus string formatting, with no new I/O (FR-007). A larger regression suggests a network call crept in.

---

## Step 7 — The full gate

```sh
./scripts/pre-pr.sh
```

Expected: zero lint errors; every suite reporting all tests passed with none failed.

**Watch the parity suite specifically.** Catalog rows A9/A10/A11 and their extractors currently compare empty against empty; this milestone makes them meaningful for the first time (research R6). A failure there is a genuine cross-format mapping discrepancy — investigate it rather than treating it as surprising. It is the most likely place for this milestone to fail first, and that is the guard working.

---

## Troubleshooting

**Coverage far below 80% with a high unmapped-skip count.** The upstream label vocabulary moved. Compare observed labels against the mapped set:

```sh
curl -s "https://api.deps.dev/v3/systems/pypi/packages/flask/versions/3.0.0" \
  | jq -r '.links[].label' | sort -u
```

If a new high-frequency label appears, that is a mapping extension — a spec change, not a silent code fix. This is exactly the drift the counter exists to surface.

**Coverage below 80% with skip counters near zero.** The service genuinely lacks repository data for those packages. Sampling measured 100% for npm and 93% for pypi, so a large shortfall on those ecosystems is unexpected — verify the fixture's components are actually reaching enrichment at all.

**Parity failure on A9/A10/A11.** A reference kind is represented differently across formats. Note SPDX 3 has scalar slots only for vcs/website/distribution; `issue-tracker`, `documentation`, and `attestation` fall through its `_ => {}` arm by design (research R5). That asymmetry is accepted and should not be "fixed" by removing the kinds from CycloneDX and SPDX 2.3.

**Wall time regression beyond 3%.** Check for a per-component network call. FR-007 forbids one; the enrichment response is already fetched and cached per scan.

**Golden diff touches something other than references.** Not expected churn. Investigate before regenerating — SC-010 confines the diff to added references.

---

## Rollback triggers

Roll back if any of the following holds:

1. A derived distribution URL does not resolve for a package the arm claims to cover. A fabricated reference is worse than an absent one (Principle IX), and worse than the gap this milestone set out to close.
2. The golden diff extends beyond added references.
3. Wall time regresses beyond 3% and the cause is a network call rather than measurement noise.
4. A parity failure reveals that a kind cannot be represented consistently across the formats that claim to support it.

Rollback follows the m772/m773/m775 process: revert the PR, record the incident, and update the relevant memory entry.
