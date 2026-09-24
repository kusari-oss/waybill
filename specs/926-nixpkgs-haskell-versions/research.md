# Phase 0 Research — nixpkgs-resolved Haskell versions (#947)

Every finding here is an observation against nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`, taken 2026-09-24. The probe is
committed at `measurements/probe_nixpkgs_haskell.py`; figures introduced by
this document are reproducible by the commands quoted beside them.

---

## R1 — Retrieval mechanism and cost

**Decision**: Fetch the generated package-set file over HTTPS from the source
the lock entry names. Do not shell out to `nix`.

**Rationale**: Constitution Principle I (pure Rust, statically linked). A host
`nix` would be exact but adds a tool dependency and produces behaviour only
some users can reproduce. #947 raises both options; the measurement below
removes the main argument for the heavier one.

**Measured**:

| quantity | observed |
|---|---|
| `hackage-packages.nix` size | 16,634,427 bytes |
| fetch, warm connection | **1.05 s / 0.99 s / 1.02 s** (3 runs) |
| parse to (name → version, sha256) | **0.01 s** (Python regex; Rust will be faster) |
| derivation blocks matched | 19,437 |
| unique package names | 19,058 (a name may be defined more than once; last wins) |

**Consequence for FR-015 (default-on)**: a ~1 s one-off per revision, cached,
is a different cost shape from the sequential-request problem in #930. The
measurement supports the default-on decision rather than merely permitting it.

**Alternatives rejected**:
- `nix eval` against the pinned revision — exact, but host-tool dependency.
- Fetching a repository tarball — far larger for one file.

---

## R2 — The source hash is a real SHA-256 of the tarball, so it is native

**Decision**: Convert the nixpkgs `sha256` from Nix base32 to hex and emit it
in the **native** checksum field of each format (CDX `hashes[]` with `alg:
SHA-256`, SPDX 2.3 `checksums[]` with `algorithm: SHA256`, SPDX 3
`software_ContentIdentifier`). Do **not** invent an annotation for it.

**This was not assumed — it was verified end to end.**

```
nixpkgs value : "1zym9yia0is8wxfd6d1ldwvvghwxg4ww3y4lrxnyb2nk60ig29ly"  (52 chars)
Nix base32    -> 32 bytes = 9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff
sha256(hackage th-compat-0.1.7.tar.gz, 14,763 bytes)
              = 9e26f12230d38ae56dcf94f8c139799dc3b7376f3434d35ce74847a0a24fd5ff   ✅ identical
```

**Why this mattered enough to check**: milestone 925 hit the opposite case.
A flake input's `narHash` is SRI base64 over a NAR **serialization of a
directory tree**, which is not a hash of any file's bytes, so it had no native
carrier and became annotation C165. Width alone proves nothing: a 32-byte
digest can be over a NAR just as easily as over a file. Here the digest is
flat over the source tarball, so Principle V (standards-native first) requires
the native field and forbids a new `waybill:` key.

**Consequence**: FR-002 is satisfied by native fields. No new catalog row is
needed for the hash, which removes three extractors and a parity row from the
work.

---

## R3 — A nulled name is compiler-supplied, and that is the whole rule

**Decision**: Treat any name bound to `null` in a candidate compiler
configuration as compiler-supplied. Do not filter that set further.

**Two more discriminating rules were implemented and both were rejected by
measurement.** The record is kept because each looks obviously right, and
because both failed in the same direction.

The naive extraction also matches `editedCabalFile`, an attribute inside a
derivation override rather than a package. Two attempts were made to exclude
it.

### Attempt 1 — attribute-set nesting depth

Keep only the shallowest null bindings. Measured across three GHC series:

| series | removed by the depth rule | correct? |
|---|---|---|
| 9.6.x | `editedCabalFile` | ✅ not a package |
| 9.4.x | `directory-ospath-streaming` | ❌ **a real package, v0.3** |
| 9.10.x | nothing | ✅ |

Real boot libraries also live at deeper nesting, inside conditional attribute
sets. Depth does not separate the cases.

### Attempt 2 — package-set membership

Keep a nulled name only when it is also a package in `hackage-packages.nix`.
This excludes `editedCabalFile` correctly and is still wrong. Measured across
all eight series at this revision, the nulled names absent from the package
set are exactly five:

| name | a real Haskell package? | excluding it would be |
|---|---|---|
| `editedCabalFile` | no — a derivation attribute | correct |
| `rts` | **yes** — the GHC runtime system | **wrong** |
| `ghc-platform` | **yes** — GHC-bundled | **wrong** |
| `ghc-toolchain` | **yes** — GHC-bundled | **wrong** |
| `system-cxx-std-lib` | **yes** — GHC-bundled | **wrong** |

Those four are absent from `hackage-packages.nix` *precisely because* they are
never built from Hackage — they only ever come from the compiler, which is the
definition of a boot library. Excluding them would report
`absent-from-package-set` for a dependency whose true reason is
`compiler-supplied`: no invented version, but a false statement in the emitted
document (Principle X).

### What the rule actually is

Nulled means compiler-supplied. `editedCabalFile` stays in the set and is
**inert**, because the set is only ever consulted for names the project
declared, and `editedCabalFile` is not a legal Haskell package name. A false
positive that nothing can ever query costs nothing — and both attempts to
remove it cost real packages.

**The asymmetry that governs every version of this rule.** Over-including a
boot library withholds a version: lossy, and visible to the operator as a
reason code. Under-including one lets a package the compiler supplies resolve
to a Hackage version the build never uses — an invented version, which
Principle IX forbids. Both rejected attempts failed toward under-inclusion.

**Consequence**: FR-004 is satisfied by reading the nulled set, with no
further filtering, no Nix nesting parser and no dependency on the package-set
parser. This is the simplest of the three designs and the only correct one.

---

## R4 — Candidate compiler sets, and what the conservative rule costs

**Decision**: Determine candidate compiler package sets by scanning the
project's flake for explicit `haskell.packages.ghc<NN>` attribute paths. When
none is found, fall back to treating **every** GHC series present at the
pinned revision as a candidate. Record which path was taken (FR-014b).

**Rationale**: `flake.lock` does not record which package set was built, and
`flake.nix` is a program, not data. An explicit attribute path is a
high-precision textual signal; anything more requires evaluating Nix, which R1
rules out.

**Measured — the conservative rule has a real but bounded cost.** Nulled-set
sizes at this revision:

| series | nulled |
|---|---|
| 9.0.x | 35 |
| 9.4.x | 40 |
| 9.6.x | 40 |
| 9.8.x | 41 |
| 9.10.x | 41 |
| 9.12.x | 44 |
| 9.14.x | 46 |
| 9.16.x | 47 |

| candidate set | union | intersection | differ |
|---|---|---|---|
| all 8 series (no flake signal) | 50 | 32 | **18** |
| the 3 the target's flake names | 44 | 38 | **6** raw, of which `editedCabalFile` is not a package (R3) → **5 genuine** |

So FR-014a's "resolve only what is non-boot in *every* candidate" costs **5
packages** when the flake scan succeeds and **18** when it does not. That is
the price of not inventing a version, and it is worth the scan being decent.

**Alternatives rejected**:
- Resolve against the default `haskellPackages` set — FR-014c forbids it; #947
  warns the default set "is not what this project builds against".
- Evaluate the flake to learn the compiler — Principle I.

---

## R5 — Caching

**Decision**: Reuse the established per-pinned-SHA cache layout — m090 fixture
cache, m108 fingerprint cache, m195 corpus cache all key a local directory by
an immutable SHA. Cache at `~/.cache/waybill/nixpkgs/<rev>/`.

**Rationale**: a pinned revision is immutable, so the cache needs **no TTL and
no invalidation** — unlike m110, which needed a 24-hour TTL because its source
was mutable. FR-010 and SC-005 fall out of the layout rather than needing
mechanism.

---

## R6 — Gating, so the default-on path costs nothing elsewhere

**Decision**: Resolution runs only when the m925 `flake.lock` reader reports an
input resolvable to an exact revision whose shape is nixpkgs-like, **and** the
Haskell reader produced at least one declared dependency. Reuse the m925
lockfile module (`scan_fs/package_db/nix/lockfile.rs`) rather than re-reading
the file.

**Rationale**: FR-015 makes this default-on; SC-009 requires that a repository
meeting neither condition performs no retrieval. Gating on both conditions
keeps the blast radius to projects that benefit, which was the argument the
default-on decision rested on.

**Note**: m925's `OriginalPinState` already distinguishes `Exact` from
`NamedRef`/`DefaultBranch`, which is exactly the FR-012 "pins no exact
revision" case. No new lockfile analysis is required.

---

## R7 — Retrieval time bound (FR-019)

**Decision**: Default bound **30 seconds**, operator-overridable.

**Anchored to measurement, and labelled as what it is**: the observed warm
fetch is ~1.0 s (R1). 30 s is **30× that observation**, chosen as headroom for
slow, proxied, or internal-mirror links — it is a *budget*, not a measured
figure, and this document does not present it as one. The number that is
measured is 1.0 s; the multiplier is a judgement about tail latency on links
that have not been measured because they are operator-specific.

Revisit if a real internal-mirror latency measurement becomes available.

---

## R8 — Private and internal sources (FR-016 – FR-019)

**Decision**: Build the retrieval URL from the lock entry's recorded location.
Treat any of {unreachable, refused, unauthorized, timed out} as the FR-008
degraded path with a reason code distinct from "reached but had no such
package set". Never prompt; never retry in a way that blocks the scan.

**Rationale**: raised during clarification. A `flake.lock` may pin a fork, an
internal mirror, or a self-hosted forge. An internal mirror of nixpkgs carries
the **same file layout**, so resolution works against it whenever it is
reachable — the failure mode is egress and authentication, not format. This is
why FR-016 derives the target rather than assuming `NixOS/nixpkgs`.

**Existing machinery**: the OCI registry path already resolves credentials
(m034) and handles TLS flexibility (m182). This feature does **not** need to
reuse that; per FR-018 it does not authenticate at all, it degrades. Recording
the decision so a future reader does not assume the omission was an oversight.

---

## Open items deliberately left to implementation

- Exact reason-code strings — plan-level naming, constrained by FR-006/FR-017
  to distinguish: compiler-supplied; absent from package set; source
  unreachable/unauthorized.
- Whether the parser streams or buffers the 16 MB file. Buffering measured at
  0.01 s to parse, so this is an ergonomics choice, not a performance one.
