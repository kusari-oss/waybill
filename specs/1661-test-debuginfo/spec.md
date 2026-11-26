# Feature Specification: Debug info in test builds

**Feature Branch**: `1661-test-debuginfo`
**Created**: 2026-11-26
**Status**: Draft
**Input**: #1719, fifth slice. Refs #1719.

## Context

All numbers below are measured. The full method and results are on #1719 and #1714.

**Locally.** Apple M5 Max, 18 cores. Each measurement is a workspace-only rebuild after touching `lib.rs` and `main.rs`, in a fresh target directory per setting, 3 rebuilds each.

| setting | entries in `target/debug/deps` | rebuild time | integration-test compile CPU |
|---|---|---|---|
| macOS default (`split-debuginfo` unpacked, full debug info) | 65,923 → 97,032 → 128,141 | 29.3 → 33.1 → 42.2 s | 371 s |
| `split-debuginfo` packed | steady at 2,458 | 35.5 / 37.4 / 38.5 s | — |
| line-tables-only | grows, as the default does | similar to the default | 366 s |
| no debug info | steady at 2,022 | 21.5 / 21.8 / 21.3 s | 240 s |

- **What grows.** With the default, every rebuild leaves about 31,100 new codegen-unit object files (`*.rcgu.o`) and nothing removes them. Rebuilds slow down until someone runs `cargo clean`. This is the documented "slow `pre-pr.sh`, run `cargo clean`" experience.
- **On CI,** the test build compiles only workspace crates: the dependency cache is restored and no dependency is rebuilt. It took 2 min 35 s on Linux and 7 min 33 s on macOS in run 45757537236.
- **CI starts from a fresh directory each time,** so it never sees the accumulation. Whether a debug-info change saves time on CI is a prediction until it's measured there.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Local rebuilds stop slowing down (Priority: P1)

A contributor on macOS runs the gate repeatedly while working. Each rebuild costs about the same as the last, and nobody needs `cargo clean` to restore speed.

**Why this priority**: it is the measured cause of the progressive local slowdown, and fixing it removes a recurring manual chore.

**Independent Test**: three successive rebuilds after a code change, in a fresh target directory. The build directory's entry count stays flat and the rebuild time doesn't climb.

**Acceptance Scenarios**:

1. **Given** the change, **When** a macOS contributor rebuilds 3 times after touching waybill's sources, **Then** the `target/debug/deps` entry count is the same after each rebuild, and the rebuild times (min/median/max) show no upward trend.
2. **Given** a test that panics, **When** it runs on macOS with backtraces enabled, **Then** the backtrace still names the source file and line. This is a positive control: full debug info is kept, only packaged differently.
3. **Given** Linux or Windows, **When** they build, **Then** their behaviour is unchanged. The setting applies to macOS only, or the spec records why it can't be scoped and what was done instead.

---

### User Story 2 - CI test builds cost less, if they do (Priority: P2)

A contributor's CI run spends less time compiling tests, without losing usable backtraces when a test fails.

**Why this priority**: it is a likely saving, but only a prediction. It is adopted only if measured on CI.

**Independent Test**: the PR branch's CI runs with full debug info, line-tables-only, and none, 2 runs each. Per lane, the test-build time and the `Tests` step are recorded, along with what a deliberately panicking test's backtrace shows.

**Acceptance Scenarios**:

1. **Given** each setting, **When** CI runs 2 times, **Then** the test-build and `Tests`-step times per lane are recorded.
2. **Given** each setting, **When** a test panics on CI, **Then** the record says whether its backtrace shows file and line.
3. **Given** the measurements, **When** a setting both saves time on CI and keeps backtraces usable, **Then** it is adopted. Otherwise CI is left as it is, and the decision and numbers are recorded either way.

---

### User Story 3 - The guidance matches reality (Priority: P3)

The project's instructions stop telling contributors to `cargo clean` for speed, once that's no longer needed.

**Why this priority**: stale guidance costs time and erodes trust in the rest of it.

**Independent Test**: CLAUDE.md and the memory note on slow `pre-pr.sh` reflect User Story 1's result.

**Acceptance Scenarios**:

1. **Given** User Story 1's result, **When** the guidance is read, **Then** it describes the cause and the fix, and no longer prescribes `cargo clean` for build speed.

---

### Edge Cases

- **A contributor who already has a large, accumulated target directory** keeps those files until one `cargo clean`. The fix stops new growth; it doesn't remove what's already there. The guidance says so.
- **Packed debug info on macOS adds a bundling step per binary** (measured: first rebuild 35.5 s against 29.3 s for the default). That's an accepted cost: the default passes it within 2 rebuilds and keeps climbing.
- **The eBPF lane** builds with different features on Linux. It's out of scope unless the CI change is the same environment line, and verified there.
- **Release builds and the shipped binary** are unaffected. Only development and test builds change.

## Requirements *(mandatory)*

### Functional Requirements

**Local (macOS)**

- **FR-001**: On macOS, development and test builds MUST package debug info so that rebuilds don't accumulate object files, while keeping full debug info.
- **FR-002**: The setting MUST NOT change Linux or Windows builds. If it can't be scoped to macOS in the workspace manifest, the spec's plan MUST record where it lives instead, and why.
- **FR-003**: FR-001 MUST be shown on macOS:
  - a flat entry count across 3 rebuilds;
  - rebuild times as min/median/max;
  - a panicking test whose backtrace names file and line.

**CI**

- **FR-004**: On the PR branch, CI MUST be measured with full debug info, line-tables-only and none, 2 runs each, on the Linux, macOS and Windows test lanes. Record:
  - the test-build time;
  - the `Tests`-step time;
  - whether a panicking test's backtrace shows file and line.
- **FR-005**: A CI setting MUST be adopted only if it reduces the measured time and keeps file and line in backtraces. The decision and the numbers MUST be recorded either way.

**Guidance and constraints**

- **FR-006**: CLAUDE.md and the memory note on slow `pre-pr.sh` MUST be updated once FR-003 holds.
- **FR-007**: No production code changes. Zero new Cargo dependencies. No corpus golden changes. The gate contract (constitution 4.1.0) is unchanged. One gate, one PR.

### Key Entities

- **Rebuild measurement**: host, setting, rebuild number, entry count, time.
- **CI measurement**: run, lane, setting, test-build time, `Tests`-step time, and whether the backtrace shows file and line.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On macOS, the build directory's entry count after the 3rd rebuild is within 1% of the count after the 1st (today it grows by about 95%).
- **SC-002**: On macOS, the 3rd rebuild is no slower than the 1st by more than the run-to-run spread (today it's 44% slower).
- **SC-003**: A panicking test's backtrace names its file and line on macOS after the change.
- **SC-004**: CI timings for the 3 settings are reported, 2 runs each per lane, with a recorded adopt-or-reject decision.

## Assumptions

- The local measurements generalize to other contributors' macOS machines. The cause is a cargo default, not something specific to this host.
- **Out of scope:**
  - #1713;
  - #1714 (measured, left open);
  - caching workspace build artifacts on CI, which is blocked on stable because content-based freshness (`-Zchecksum-freshness`) is nightly-only.
