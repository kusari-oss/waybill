# Contract — go.work directive vocabulary + parser agreement (US2)

**Feature**: 775-preflight-single-flight
**Status**: Complete
**Date**: 2026-09-05

Contract for the shared directive vocabulary and the two `go.work` parsers that consult it.

---

## Contract 1 — Single shared vocabulary (FR-014, Clarifications Q2)

**Post-milestone**: the set of directive keywords the `go.work` format defines — `go`, `toolchain`, `godebug`, `use`, `replace` — is declared once and consulted by both parsers.

**Consumers**:
- Strict validator (`gowork.rs`) — decides whether an unrecognized leading token yields `unknown-directive`.
- Lenient member-extractor (`mod_why.rs`) — confirms a skipped non-`use` line is a directive it legitimately ignores.

**Property**: recognizing a directive added by a future Go release is a **one-place edit**. A change to one parser's directive knowledge alone must not be able to make the two disagree.

**Verification**: a test asserts both parsers agree on validity across a fixture corpus spanning every vocabulary directive. Editing one parser in isolation fails that test.

---

## Contract 2 — `godebug` accepted (FR-011)

**Pre-milestone**: `godebug default=go1.26` ⇒ `unknown-directive` ⇒ the emitted `waybill:go-workspace-mode` annotation reads `malformed: unknown-directive`. This is the observed Kubernetes behavior.

**Post-milestone**: accepted as valid; the annotation reports the workspace as detected with its member count.

**Semantics**: accept-and-ignore. `GoWorkDocument` gains no field (research R6). Repeated `godebug` lines are valid — Go permits them — so no duplicate detection applies.

**Verification**: unit test on a `go.work` containing `godebug`; assert detected-with-member-count, not malformed.

---

## Contract 3 — `toolchain` accepted (FR-012)

**Post-milestone**: `toolchain go1.26.0` is accepted as valid, accept-and-ignore, same treatment as `godebug`.

**Verification**: unit test asserting detected-with-member-count.

---

## Contract 4 — Genuinely malformed input still reported (FR-013)

**Post-milestone**: the existing malformed-reason vocabulary is preserved verbatim: `invalid-use-path`, `duplicate-use-path`, `invalid-replace-syntax`, `unknown-directive`.

Tolerance is scoped to the vocabulary in Contract 1. A token outside it still yields `unknown-directive`; an unparseable `use` path still yields `invalid-use-path`; a repeated `use` path still yields `duplicate-use-path`.

This is the anti-over-correction contract: the fix must not degrade into accepting arbitrary input.

**Verification**: unit tests — a `go.work` with a genuinely unknown token yields `unknown-directive`; the existing malformed-input tests pass unchanged.

---

## Contract 5 — No regression for previously-valid files (FR-007)

**Post-milestone**: a `go.work` using only directives already supported pre-milestone parses to the identical result, and its annotation value is unchanged.

**Verification**: the existing `gowork.rs` unit tests pass unchanged.

---

## Contract 6 — Bounded, reviewed golden diff (FR-007, SC-002, research R7)

**Post-milestone**: the only permitted emitted-document difference is the `waybill:go-workspace-mode` annotation value, and only on repositories whose `go.work` contains `toolchain` or `godebug`.

The k8s corpus golden is the known instance: its value moves from `malformed: unknown-directive` to the detected form.

**Verification**: regenerate affected goldens; confirm — per memory `feedback_verify_golden_churn_normalized`, masking content-addressed IDs and sorting before diffing — that the diff is confined to that annotation. Any component, relationship, or other-annotation change is a defect, not expected churn.

---

## Contract 7 — Parser self-consistency within one scan (FR-014)

**Post-milestone**: a single scan cannot emit an annotation claiming a `go.work` is malformed while simultaneously enumerating workspaces parsed from that same file.

Pre-milestone, the k8s scan did exactly this: `waybill:go-workspace-mode = malformed: unknown-directive` alongside a correctly-populated `waybill:workspaces-detected` listing all 38 workspaces.

**Verification**: scan a fixture whose `go.work` carries `godebug`; assert the workspace-mode annotation and the workspaces-detected annotation tell a consistent story.
