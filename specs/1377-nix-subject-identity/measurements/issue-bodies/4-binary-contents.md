The nix closure tier enumerates derivations, so the Go modules and Rust crates compiled into a Nix-built binary never appear. For a Go or Rust program they are most of its vulnerability surface. waybill's binary readers already recover them from the built output, but the closure path never looks at the output.

## Measurement

nixpkgs `a799d3e3886da994fa307f817a6bc705ae538eeb`, aarch64-darwin, waybill `main@72b455e2`, grype 0.120.0. Targets: wrapper flakes exposing `nixpkgs#gh` (2.93.0, Go) and `nixpkgs#ripgrep` (15.1.0, Rust).

| | `pkg:golang` (gh) | `pkg:cargo` (ripgrep) | grype findings on modules/crates |
| --- | --- | --- | --- |
| waybill `--nix-closure` | 0 | 0 | 0 |
| sbomnix 1.9.0, buildtime and runtime | 0 | 0 | 0 |
| waybill `sbom scan --path <realised output>` | **163** | 45 components read via cargo-auditable | gh: **55** |
| grype `dir:<realised output>` (reference) | — | — | gh: 40, on 10 packages |

In both closure SBOMs the Go dependencies exist only as one opaque `gh-2.93.0-go-modules` derivation, and grype matched nothing on it or on the Go toolchain. The 55 findings are not adjudicated. The ripgrep row stops at reading: emission then hits the debug-build panic filed separately.

## Expected

When a closure member's output is realised in the store, waybill reads its embedded build information with the existing binary readers and attaches the recovered modules or crates as that package's dependencies.

Whether to realise outputs that are not yet present, and how this interacts with runtime vs buildtime scope, is a design question for #1034. Scanning built outputs is also part of what #431 asks for.

Refs #431, #1034, #1245

---
*Drafted with AI assistance (Claude), reviewed and filed by me. Measurements quoted were executed and verified.*
