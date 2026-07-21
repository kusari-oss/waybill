# Binding-hash reference fixtures (milestone 072 / T028b)

External-verifier reference fixtures for the milestone-072 cross-tier
SBOM binding contract (`docs/reference/cross-tier-binding.md`,
`specs/072-cross-tier-sbom-binding/contracts/binding-hash-v1.md`).

Three fixture pairs cover the three `BindingStrength` outcomes:

- **`cargo-verified/`** — cargo project with `Cargo.toml` + `Cargo.lock` +
  pinned VCS commit. Expected `strength: verified` (all three input
  sides populated).
- **`golang-verified/`** — Go module with `go.mod` + `go.sum` + pinned
  VCS commit. Expected `strength: verified`.
- **`maven-weak/`** — Maven project with `pom.xml` + pinned VCS commit
  but NO lockfile (Maven has no canonical lockfile per
  `contracts/binding-hash-v1.md` C-7). Expected `strength: weak`.

Each fixture directory contains:

- `source.cdx.json` — the source-tier SBOM (CDX 1.6 JSON) with the
  expected `waybill:source-document-binding` annotation pre-pinned on
  the main-module component.
- `image.cdx.json` — the matching image-tier SBOM whose binding asserts
  the same hash. Running `waybill sbom verify-binding --image-sbom
  image.cdx.json --source-sbom source.cdx.json` against any
  alpha.15+ build MUST produce a clean verify (exit 0).
- `EXPECTED.md` — the canonical input triple `(vcs, lockfile, manifest)`
  + the expected SHA-256 hex output.

The pinned hex values match the `pinned_vec_*` unit tests in
`waybill-cli/src/binding/hash.rs::tests` — single source of truth.
External verifier authors writing a compatibility implementation use
these to validate against waybill's emission per SC-004.

## Algorithm version

All fixtures use `algo: "v1"`. Future v2-bumps add fixtures under
`docs/reference/binding-fixtures-v2/` in parallel with v1 — see
`contracts/binding-hash-v1.md` C-6.
