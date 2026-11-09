In a Nix closure, `python3` is emitted without a CPE. For a Python application it is in the runtime closure, so it ships, and a CPE-matching scanner reports nothing against it.

## Measurement

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin, waybill `main@72b455e2`, grype 0.120.0. Target: a wrapper flake exposing `nixpkgs#httpie` (3.2.4) as `packages.<system>.default`, scanned with `--nix-closure`.

| | waybill | sbomnix 1.9.0 (`--buildtime`) |
| --- | --- | --- |
| `python3 3.13.13` CPE | none | `cpe:2.3:a:python:python:3.13.13:*:*:*:*:*:*:*` |
| grype findings on it | **0** | **35** |

`python3` is in Nix's runtime closure of httpie (`nix path-info -r`). None of the 35 is attributable to a CVE-named nixpkgs patch on python3; they are otherwise not adjudicated.

nixpkgs cannot supply this one: `python313.meta.identifiers` has `cpeParts.product = "python3"` and no vendor, so no `cpe`. sbomnix's CPE here comes from its name heuristic, not from nixpkgs metadata.

## Expected

The curated CPE table gains a `python:python` row for the CPython interpreter closure member, so the interpreter a Python application ships with is matchable.

*Not measured:* whether other interpreters in closures (perl, ruby, nodejs) have the same gap.

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
