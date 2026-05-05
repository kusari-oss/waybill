# Contract — Binding Hash algorithm v1

This contract is what a third-party verifier needs to recompute a mikebom binding hash and compare it against an emitted assertion. Implementations in any language MUST produce byte-identical output for byte-identical inputs.

## C-1 — Inputs

The binding hash input is a triple `(vcs, lockfile, manifest)` where each side is optional. Inputs are sourced per-ecosystem per the table in research.md §1.

| Side | Type | When populated |
|---|---|---|
| `vcs` | `string` (commit identifier, typically 40-char SHA-1 hex but tolerant of any string) or `null` | Source tree is in a recognized VCS checkout (git's `git rev-parse HEAD`); OR binary embeds VCS metadata (Go BuildInfo `vcs.revision`, cargo-auditable embedded metadata) |
| `lockfile` | `string` (lowercase hex SHA-256 of lockfile bytes) or `null` | Project has a recognized lockfile per the ecosystem-specific list in research.md §1 |
| `manifest` | `string` (lowercase hex SHA-256 of canonical manifest bytes) or `null` | Project has a recognized top-level manifest |

**Manifest canonicalization rule**: SHA-256 the raw bytes of the manifest file as on disk. mikebom does NOT re-serialize / re-format the manifest before hashing — bytes-on-disk is the contract. (Rationale: avoids subtle whitespace + parser-version drift between mikebom and external verifiers; the manifest's canonical form is "what the maintainer committed.") Exception: maven `pom.xml` after parent inheritance + property substitution is a *resolved* form mikebom must emit canonically; that's a known wart, captured separately under "Per-ecosystem details" below.

**Lockfile canonicalization rule**: Same — SHA-256 the raw bytes as on disk.

## C-2 — Canonical JSON envelope

The triple is wrapped in a JSON object whose shape is **fixed**:

```json
{
  "algo": "v1",
  "lockfile": "<sha256-or-null>",
  "manifest": "<sha256-or-null>",
  "vcs": "<commit-or-null>"
}
```

**Fixed shape rules**:

- Exactly four keys: `algo`, `lockfile`, `manifest`, `vcs`. No more, no fewer.
- Keys appear in lexicographic order: `algo`, `lockfile`, `manifest`, `vcs`.
- `algo` value is the literal string `"v1"` for this contract version.
- `lockfile`, `manifest`, `vcs` values are either a string OR JSON null (NOT empty string, NOT missing key).
- No whitespace between tokens. Equivalent to `serde_json::to_string` (compact form) on a `BTreeMap<&str, Option<String>>`.

**Serialization output** (worked example, all three sides populated):

```text
{"algo":"v1","lockfile":"a1b2c3d4...","manifest":"e5f6a7b8...","vcs":"deadbeef0123..."}
```

**Serialization output** (worked example, only manifest populated):

```text
{"algo":"v1","lockfile":null,"manifest":"e5f6a7b8...","vcs":null}
```

## C-3 — Hash computation

```text
binding_hash = sha256(utf8(canonical_envelope_string))
```

Output is the **lowercase hex** representation of the SHA-256 digest, 64 characters. No prefix (`sha256:` is NOT prepended). Examples elsewhere in mikebom that already follow this convention: `mikebom-cli/src/sbom/mutator.rs::attestation_sha256` (line 45).

## C-4 — Strength derivation

After emission, the binding hash is paired with a `BindingStrength` label per FR-012:

| populated_count | match against source-tier recomputation | strength |
|---|---|---|
| 3 | all three match | `verified` |
| 2 | both populated sides match | `weak` |
| any | any populated side fails to match | `unknown` |
| < 2 | (insufficient evidence) | `unknown` (with `reason: "no-evidence"` or similar) |

A verifier confirms strength by:

1. Reading the emitted `SourceDocumentBinding.hash` and `SourceDocumentBinding.strength` from the target SBOM.
2. Recomputing the binding hash from the source-tier SBOM (which carries the project-tier `BindingHashInputs` directly or implicitly via the source-tier component's evidence).
3. Comparing the recomputed hash against the emitted hash.

Match → strength as labeled. Mismatch → strength `unknown` with reason `verification-failed` regardless of what the emitter labeled.

## C-5 — Determinism contract

For byte-identical `(vcs, lockfile, manifest)` inputs, two distinct mikebom invocations on potentially different machines / different alpha versions MUST produce byte-identical binding hashes. Determinism breaks ⇒ contract violation.

Specifically:

- Hash algorithm: SHA-256 (RFC 6234). Any conforming implementation works.
- Hex encoding: lowercase, no separators, no prefix. (Use `data-encoding::HEXLOWER` or equivalent.)
- JSON serializer: any RFC 8259-conformant compact serializer that produces sorted-key objects. The reference implementation uses Rust's `serde_json::to_string` over a `BTreeMap`.

## C-6 — Algorithm versioning

The `algo: "v1"` field in the envelope is mandatory and a fixed string for this contract. Future versions (V2, V3, ...) MUST:

- Bump the `algo` value to `"v2"` etc.
- Be specified in a separate contract document (`binding-hash-v2.md`).
- Be emitted in parallel with V1 for at least one mikebom milestone (so consumers have a deprecation window).
- Treat unknown `algo` values from external sources as "cannot verify" (`unknown` strength, reason `unsupported-algo`) rather than failing.

## C-7 — Per-ecosystem details

Specific mappings each verifier MUST follow per ecosystem to extract the input triple. Mismatch with mikebom's emit-side mapping → false-negative verification.

### golang

- **vcs**: Go BuildInfo `vcs.revision` for binary-tier scans (`mikebom-cli/src/scan_fs/package_db/go_binary.rs:66+`); `git rev-parse HEAD` from the source tree's git checkout for source-tier scans.
- **lockfile**: SHA-256 of `go.sum` bytes.
- **manifest**: SHA-256 of `go.mod` bytes.

### cargo

- **vcs**: cargo-auditable embedded VCS metadata for binary-tier scans; `git rev-parse HEAD` from source-tree's git checkout otherwise.
- **lockfile**: SHA-256 of top-level `Cargo.lock` bytes.
- **manifest**: SHA-256 of top-level `Cargo.toml` bytes (workspace root, NOT individual crate manifests).

### npm

- **vcs**: `git rev-parse HEAD` (no widespread binary-embed convention).
- **lockfile**: SHA-256 of `package-lock.json` bytes; fallback to `yarn.lock` then `pnpm-lock.yaml` if the canonical lockfile is absent.
- **manifest**: SHA-256 of top-level `package.json` bytes.

### pip

- **vcs**: `git rev-parse HEAD`.
- **lockfile**: SHA-256 of `poetry.lock` (Poetry projects); fallback to `pdm.lock` (PDM); fallback to a SHA-256 of the concatenated `--hash=` lines from `requirements*.txt` (PEP 503 sorted alphabetically).
- **manifest**: SHA-256 of top-level `pyproject.toml` bytes.

### gem

- **vcs**: `git rev-parse HEAD`.
- **lockfile**: SHA-256 of `Gemfile.lock` bytes.
- **manifest**: SHA-256 of top-level `*.gemspec` bytes (project's own gemspec, NOT vendored gemspecs).

### maven

- **vcs**: `git rev-parse HEAD`. (Future: `<scm>` block in pom.xml.)
- **lockfile**: NOT POPULATED. Maven has no canonical lockfile in mikebom's milestone-070 emission pattern. Strength capped at `weak` for maven projects until a content-hash sidecar (e.g., reproducible-build expected-output hash) lands as a future milestone.
- **manifest**: SHA-256 of top-level `pom.xml` bytes (resolved form after parent inheritance + property substitution per milestone 070).
