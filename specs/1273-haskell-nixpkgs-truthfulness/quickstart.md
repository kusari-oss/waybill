# Quickstart: Haskell/nixpkgs resolution correctness

**Milestone**: 1273 | **Spec**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)

How to reproduce each of the four defects, and how to check a fix.

## Prerequisites

```sh
cargo build -p waybill                       # target/debug/waybill
```

Three of the four defects need a cached nixpkgs revision. Both revisions
these probes use are already in `~/.cache/waybill/nixpkgs/`, so **every
command below runs `--offline`**:

| revision | used by |
|---|---|
| `a799d3e3886da994fa307f817a6bc705ae538eeb` | `reproduce-1273.sh`, `license-shapes.sh` |
| `5ae3b07d8d6527c42f17c876e404993199144b6a` | `sc003-public-library.sh` (the library's own lock) |

**The test suite needs neither.** Only the probes read the shared cache, and
only read-only. A test that read `~/.cache/waybill/nixpkgs/` would pass here
and fail on CI, which has no such cache — so tests seed a
`tempfile::tempdir()` with a synthetic package set instead, per `seed_cache()`
at `waybill-cli/tests/nix_haskell_resolution_m926.rs:116`.

`hpack` is **not** a prerequisite for waybill or for its tests — FR-021 and
FR-022 forbid depending on it. It is needed only to re-run the semantics
probe:

```sh
nix build --no-link --print-out-paths nixpkgs#haskellPackages.hpack
```

## Reproduce all four defects

```sh
./specs/1273-haskell-nixpkgs-truthfulness/measurements/reproduce-1273.sh
```

Prints, at the current binary:

| issue | expected while the defect stands |
|---|---|
| #1278 | `hackage components: 0` for a manifest declaring 3; `graph-completeness: complete`; `diagnostic naming package.yaml: ABSENT` |
| #1281 | `aeson`/`async` go from `lifecycle=optional` to runtime once `flake.lock` is added; `declared=5`, `restored=2` |
| #1280 | `carrying a license: 0/66` |
| #1279 | `RESOLVED *and* still unresolved: 2` |

## Per-story checks

### US1 — hpack-only (#1278)

```sh
./target/debug/waybill --offline sbom scan --path <tree-with-package.yaml-only> \
  --no-deep-hash --format cyclonedx-json --output cyclonedx-json=out.json
jq '[.components[] | select(.purl | startswith("pkg:hackage/"))] | length' out.json
jq -r '.metadata.properties[] | select(.name=="waybill:graph-completeness") | .value' out.json
```

The reader is already dispatched to the file — `RUST_LOG=info` shows
`per_reader_dispatch_counts{"haskell": 1}`. It sees the manifest and says
nothing, which is sharper than #1278 states and is why C8 asserts the
diagnostic rather than only the component count.

**The corpus has a live witness.** `nix-closure-moat`
(`MercuryTechnologies/moat` @ `d0690555`) has a root `package.yaml` with no
sibling `*.cabal`, and its committed golden records 321 hackage components
derived without reading it.

### US2 — the optional mark (#1281)

```sh
./specs/1273-haskell-nixpkgs-truthfulness/measurements/sc003-public-library.sh
```

Scans the public library #955 was filed against, with and without its
`flake.lock`, and reports both the classification disagreement and the
reachability fan-out. SC-003's figure is **10** — the 2 marked components
plus 8 non-boot components reachable from the root only through them.

Two traps this probe exists to avoid:

- **Comparing component counts between the two cases proves nothing.** Case
  A emits 44 hackage components and case B 190, because the closure pass
  runs only in case B. The fan-out components are *introduced* by the
  closure, not *re-classified* by the withdrawal.
- **A boot library must not be counted.** `pretty` is reachable only
  through the marked pair, but nixpkgs binds it to `null` in
  `configuration-ghc-9.10.x.nix`, so it is compiler-supplied and present in
  every build. `fanout.py` reads that binding from the same per-revision
  cache the scan used, so the exclusion cannot drift from the revision under
  measurement.

### US3 — licenses (#1280)

```sh
./specs/1273-haskell-nixpkgs-truthfulness/measurements/license-shapes.sh
```

Offline; reads only the cached `hackage-packages.nix`. Prints the three
declaration families, the full 15-name attribute vocabulary, and every
bare-string value. The three family counts must sum to the declaration
count — if they do not, the file has a shape the parser does not model.

Then, on any resolved document:

```sh
jq '[.components[] | select(.purl | startswith("pkg:hackage/")) | select(.licenses != null)] | length' out.json
```

### US4 — unresolved-reason (#1279)

```sh
jq '[.components[]
     | select(.version != null and .version != "")
     | select(any(.properties[]?; .name == "waybill:unresolved-reason"))] | length' out.json
```

Must be `0`. Measured today: `2`, and those 2 are the same 2 US2 is about —
so **measure US4's remaining population after US2 lands, not before.** The
62 components carrying no reason are not a defect: the pass resolved them,
so FR-013 says they must not carry one.

## hpack semantics (FR-001a's measurement obligation)

```sh
HPACK=$(nix build --no-link --print-out-paths nixpkgs#haskellPackages.hpack)/bin/hpack \
  ./specs/1273-haskell-nixpkgs-truthfulness/measurements/hpack-semantics.sh
```

Twelve cases, printing each rendered stanza verbatim. The three results
that constrain the implementation:

- **A1** top-level `dependencies` reaches `library`, `executables`, `tests`
  **and** `benchmarks`; a per-component list is additive.
- **A3** for a repeated name the **last** declaration wins and replaces the
  earlier constraint — no intersection, measured both across the
  inheritance boundary and within one list.
- **A5** `when:` is not evaluated except for a literal `false`; `os(...)`
  and `flag(...)` lower to a Cabal `if`. So a `when:` block *is* a Cabal
  conditional, and the existing flag-default evaluator is the right one.

**Read the raw output, not a filter over it.** The probe's first version
filtered the rendered `.cabal` through awk matching `^  build-depends:` and
its `^    ` continuations. A Cabal `if` block nests its own `build-depends`
one level deeper, so every conditional dependency was invisible — and the
three `when:` cases, whose whole subject is the conditional, printed
*identically*. Cases that must differ reading the same is the signature of
a broken harness, not of a uniform system.

## The gate

```sh
./scripts/gate.sh --expect '>>> all pre-PR checks passed\.' prepr -- ./scripts/pre-pr.sh
```

No `+toolchain` argument (#1189). Assert on the positive signal, never on
the absence of `FAILED` — a build that never compiled also contains no
failures.

## Corpus goldens

Three of the four Haskell/Nix corpus targets are expected to change, and
one is a control:

| target | expected |
|---|---|
| `haskell-aeson` | **unchanged** — no `flake.lock`, no `package.yaml` at its pin |
| `haskell-security-advisories` | changed by US3; possibly US2/US4 |
| `haskell-language-server` | changed; its one `package.yaml` is test data **with** a sibling `.cabal`, so it exercises FR-002's suppression |
| `nix-closure-moat` | changed by all four stories |

A refresh that changes `haskell-aeson` is evidence of unintended blast
radius, not of this milestone working (C27).

The goldens are `#[ignore]`d locally and `public-corpus.yml` runs only on
`schedule` and `workflow_dispatch`, never on a PR, so this churn surfaces
as a red nightly unless driven deliberately.
**Never generate corpus goldens locally**; the refresh goes through the
workflow dispatch, and `WAYBILL_UPDATE_PUBLIC_CORPUS_GOLDENS=1` is never
set locally.
