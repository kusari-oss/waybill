nixpkgs declares an authoritative CPE for many packages in `meta.identifiers.cpe`. The nix closure tier does not read it. waybill's curated table already reproduces most of these exactly, so the gap is narrow, but it includes the package being scanned (see the companion issue on the document subject) and one version encoding that differs.

## Measurement

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin.

`meta.identifiers` at that revision:

| package | `identifiers.cpe` |
| --- | --- |
| openssh | `cpe:2.3:a:openbsd:openssh:10.3p1:*:*:*:*:*:*:*` |
| curl | `cpe:2.3:a:haxx:curl:8.20.0:*:*:*:*:*:*:*` |
| openssl | `cpe:2.3:a:openssl:openssl:3.6.2:*:*:*:*:*:*:*` |
| zlib | `cpe:2.3:a:zlib:zlib:1.3.2:*:*:*:*:*:*:*` |
| python313, httpie | none — `cpeParts` carries a product but no vendor |

Across the buildtime closure of `openssh` (`--nix-closure` on a wrapper flake), 16 components carry a nixpkgs-declared CPE. Against waybill's output:

- **14** — waybill emits the identical CPE string.
- **1 differs** — bash: waybill `cpe:2.3:a:gnu:bash:5.3p9:*:…`, nixpkgs `cpe:2.3:a:gnu:bash:5.3:9:…` (version `5.3`, update `9`).
- **1 missing** — gnugrep has no CPE in waybill's output; nixpkgs declares `gnu:grep`.

Plus the scanned package itself, which waybill emits without a CPE.

For comparison, sbomnix 1.9.0 places a CPE on 662 of 731 components in the same closure, but with `--exclude-cpe-matching` (nixpkgs-declared only) it keeps 17. The other 645 are synthesised from the package name, including `filename:filename` CPEs for every source tarball and patch file.

## Expected

Prefer the nixpkgs-declared CPE where one exists, record which source a component's CPE came from, and keep the curated table as the fallback for packages nixpkgs does not identify (python3 is one; see the companion issue).

*Not verified:* whether the meta evaluation milestone 1281 already performs for licenses fetches `identifiers` in the same call. *Inference, not measured:* the `5.3p9` version string probably falls outside NVD's bash version ranges, which encode the patch level separately.

`meta.identifiers` also carries `purls` / `purlParts`, empty for these packages at this revision; that bears on #952.

Refs #952

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
