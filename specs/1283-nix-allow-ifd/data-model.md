# Data Model: Opt-in import-from-derivation

## IfdPolicy (requested; from the CLI)

`enum IfdPolicy { Refuse, Allow }`

- Default: `Refuse`. Set to `Allow` only by `--nix-allow-ifd`.
- `fn setting(&self) -> &'static str`: `"false"` or `"true"`. This is the only source of the IFD argv value at all 8 call sites (research R1).

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

- **Proxy match:** a finding is recalled when the runtime-only view has a component that satisfies all three:
  1. **Name.** Its normalized name equals the finding's. Normalization lowercases the name and strips a leading `python3-` / `python3.N-` (`norm_name` in `measurements/recall/recall.py`, copied from `specs/1377-nix-subject-identity/measurements/analyze.py`).
  2. **Version.** Its `version` equals the finding's, as strings.
  3. **Matcher.**
     - `matcher.cpe`: the vendor:product (fields 4–5 of a CPE 2.3 name) of the component's `cpe` or of any `waybill:cpe-candidates` value equals `matcher.cpe`.
     - `matcher.purl`: the component's purl equals it, ignoring qualifiers. For a purl matcher, step 1 is skipped and the purl decides.
- **Runtime-only view:**
  - Components are counted recursively: CycloneDX components nest, so walk `components[].components[]`.
  - Components with CycloneDX `scope == "excluded"` are dropped.
- **Truth:** finding IDs are canonicalized to the CVE alias when one exists (`canon` in `recall.py`), so a GHSA and its CVE count once.
