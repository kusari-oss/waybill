# Data Model: Opt-in import-from-derivation

## IfdPolicy (requested; from the CLI)

`enum IfdPolicy { Refuse, Allow }`

- Default: `Refuse`. Set to `Allow` only by `--nix-allow-ifd`.
- `fn setting(&self) -> &'static str`: `"false"` or `"true"`. This is the only source of the IFD argv value at all 7 call sites (research R1).

## IfdPolicyVerified (attested; only the preflight constructs it)

`enum IfdPolicyVerified { Refused(IfdRefusalVerified), Allowed(IfdAllowanceVerified) }`

- `IfdRefusalVerified` is the existing type, unchanged. `IfdAllowanceVerified` is new, with the same private-constructor pattern (`result.rs`).
- Every evaluating entry point takes `IfdPolicyVerified` by value, as they take `IfdRefusalVerified` today. An unverified evaluation does not type-check.

## NixEnvironment (observed by the preflight)

| field | type | source |
|---|---|---|
| `sandbox` | `Option<String>` | the `sandbox = …` line of `nix config show`, verbatim (`true` / `false` / `relaxed`); `None` if absent |

## DegradationReason (C190 `reason`; extended closed enum)

| value | meaning | new? |
|---|---|---|
| `import-from-derivation-required` | evaluation needed IFD and the policy refused it | existing |
| `ifd-refusal-unverified` | the refusal could not be verified | existing |
| `ifd-allowance-unverified` | `--nix-allow-ifd` was passed but nix did not report `= true` | **new** |
| `import-from-derivation-build-failed` | IFD was allowed, and the derivation built during evaluation failed | **new** |

Every match is exhaustive, so a reason added later fails to compile until it is handled.

## C217 `waybill:nix-evaluation-policy`

See `contracts/c217-nix-evaluation-policy.md`. Built from `IfdPolicy` and `NixEnvironment`, with a `verified` flag: true only when a matching `IfdPolicyVerified` was constructed. Emitted whenever a Nix tier was admitted.

## Corpus: ScanMode and RecallTruth

- `ScanMode::NixClosure { attr: &'static str, allow_ifd: bool }`. Existing targets set `allow_ifd: false`.
- `RecallTruth` is the file `recall-truth.json`:

  ```json
  {
    "schema": 1,
    "produced": "<date>",
    "tools": {"sbomnix": "...", "grype": "...", "grype-db": "...", "osv-scanner": "..."},
    "target-pin": "<sha>",
    "findings": [
      {"id": "CVE-…", "name": "openssl", "version": "3.6.0", "matcher": {"cpe": "openssl:openssl"}},
      {"id": "HSEC-…", "name": "aeson", "version": "2.2.4.1", "matcher": {"purl": "pkg:hackage/aeson@2.2.4.1"}}
    ],
    "excluded": [{"id": "CVE-…", "name": "warp", "reason": "name-collision", "excerpt": "…"}]
  }
  ```

- Proxy match: a finding is recalled when the runtime-only view has a component with the same normalized name and version, plus either a CPE whose vendor:product equals `matcher.cpe` or a purl equal to `matcher.purl`.
