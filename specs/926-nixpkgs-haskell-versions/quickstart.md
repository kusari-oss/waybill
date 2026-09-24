# Quickstart — nixpkgs-resolved Haskell versions (#947)

## What changes for an operator

Scanning a Haskell repository that builds through Nix and ships no Haskell
lockfile:

```bash
waybill sbom scan --path . --output sbom.cdx.json
```

Before: every dependency versionless at design tier.
After: dependencies the pinned nixpkgs revision carries have an exact version
and a SHA-256 of their source tarball; the rest stay versionless with a reason.

No flag required — resolution is on by default (FR-015), and fires only when
the repository both pins a nixpkgs-shaped input and declares Haskell
dependencies (C1).

Opting out:

```bash
waybill sbom scan --path . --offline                  # all network enrichment off
waybill sbom scan --path . --no-<resolution-flag>     # this feature only (FR-015a)
```

---

## Reproducing the research

Everything in `research.md` re-runs from the committed probe:

```bash
cd specs/926-nixpkgs-haskell-versions/measurements
python3 probe_nixpkgs_haskell.py                    # default revision + GHC 9.6.x
python3 probe_nixpkgs_haskell.py --series 9.10.x    # a different compiler series
python3 probe_nixpkgs_haskell.py --json             # machine-readable
```

Expected against `a799d3e3886da994fa307f817a6bc705ae538eeb`: 16,634,427 bytes,
19,058 unique package names, warm fetch ~1 s.

The probe treats any nulled name as compiler-supplied (research R3). Two
narrower rules were tried and rejected by measurement: nesting depth drops
`directory-ospath-streaming`, and package-set membership drops `rts`,
`ghc-platform`, `ghc-toolchain` and `system-cxx-std-lib` — all real packages
the compiler supplies.

---

## Verifying the source hash is native, not an annotation

The single most load-bearing check in this feature (research R2). If it ever
stops holding, the hash must move out of the native checksum field:

```bash
# 1. take a package's sha256 from the pinned revision's package set
#    e.g. th-compat 0.1.7 -> "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly"
# 2. decode Nix base32 -> hex (32 bytes expected)
# 3. compare against the tarball's own digest:
curl -sS -o t.tar.gz https://hackage.haskell.org/package/th-compat-0.1.7/th-compat-0.1.7.tar.gz
shasum -a 256 t.tar.gz
# expect: 9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff
```

They must match. A mismatch means the value is not a flat hash of the source
bytes, and emitting it as a native SHA-256 would be a false claim.

---

## Test scenarios

| scenario | fixture shape | asserts |
|---|---|---|
| resolves | `flake.lock` pinning a revision + `.cabal` with ranges, no freeze | version + native hash present; provenance names the revision |
| boot library honest | declares an ordinary dep and a boot library | first has a version, second has none and reason `compiler-supplied` |
| never invents | any unresolved dependency | no version, no hash, no versioned identifier (SC-003) |
| no flake | no `flake.lock` | byte-identical to pre-feature output (SC-007) |
| moving ref | lock pins a branch, not a revision | no retrieval; reason `no-exact-revision` |
| unreachable / private | lock names an unreachable host | counts identical to pre-feature; reason `source-unreachable`; no prompt; within the time bound (SC-006, SC-008) |
| determinism | same repo, same revision, twice | byte-identical (SC-004) |
| cache | second scan of a known revision | no retrieval (SC-005) |
| no Haskell | repo with a flake but no Haskell deps | no retrieval at all (SC-009) |
| local wins | freeze file and nixpkgs disagree | local value emitted, disagreement recorded (C6) |

Fixtures are offline: the retrieval boundary is injected so tests assert
behaviour without network, matching the hermetic posture of the existing
reader suites.

---

## Gotchas for the implementer

- **Nix base32 is not RFC 4648.** Custom alphabet `0123456789abcdfghijklmnpqrsvwxyz`
  (no `e`, `o`, `u`, `t`) and reversed bit order. 52 chars → 32 bytes.
- **The attribute name is usually unquoted** — `th-compat = callPackage`, quoted
  only when the name is not a valid Nix identifier. A parser keyed on
  `"name" =` finds almost nothing.
- **A name can be defined more than once** — 19,437 blocks, 19,058 unique names.
  Last definition wins, per Nix attrset semantics.
- **Do not try to narrow the nulled set.** Neither nesting depth nor
  package-set membership separates a derivation attribute from a real boot
  library; both drop real packages (R3). Over-including withholds a version;
  under-including invents one, which Principle IX forbids — so the rule must
  fail toward over-inclusion, and the unfiltered set is that rule.
- **Do not assume upstream nixpkgs.** The retrieval target comes from the lock
  entry; it may be a fork or an internal mirror (FR-016).
