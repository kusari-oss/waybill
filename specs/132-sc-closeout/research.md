# Research: Close milestone-131 SC misses

**Date**: 2026-06-19
**Branch**: `132-sc-closeout`
**Driven by**: spec.md FR-012 (BLOCKING US3 implementation per the 2026-06-19 Q1 clarification)

## §Audit Baseline

> **DIGEST CAPTURED 2026-06-19** — resolved to
> `sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c` via
> `AWS_PROFILE=AWSAdministratorAccess-204753367867 aws ecr describe-images --registry-id 767397973649 --region us-east-1 --repository-name remediation-planner --image-ids imageTag=latest`
> (cross-account, registry-id flag required). Back-substituted into `spec.md §Assumptions`,
> `spec.md §Dependencies`, and every literal pin reference in this document. The
> placeholder preamble below is retained for historical context describing the BLOCKING
> contract per the 2026-06-19 Q3 clarification.

### Pinned digest capture

```sh
aws sso login                            # or `aws configure sso` if first-time
aws ecr describe-images \
  --region us-east-1 \
  --repository-name remediation-planner \
  --image-ids imageTag=latest \
  --query 'imageDetails[0].imageDigest' \
  --output text
```

**Decision**: Pin to `767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c`.
**Rationale**: Every quantitative number in `spec.md` (374 mismatches, 1107 / 2926
components with licenses, 339 PE/CLR fingerprint hits, US2's <50 target, all SC-003
per-path projections in §License Path Analysis below) is bound to a single moving
`:latest` tag today. Without an immutable pin, "SC met" is unverifiable across re-scans —
which is exactly the auditability failure the maintainer flagged when declaring milestone
131 "complete" against ephemeral measurements.
**Alternatives considered**:
- Keep `:latest`: rejected because SC verification becomes point-in-time-only and
  reviewers cannot reproduce the measurement. See spec.md §Clarifications Q3.
- Expand the audit corpus immediately (multi-image baseline): rejected per spec.md §Out
  of Scope item 1 — tracked for a follow-up milestone, not gating milestone 132's
  closeout work.

### Re-measurement protocol

Once `<DIGEST>` is captured:

```sh
docker pull 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c

# mikebom side
cargo build --release
./target/release/mikebom sbom scan \
  --image 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c \
  --output /tmp/mb-rp-132-baseline.cdx.json \
  --root-name 767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner \
  --offline

# syft side (re-measure for SC-001 / SC-002 / SC-003 / SC-004 verification — the
# cached ~/Downloads/remediation-planner-syft-image-sbom.json is bound to a stale
# `:latest` and MUST NOT be reused for milestone-132 SCs)
syft scan \
  registry:767397973649.dkr.ecr.us-east-1.amazonaws.com/remediation-planner@sha256:4e7b05811ce4885d8a7183819b4e0e209662784fe24b7553ceea3d149e3c719c \
  -o cyclonedx-json=/tmp/syft-rp-132-baseline.cdx.json

# sbom-comparison harness
/Users/mlieberman/Projects/sbom-comparison/sbom-comparison \
  --a /tmp/mb-rp-132-baseline.cdx.json --aLabel mikebom \
  --b /tmp/syft-rp-132-baseline.cdx.json --bLabel syft \
  --format json > /tmp/mb-rp-132-baseline.scorecard.json
```

The scorecard JSON's `licenses.effectiveRateA` + `licenses.starsA` fields are the
canonical SC-003 inputs. See §SC-003 Threshold below for the band mapping.

## §SC-003 Threshold (Research Task ORDER 0, per the 2026-06-19 Q2 clarification)

**Decision**: SC-003 "≥3/5 License Coverage" maps to **EffectiveRate ≥ 60.0 %** per the
`coverageStarsPct` banding function in `sbom-comparison/pkg/compare/packages.go:140`.
**Rationale**: Extracted directly from the comparison tool's source rather than
estimated. This closes the assumption-driven-implementation pattern that bit milestone
131 SC-002 (where the <20 target was based on an incorrect premise about the cause of
the 374 mismatches).
**Alternatives considered**:
- Define our own coverage-% target (e.g. ≥50 %): rejected because it disconnects from
  the standing audit metric the maintainer is reading.
- Defer the threshold definition to runtime ("implement Path A → measure → expand if
  short"): rejected per the same Q1 clarification — the milestone-131 mistake we are
  closing out.

### Extracted scoring formula

From `/Users/mlieberman/Projects/sbom-comparison/pkg/compare/packages.go:140-154`:

```go
// coverageStarsPct maps a 0-100 coverage percentage to 1-5 stars.
func coverageStarsPct(p float64) int {
    switch {
    case p >= 95: return 5
    case p >= 80: return 4
    case p >= 60: return 3
    case p >= 30: return 2
    default:      return 1
    }
}
```

| Stars | EffectiveRate band |
|---|---|
| 5★ | ≥ 95 % |
| 4★ | ≥ 80 % |
| 3★ | ≥ 60 % |
| 2★ | ≥ 30 % |
| 1★ | < 30 % |

### Extracted CDX license-resolution priority

From `/Users/mlieberman/Projects/sbom-comparison/pkg/sbom/cyclonedx.go:188-204`:

```go
func cdxLicenseExpr(lics []CDXLicense) string {
    for _, l := range lics {
        if l.Expression != "" { return l.Expression }
        if l.License != nil {
            if l.License.ID != "" { return l.License.ID }
            if l.License.Name != "" { return l.License.Name }
        }
    }
    return ""
}
```

A CDX component counts as "license present" (for the EffectiveRate numerator) if any of
the following are non-empty, non-`NOASSERTION`, non-`NONE`:

1. `licenses[i].expression`
2. `licenses[i].license.id`
3. `licenses[i].license.name`

From `/Users/mlieberman/Projects/sbom-comparison/pkg/sbom/cyclonedx_normalize.go:87-95`:
the CDX→Normalized mapping places this expression in `LicenseConcluded`, leaves
`LicenseDeclared` as `NoAssertion`, then `effectiveLicense(p)` returns `LicenseConcluded`
if resolved else `LicenseDeclared`. Practically: **any non-empty SPDX-ID-or-name on the
CDX component counts.** mikebom emitting `licenses[].license.id = "MIT"` is sufficient.

### Edge cases

- `NOASSERTION` / `NONE` strings (case-insensitive): treated as "not resolved" per
  `licenses.go:61-64`. mikebom MUST NOT emit either as a license expression.
- Empty `licenses[]` array: counts as "not resolved" (numerator unaffected).
- Multiple licenses: only the first usable expression is consumed per `cdxLicenseExpr`;
  mikebom may emit multiple but only the first ranks.

## §License Path Analysis

### Baseline measurement (cached `/tmp/mb-rp-131-final.cdx.json`)

> Numbers below are from the **cached final-131 SBOM**. They will shift slightly when
> the implementer re-runs against the pinned `<DIGEST>` per §Audit Baseline above. The
> ratios and the path decision do NOT change.

| Metric | Value |
|---|---|
| Total components | 2 926 |
| Components with non-empty `licenses[]` | 1 107 |
| EffectiveRate | 37.8 % |
| SC-003 stars (current) | 2★ |
| Target (≥60 %) | 1 756 components with licenses |
| Gap | **649 ADDITIONAL** components need licenses |

### Per-ecosystem gap

Measured by parsing `/tmp/mb-rp-131-final.cdx.json` and grouping by `purl` ecosystem
prefix:

| Ecosystem | Total | With license | Gap | Notes |
|---|---|---|---|---|
| `pkg:cargo` | 1 116 | 0 | 1 116 | Largest gap; deps.dev hit rate ≥95 % expected |
| `pkg:nuget` | 819 | 339 | 480 | Milestone-131 PE/CLR fingerprinter hits 41 %; deps.dev would cover the remaining 480 |
| `pkg:gem` | 85 | 0 | 85 | Out of scope for milestone 132 (see spec §Out of Scope item 5) |
| `pkg:maven` | 72 | 0 | 72 | Out of scope for milestone 132 (see spec §Out of Scope item 5) |
| `pkg:golang` | 61 | 0 | 61 | Out of scope for milestone 132 |
| `pkg:pypi` | 64 | 62 | 2 | Already at 96.9 % — no work |
| `pkg:npm` | 531 | 529 | 2 | Already at 99.6 % — no work |
| `pkg:apk` | 177 | 177 | 0 | 100 % — apk reader populates license-info |
| `pkg:generic` | 1 | 0 | 1 | Single root component; ignored |

### Path A — Extended PE/CLR fingerprinting (offline)

**Mechanism**: Extend the milestone-131 US2a `LICENSE.txt` fingerprint table at
`mikebom-cli/src/scan_fs/package_db/nuget/pe_clr.rs` from 6 SPDX IDs (Apache-2.0, MIT,
BSD-3-Clause, BSD-2-Clause, GPL-3.0, GPL-2.0) to additionally include: **MS-PL,
LGPL-2.1-only, LGPL-3.0-only, LGPL-2.1-or-later, MIT-0, Microsoft-Open-Source-License,
EPL-1.0, EPL-2.0**.

**Projected lift**: From 339 / 819 nuget hits (41 %) to ~470 / 819 (57 %). **+131 cargo
components** affected. **Path A alone delivers +131 components on the audit image** —
new total: 1 238 / 2 926 = 42.3 %. Still 2★.

**Verdict**: insufficient alone. Worthwhile as a complement to Path C for offline-mode
scans where deps.dev is unavailable.

### Path B — Rootfs-local cargo cache

**Mechanism**: Read
`~/.cargo/registry/cache/index.crates.io-*/<crate>-<version>.crate` if present in the
rootfs; extract the embedded `Cargo.toml`'s `license` field.

**Projected lift**: ~0. Production container images (including the audit image, which is
a remediation-planner runtime image) DO NOT ship cargo build artifacts in
`~/.cargo/registry/cache/`. These are deleted in the build stage's cleanup, or never copied
to the runtime stage at all. Spot check of the cached SBOM annotations: zero components
carry the existing `mikebom:cargo-cache-source` annotation (which milestone 131 added but
never matched).

**Verdict**: **REJECTED**. Effectively dead-letter for production images. Would consume
implementation budget for ~0 measurable lift.

### Path C — deps.dev online enrichment for cargo + nuget

**Mechanism**: For each emitted component with `purl` ecosystem in `{cargo, nuget}`,
issue a deps.dev `getVersion` query to retrieve the `licenses` array. Annotate the
emitted CDX `licenses[].license.id` with the canonical SPDX expression returned. Apply
provenance annotation `mikebom:license-source = "depsdev"`. Skip when `--offline` is set
(the audit run command uses `--offline`; SC-003 verification will use
`--offline=false`).

**Projected lift**:
- cargo: +1 116 × 0.95 hit rate ≈ **+1 060 components**
- nuget gap (where milestone-131 PE/CLR fingerprinter missed): +480 × 0.92 hit rate ≈
  **+442 components**
- Total: **+1 502 components**

New total: (1 107 + 1 502) / 2 926 = 2 609 / 2 926 = **89.2 %**. **4★** (exceeds
SC-003's ≥60 % / 3★ target with margin).

**Constitution check**: Permitted per Principle XII. Constraint 1 (no new components
introduced) holds because we only enrich existing eBPF-discovered components.
Constraint 2 (provenance annotation) holds via `mikebom:license-source = "depsdev"`.
Constraint 3 (graceful degradation) holds via the milestone-012 retry / timeout / cache
patterns reused. Constraint 4 (eBPF authority) holds because this is the scan_fs path,
not the trace path.

**Failure-mode handling**:
- deps.dev 404 (PURL not in registry) → omit `licenses[]` field; annotate
  `mikebom:license-source = "depsdev-not-found"` per the milestone-012 pattern.
- Network timeout → omit `licenses[]` field; annotate
  `mikebom:license-source = "depsdev-unavailable"` per the milestone-012 pattern. Scan
  still emits.
- Rate-limit (429) → respect `Retry-After`; retry up to 3× with exponential backoff per
  milestone-012's existing logic.

**Verdict**: **ACCEPTED as the primary US3 path**.

### Decision matrix

| Path | EffectiveRate | Stars | Offline-compatible | Implementation cost | Decision |
|---|---|---|---|---|---|
| A alone | 42.3 % | 2★ | Yes | Low (extend a constant table) | **Accepted as complement** for offline-mode runs |
| B alone | 37.8 % | 2★ | Yes | Low | **REJECTED** (~0 lift in production images) |
| C alone | 89.2 % | 4★ | No (requires network) | Medium (extend milestone-012's existing scaffolding) | **PRIMARY US3 path** |
| A + C combined | 89.2 % | 4★ | Partial (degrades gracefully when offline) | Low + Medium | **SHIPPED COMBINATION** |

**Final decision**: Ship Path A as a constant-table extension (always-on, no flag
required, no network); ship Path C as opt-in (`--enrich-licenses=depsdev`, gates on
`--offline=false`; off by default per Constitution III "Fail Closed" — operators
explicitly opt in to network access). For SC-003 verification on the pinned audit image,
both paths active; SC-006 (scan-time growth <30 %) measured with Path C ON because
that's the path doing real I/O.

## §Best-practices research (US1 supplier table)

**Decision**: Static `const SUPPLIER_TABLE: &[(&str, &str)]` lookup at the head of
`mikebom-cli/src/scan_fs/mod.rs`, keyed on `Purl::ecosystem()` return value.
**Rationale**: PURL ecosystems are a small closed set (cargo, nuget, maven, npm, pypi,
gem, apk, deb, rpm, golang, bitbake, opkg, swift); a static slice with linear scan is
trivially cache-friendly and avoids a `BTreeMap`'s heap allocation. Matches the existing
pattern at `scan_fs/mod.rs::supplier_from_purl` (lines 572+) which already does the
linear scan; this is a 10-entry extension to that table.
**Alternatives considered**:
- `phf` const-evaluated hash map: rejected because `phf` is not in the workspace
  dependency closure and Principle V says "no new Cargo dependencies for US1".
- Builder pattern with per-ecosystem method dispatch: rejected as overkill for a 10-row
  table.

## §Best-practices research (US2 stripped-Informational emission)

**Decision**: Emit `mikebom:assembly-version-informational-stripped` in the
`extra_annotations` bag of the existing PE/CLR `ResolvedComponent`, computed by splitting
the InformationalVersion on the first `+` and re-running the milestone-131
`is_plausible_version_string` sanity filter on the prefix. Skip when no `+` is present.
**Rationale**: SemVer §10 build-metadata semantics are unambiguous — everything after
the first `+` is build metadata and MUST NOT affect ordering / equality. Stripping at the
first `+` is the canonical SemVer-conformant way to obtain a comparable canonical form.
**Alternatives considered**:
- Strip on every non-numeric character: rejected because it loses pre-release suffixes
  (`-rc1`, `-alpha.3`) which ARE semantically meaningful per SemVer §9.
- Walk the FileVersion table instead: rejected because syft's choice of the FileVersion
  4-tuple is what we are NOT matching by default; emitting BOTH the original
  Informational AND the stripped form gives consumers the choice.
- Replace the existing `mikebom:assembly-version-informational` with the stripped form:
  rejected — milestone 131 deliberately emits Informational verbatim per SemVer §10
  ("build metadata SHOULD be ignored when determining version precedence" but is
  permitted in the representation). Adding a companion preserves both.

## §Best-practices research (US3 deps.dev cargo support)

**Decision**: Reuse the existing milestone-012 `mikebom-cli/src/enrich/depsdev_source.rs`
scaffolding; add a `pkg:cargo` arm to the `match purl.ecosystem()` dispatch. The deps.dev
API endpoint is `https://api.deps.dev/v3/systems/CARGO/packages/{name}/versions/{version}`
which returns a `licenses: [{spdxExpression: "MIT OR Apache-2.0"}]`-shaped JSON. Map to
the existing `DepsDevLicense` newtype.
**Rationale**: deps.dev's CARGO system identifier returns SPDX-canonical license
expressions already (per their docs). No string mangling needed mikebom-side beyond
trim + lowercase NOASSERTION rejection.
**Alternatives considered**:
- `crates.io` API directly: rejected because deps.dev unifies cargo + nuget under one
  endpoint shape; adding crates.io would mean two error-handling code paths instead of
  one.
- `ClearlyDefined` API: rejected because it returns license expressions in non-SPDX
  form for ~15 % of cargo packages; would require an expression-canonicalization layer.

## §Best-practices research (US4 retrospective spec edits)

**Decision**: Edit `specs/131-quality-metadata-backfill/spec.md` in place. Per FR-015,
APPEND a `**Status**:` line to each of SC-001 through SC-004 — the original target text
stays so the historical aspiration is preserved alongside what actually shipped. Per
FR-016, add a new `## Post-Milestone Outcomes (2026-06-19)` section immediately after
the existing Success Criteria block.
**Rationale**: The maintainer's flag was "you declared milestone 131 complete without
verifying SCs". The structural remediation is making the spec record honest. In-place
amendment + appended outcomes section is the smallest possible edit that achieves the
required honesty without rewriting milestone-131 history.
**Alternatives considered**:
- Rewrite the SCs to match what actually shipped: rejected — loses the audit trail of
  the original intent.
- Delete milestone-131's spec entirely: rejected — loses the historical record of why
  the work was attempted.
- Put the post-mortem in a separate file: rejected — splits the spec record from the
  outcome record; reviewers reading the milestone-131 spec would not see the post-mortem.

## §All NEEDS CLARIFICATION resolved

The `Technical Context` in `plan.md` carries no `NEEDS CLARIFICATION` markers. All
ambiguities surfaced during `/speckit-clarify` (Q1, Q2, Q3) are recorded in
`spec.md §Clarifications` and incorporated into FR-012 + Assumptions + Dependencies.
The `<DIGEST>` placeholder in §Audit Baseline above is NOT a clarification — it is a
documented BLOCKING prerequisite for the implementer, resolvable mechanically via the
shell command in this section.
