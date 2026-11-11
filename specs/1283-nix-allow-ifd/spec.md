# Feature Specification: Opt-in import-from-derivation for Nix closure scans

**Feature Branch**: `1283-nix-allow-ifd`
**Created**: 2026-11-11
**Status**: Draft
**Input**: User description: "1283 Opt-in import-from-derivation for --nix-closure. Closes #1283. Measured 2026-11-11 (main@1303ba82, aarch64-darwin): waybill refuses IFD, so cabal2nix/callCabal2nix flakes get no closure. 3 of the 4 project flakes in the recall corpus need IFD: MercuryTechnologies/slack-web@7b46273d (recall 7%), openai@8a357137 (12%), slacklinker@72af26a7 (degraded; truth not measurable on darwin). Building the output first does not help. sbomnix 1.9.0 inherits nix's default allow-import-from-derivation=true, so it performs IFD silently. Add --nix-allow-ifd (off by default), passing allow-import-from-derivation=true to every closure-tier and eval-tier nix call. Docs and --help say it builds derivations during evaluation and must not be used on an untrusted flake. The IFD degradation WARN names the flag. The document records that IFD was allowed (audit C190/C204/C216 for the carrier first, Principle V). Success: with the flag, slack-web, openai and slacklinker (measured on x86_64-linux) each reach >= 90% recall of real vulns (sbomnix runtime grype findings minus name-collision false positives, plus Haskell advisories), using the recall-corpus probe. Zero new Cargo deps; no network beyond the closure tier's existing reach."

Every figure below comes from `measurements/README.md` unless it says otherwise.

## Background

Many Haskell flakes describe their package with cabal2nix at evaluation time (`callCabal2nix`, `developPackage`, `packageSourceOverrides`). Evaluating them requires **import-from-derivation (IFD)**: nix builds a small derivation, the generated Nix expression, and imports its output, all *during* evaluation.

waybill refuses IFD in every Nix invocation and verifies that the refusal took effect. The reason is safety: building during evaluation runs code from the scanned repository. As a result, these flakes get no closure. Of the four public project flakes in the recall corpus, three need IFD, and waybill's recall of real vulnerabilities on them is 7% and 12% (the third can't be measured on this platform). Building the output beforehand does not help: slack-web's output was already in the store and evaluation was still refused.

sbomnix reads the same flakes because it inherits nix's default, `allow-import-from-derivation = true`, and never says so. On this host nix's sandbox is also off, which is nix's macOS default, so those builds run unsandboxed.

This feature lets a user who trusts the flake opt in, records that they did, and leaves the safe default unchanged.

## Clarifications

### Session 2026-11-11

- Q: When nix's sandbox is off (the macOS default), what does the opt-in do? → A: Allow it (option A): log a WARN naming the unsandboxed build, and record `sandbox=false` in the document. No second flag, no refusal.
- Q: Where are the IFD flakes measured on an ongoing basis? → A: As pinned public-corpus targets (option A). slack-web, openai and slacklinker scan with the opt-in in the nightly Linux job, which is sandboxed and disposable, and the success criteria are checked there as corpus expectations.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Scan a callCabal2nix flake I trust (Priority: P1)

A user scans their own Haskell flake, which uses callCabal2nix, and passes the opt-in. waybill evaluates it with IFD allowed, reads the closure, and produces the same quality of document it does for flakes that don't need IFD: runtime scope, static linking, licences and VEX. The document states that IFD was allowed.

**Why this priority**: three of four project flakes measured need it. Without this, the closure tier gives nothing for the most common way Haskell projects are packaged with Nix.

**Independent Test**: scan slack-web with the opt-in, then check that the closure was read (not degraded), that the document records IFD as allowed, and that recall against the corpus truth is ≥ 90%.

**Acceptance Scenarios**:

1. **Given** slack-web's flake and the opt-in, **When** it is scanned with `--nix-closure`, **Then** the closure is read, no IFD degradation is recorded, and the document says IFD was allowed.
2. **Given** the same flake **without** the opt-in, **When** it is scanned, **Then** behaviour is unchanged from today: degraded, with the WARN, which now names the opt-in.
3. **Given** a flake that does not need IFD (moat), **When** it is scanned with the opt-in, **Then** the document is identical to a scan without it, except for the record that IFD was allowed.

---

### User Story 2 - Understand the risk before opting in (Priority: P1)

A user reading `--help`, the docs, or the degradation WARN learns what the opt-in does and when not to use it, before they use it.

**Why this priority**: the opt-in trades away a safety property, so the trade has to be stated where the decision is made.

**Independent Test**: read `--help` and the WARN. Each says IFD builds derivations during evaluation, must not be used on an untrusted flake, and may build unsandboxed where nix's sandbox is off.

**Acceptance Scenarios**:

1. **Given** `waybill sbom scan --help`, **When** the opt-in is listed, **Then** its text states that it builds derivations during evaluation and must not be used on an untrusted flake.
2. **Given** a scan that degrades for lack of IFD, **When** the WARN is logged, **Then** it names the opt-in and repeats the untrusted-flake caution.
3. **Given** the user guide's Nix section, **When** a reader looks for IFD, **Then** they find what it is, why waybill refuses it by default, the sandbox caveat, and how sbomnix behaves differently.

---

### User Story 3 - Know from the document how it was produced (Priority: P2)

A consumer reading a waybill document can tell whether IFD was allowed for the scan that produced it.

**Why this priority**: two documents for the same flake differ greatly with and without the opt-in, and the reader must be able to tell which they hold.

**Independent Test**: compare documents from scans with and without the opt-in, and check that the record differs as specified, in all three formats.

**Acceptance Scenarios**:

1. **Given** a scan with the opt-in, **When** the document is read, **Then** it records that IFD was allowed, in every format, carried in a construct that keeps parity across formats.
2. **Given** a scan without it, **When** the document is read, **Then** it records that IFD was `refused`, with `verified: true` when today's refusal check succeeds.

### Edge Cases

- **The setting doesn't take effect.** Some nix versions accept an unknown `--option` and ignore it. waybill already verifies that the *refusal* applied, and it MUST verify the *allowance* too, rather than assume it. If it can't be verified, degrade with a recorded reason, not silently.
- **The user's nix configuration already disallows IFD** (`allow-import-from-derivation = false` in nix.conf). The opt-in passes the setting explicitly, so the command-line value wins, as for any nix option. Record what was actually in effect.
- **The flake's `nixConfig` requests extra substituters or keys** (slack-web's and slacklinker's do). These remain refused: waybill doesn't pass `--accept-flake-config`. IFD derivations are therefore built from the user's configured substituters, or locally.
- **The IFD build fails.** Example: slacklinker's output fails `disallowedReferences` on darwin. If evaluation fails, the closure tier degrades with the failure recorded, as for any evaluation failure. This must not be confused with the IFD refusal.
- **The IFD build is slow or hangs.** It runs under the closure tier's existing time budget; exceeding it degrades with the budget reason.
- **The nix sandbox is off** (the macOS default). The opt-in still applies: waybill can't make the build sandboxed, and refusing would exclude most macOS users. Each such scan logs a WARN that IFD builds will run unsandboxed, and the document records `sandbox=false` (FR-013).
- **`--offline`.** The closure tier doesn't run under `--offline` (C190 `offline-requested`), so the opt-in has no effect there, and saying so is enough.
- **The opt-in without `--nix-closure`.** It has nothing to apply to. waybill accepts it with an INFO note, or rejects it with a clear message; the plan decides which.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: waybill MUST provide an opt-in, off by default, that allows import-from-derivation for the scan's Nix evaluations. Without it, behaviour MUST be unchanged from today, IFD refused and the refusal verified, except for the FR-008 record, which every scan that runs a Nix tier now carries. Documents from scans without a Nix tier MUST be byte-identical.
- **FR-002**: With the opt-in, every Nix invocation in the closure tier and the version-evaluation tier MUST allow IFD. No invocation may still refuse it, or a scan could half-succeed.
- **FR-003**: With the opt-in, waybill MUST verify that nix actually applied the allowance, the same way it verifies the refusal today. If it can't be verified, it MUST degrade with a distinct recorded reason.
- **FR-004**: The opt-in MUST NOT relax any other safety rule. `--accept-flake-config` and `--impure` remain refused, and flake-supplied substituters and trust keys remain ignored.
- **FR-005**: `--help` MUST describe the opt-in as building derivations during evaluation, MUST say it must not be used on an untrusted flake, and MUST mention that builds may be unsandboxed when nix's sandbox is off.
- **FR-006**: When the closure tier degrades because IFD was refused, the WARN (#1446) MUST name the opt-in and repeat the untrusted-flake caution.
- **FR-007**: The user guide MUST document:
  - what IFD is;
  - why waybill refuses it by default;
  - the opt-in, with the risk statement;
  - the sandbox caveat;
  - that sbomnix 1.9.0 performs IFD silently by inheriting nix's default.
- **FR-008**: Every document from a scan where a Nix tier (`--nix-closure` or `--nix-eval`) ran MUST record whether IFD was allowed or refused, and nix's reported sandbox setting. The record MUST appear in all three formats with parity. Under `--offline` no tier runs, and C190 `offline-requested` takes the record's place. Before adding a field, the plan MUST audit C190, C204 and C216 for a fitting carrier (Principle V), and cite the result.
- **FR-009**: A scan with the opt-in on a flake that doesn't need IFD MUST produce the same components, relationships, scope and VEX as a scan without it. Only the FR-008 record may differ.
- **FR-010**: When an IFD build fails, or exceeds the closure tier's budget, the degradation MUST be recorded with that reason, distinct from "IFD refused".
- **FR-011**: The opt-in MUST be testable without network access and without real builds, using the existing stub-nix harness: allowance passed, allowance verified, allowance not verified, IFD build failure.
- **FR-012**: No new Cargo dependencies. Network reach stays what the closure tier already has, including the configured substituters from 1423. IFD derivations may themselves fetch through nix, as any nix build does; this is the risk FR-005 states.
- **FR-013**: When the opt-in is used and nix reports `sandbox = false`, waybill MUST log a WARN on that scan saying derivations built during evaluation run unsandboxed, and MUST still proceed. No second confirmation is required.
- **FR-014**: slack-web, openai and slacklinker MUST be added as public-corpus targets pinned to commit SHAs, scanning with `--nix-closure` and the opt-in in the nightly x86_64-linux job. Their expectations MUST assert that the closure was read (no IFD degradation), that IFD was recorded as allowed, and that recall clears the SC-001–SC-003 bars against the committed truth rule. Goldens are produced by the CI regen, never locally.

### Key Entities

- **IFD policy**: refused (default, verified) or allowed (opt-in, verified). This is per scan.
- **Sandbox setting**: nix's reported `sandbox` value at scan time. It is recorded, not changed.
- **IFD degradation reasons**: the existing `import-from-derivation-required`, plus new distinct reasons for "allowance not verified" and "IFD build failed".

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With the opt-in, slack-web's runtime-only view reaches ≥ 90% recall of real vulnerabilities, measured by the recall-corpus probe against the committed truth rule. It is 7% today.
- **SC-002**: With the opt-in, openai reaches ≥ 90% under the same measure. It is 12% today.
- **SC-003**: With the opt-in, slacklinker, measured on x86_64-linux where its build succeeds, reaches ≥ 90% under the same measure.
- **SC-004**: Without the opt-in, every existing test, golden and corpus target is unchanged except for the added FR-008 record on scans that run a Nix tier, and goldens of scans without a Nix tier are byte-identical (FR-001).
- **SC-005**: On a flake that doesn't need IFD, documents produced with and without the opt-in differ only in the FR-008 record (FR-009).
- **SC-006**: A reader of `--help`, the WARN, or the docs can find the untrusted-flake caution in each, in one place each.

## Assumptions

- **Opt-in, not default:** the user chose an opt-in with clear documentation (2026-11-11). Default-on was considered and rejected as a change to waybill's security posture.
- **Same evaluation path:** IFD is enabled by changing the setting passed to the same Nix invocations waybill already makes. No new kind of evaluation is introduced.
- **Recall truth:** the recall-corpus truth rule is sbomnix runtime grype findings minus name-collision false positives, plus Haskell advisories (HSEC). It counts what nix references as shipping. For libraries this includes GHC's toolchain (openai's 52 truth findings all reach the output through GHC), which the success criteria accept as measured. Whether toolchain references in a library's closure should count is a separate question, not this feature's.
- **Measuring the IFD flakes:** all three are measured on x86_64-linux in the nightly corpus job (FR-014), which is sandboxed and disposable. That matters most for slacklinker, whose build fails on darwin. Recall truth for the corpus expectations is computed from a committed sbomnix runtime baseline per target, because the nightly job doesn't run sbomnix; the plan decides how the baseline is produced and refreshed.
- **Out of scope:** haskell.nix flakes (no measured target uses it), relaxing `--accept-flake-config`, and sandboxing builds on the user's behalf.
