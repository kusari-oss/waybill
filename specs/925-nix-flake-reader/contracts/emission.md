# Contract: what a scan emits for a flake.lock

The interface this feature exposes is the emitted SBOM. This contract states
what a consumer may rely on, in terms observable from the document alone.

## C-1 — One component per identifiable locked input

For every node in `nodes` that has a `locked` reference with a `rev`, and whose
type is not `path` or `indirect`, the document contains exactly one component.

Not emitted:
- the root node (it is the project, not an input)
- `path` inputs — local directories with no published upstream identity (FR-003)
- `indirect` inputs — registry-resolved, no upstream identity in the lockfile
- `follows` aliases — they name an existing pin, not a new one (FR-004)

## C-2 — Identifier shape

| locked type | identifier |
|---|---|
| `github` | `pkg:github/<owner>/<repo>@<rev>` |
| `gitlab` | `pkg:gitlab/<owner>/<repo>@<rev>` |
| `sourcehut` | `pkg:sourcehut/<owner>/<repo>@<rev>` |
| `tarball`, `git`, other | `pkg:generic/<name>@<rev>` + source-location annotation |

A consumer may rely on every identifier being purl-spec conformant. No
`pkg:nix` identifier is emitted (FR-013c).

## C-3 — The revision is the version

A component's version is the locked revision verbatim. It is not truncated, not
normalised, and not replaced by `lastModified`.

## C-4 — The NAR hash is not a checksum

The document MUST NOT contain a native checksum field populated from `narHash`.
The value appears only in its annotation, which names what it covers.

A consumer verifying native checksums will find none for these components. That
is deliberate: the available hash covers a NAR serialization of a directory
tree, not the component's bytes (FR-009).

## C-5 — Reachability

Every emitted input component is reachable from the document root by walking
dependency edges (FR-007, SC-003). Inputs declared by other inputs are expressed
as edges between those components, not re-parented to the root (FR-008).

## C-6 — The original reference, when it differs

When an input's `original` differs from its `locked` reference, the document
records the original. When they agree, nothing is emitted for it (FR-006).

## C-7 — Failure is visible and contained

A `flake.lock` that is malformed, or carries an unrecognised schema version,
emits no components, logs a warning naming the file, and leaves every other
ecosystem's output unchanged (FR-010, SC-007).

A flake with no lockfile emits no input components and records why (FR-012).

## C-8 — No external dependency

Emission requires no network and no Nix installation. Output is identical
offline and online (SC-002), and byte-identical across repeated scans of an
unchanged tree (SC-006).

## C-9 — Per-directory scope

A `flake.lock` governs inputs for the directory that contains it. A repository
with several independent flakes yields several independent input sets; one
lockfile never speaks for another directory (FR-011).

This is the rule #938 established for Haskell lockfiles, applied here from the
start rather than after a defect.
