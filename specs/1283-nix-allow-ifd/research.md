# Research: Opt-in import-from-derivation

Code citations are from main@`1303ba82`. Measurements are in `measurements/`; each section names the file it relies on.

## R1 — Every place IFD is set

**Finding:** seven call sites pass `--option allow-import-from-derivation false` (`eval::preflight::IFD_SETTING`):

| site | what it runs |
|---|---|
| `nix/closure/mod.rs::closure_argv` | `nix derivation show -r` (the closure) |
| `nix/closure/mod.rs::attributes_argv` | `nix eval --apply builtins.attrNames` (attribute listing) |
| `nix/eval/invoke.rs:124` | `nix config show` (the refusal preflight) |
| `nix/eval/invoke.rs:~306` | the m1034 version evaluation |
| `nix/eval/licenses.rs:~212` | the m1281 meta/licence evaluation |
| `nix/declarations/evaluate.rs:~141, ~237` | the m1050 nixpkgs security-declaration evaluation |

The 1423 runtime query (`closure/runtime.rs`) runs `path-info` on output paths and does not evaluate, so it needs no setting.

**Decision:** replace the literal `"false"` at every site with a value taken from one `IfdPolicy`, passed down from `scan_cmd`. A grep test asserts that no `IFD_SETTING, "false"` literal remains outside the policy type (FR-002).

**Rationale:** one source of truth. A missed site would leave a scan half-allowed: the closure readable, the licence eval refused.

## R2 — Verifying the allowance (FR-003)

**Measured** (`measurements/ifd-eval/verify.txt`, nix 2.34.6): `nix config show --option allow-import-from-derivation true` prints `allow-import-from-derivation = true`. With `false` it prints `= false`. The same listing carries `sandbox = <bool>`.

**Decision:** generalise `preflight::verify_from_config_output` to verify the *requested* value. Under the opt-in it returns a new attestation, `IfdAllowanceVerified`, alongside today's `IfdRefusalVerified`. Both are produced only by the preflight. The type that gates evaluation becomes `IfdPolicyVerified { Refused(IfdRefusalVerified) | Allowed(IfdAllowanceVerified) }`, so no evaluation can run on an unverified policy.

- **Failure:** if the allowance line is missing, degrade with a new reason, `IfdAllowanceUnverified`.
- **Sandbox:** read the `sandbox` line from the same output. Record `true`, `false`, or `relaxed` verbatim; absent means unknown.

**Alternative rejected:** trust the option without verifying. That is the failure mode the refusal check exists to prevent: an old nix accepts an unknown option and ignores it.

## R3 — Cost of evaluating with IFD allowed

**Measured** (`measurements/ifd-eval/results.txt`; 3 runs each; outputs already built locally, so the IFD derivations are warm):

| flake | refused | allowed |
|---|---|---|
| slack-web | exit 1 in 0.74–0.91 s | exit 0 in 0.25–1.12 s, 1,535 derivations |
| openai | exit 1 in 0.80–0.93 s | exit 0 in 0.25–1.18 s, 1,830 derivations |

The cold case, where the cabal2nix output must be built first, was not measured. Its cost is one small derivation build plus fetching cabal2nix from the cache. It runs under the closure tier's existing 300 s per-call budget, and exceeding the budget degrades with `BudgetExceeded` (FR-010).

## R4 — The record carrier (FR-008, Principle V audit)

| candidate | present when | fits? |
|---|---|---|
| CycloneDX / SPDX native fields | — | No construct describes how a build-system evaluation was configured. |
| C190 `waybill:nix-closure-degraded` | only when the tier degraded | No: absent on success. |
| C184 `waybill:nix-closure` | only when the closure was read | No: absent on degradation. |
| C204 `waybill:nix-closure-read` | when the reader ran | It describes what the reader saw and did not consult, not the evaluation policy. Adding the policy would mix two meanings. |
| C216 `waybill:closure-scope-method` | when the tier ran | It is the scope decision. Also absent on degradation. |

**Decision:** add a new document-scope row, **C217 `waybill:nix-evaluation-policy`**: a JSON object emitted whenever a Nix tier was admitted (`--nix-closure` or `--nix-eval` requested and not `--offline`), on success and on degradation alike. Contract: `contracts/c217-nix-evaluation-policy.md`. Classification is KEEP-NO-NATIVE, with the audit above as justification.

## R5 — The flag's surface

**Decision:**
- Name: `--nix-allow-ifd`.
- It requires at least one Nix tier (`--nix-closure` or `--nix-eval`), through a clap `ArgGroup`, mirroring how `--nix-closure-attr` uses `requires = "nix_closure"` (`scan_cmd.rs:728`). Without a tier, clap rejects it with its standard message. This settles the spec's deferred edge case: reject, don't ignore.
- No environment variable. An opt-in that weakens a safety rule should be visible on the command line.

**Help text** (FR-005): "Allow import-from-derivation during Nix evaluation. Nix will BUILD derivations while evaluating the flake (e.g. callCabal2nix). Do not use on an untrusted flake. Where nix's sandbox is off (the macOS default) those builds run unsandboxed."

## R6 — Warnings (FR-006, FR-013)

- **IFD degradation without the opt-in:** extend the #1446 WARN with: "Pass --nix-allow-ifd to allow it, only for a flake you trust."
- **Opt-in with `sandbox = false`:** one WARN per scan, before evaluation: "--nix-allow-ifd: nix's sandbox is off; derivations built during evaluation run unsandboxed with your user's access."
- **IFD build failure:** a new reason, `ImportFromDerivationBuildFailed`, distinguished by classifying nix's stderr. The existing `stderr::classify_failure` gains the pattern: an evaluation error naming a `.drv^out` that failed to build while IFD was allowed (FR-010).

## R7 — The nightly corpus targets and their truth (FR-014)

**Targets:** slack-web `7b46273d`, openai `8a357137` and slacklinker `72af26a7`. Each gets `ScanMode::NixClosure { attr: "default", allow_ifd: true }`. The harness adds `--nix-allow-ifd` when `allow_ifd` is set. All three are github.com repos, so the public-only allowlist already admits them.

**Truth baseline:** the nightly job runs neither sbomnix nor grype.

- **Baseline file:** `waybill-cli/tests/fixtures/public_corpus/<target>/recall-truth.json`, holding the truth findings. Each is `(vuln id, package name, version, matcher)`, where the matcher is a `cpe` vendor:product or a `pkg:hackage` purl.
- **Producing it:** a dispatch-only workflow, `recall-truth.yml`, on x86_64-linux. It realises the output, then runs sbomnix (`nix run nixpkgs#sbomnix` at a pinned nixpkgs revision), grype at a pinned version with a fixed database, and osv-scanner. It applies the committed name-collision classifier (`specs/1283-nix-allow-ifd/measurements/recall/classify.py`, promoted from the corpus work) and uploads the files as an artifact. They are committed after review, the same flow as goldens. They are refreshed when the target pin changes or by deliberate dispatch.
- **Nightly expectation (component-level proxy):** for each truth finding, the runtime-only view (`--drop scope=excluded`, applied in-harness through the library) must contain a component with that name and version, plus either a CPE with the matcher's vendor:product or the matching `pkg:hackage` purl. Recall is the share of truth findings satisfied, and it must be ≥ 0.90 (SC-001–SC-003).
- **Why a proxy:** grype's match is a function of exactly those fields, so the proxy can't pass where grype would fail for identity reasons. It can diverge only on version-range edge cases. The one-off real-grype measurement (quickstart) checks agreement and records it.

**Alternative rejected:** run grype in the nightly job. That adds a vulnerability-database download, and nightly results would drift with the database rather than with waybill.

## R8 — What the opt-in does not change (FR-004)

`argv_is_safe` keeps refusing `--accept-flake-config` and `--impure` (`eval/invoke.rs:300`). Flake `nixConfig` requests, such as slack-web's extra substituters, remain ignored, so IFD derivations build from the user's configured substituters or locally. A test asserts that the refused flags are still refused under the opt-in.
