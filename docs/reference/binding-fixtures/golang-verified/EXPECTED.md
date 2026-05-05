# golang-verified — `strength: verified`

Reference fixture for the milestone-072 binding-hash-v1 algorithm
contract — Go ecosystem variant. All three input sides populated:
VCS commit (`git rev-parse HEAD` for source-tier scans, Go BuildInfo
`vcs.revision` for binary-tier scans) + `go.sum` SHA-256 + `go.mod`
SHA-256.

## Canonical input triple

| Side | Value |
|---|---|
| `vcs` | `deadbeef0123456789abcdef0123456789abcdef` |
| `lockfile` | `4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e` |
| `manifest` | `8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da` |

(Same canonical input substrate as the cargo-verified fixture so
external verifier authors don't need a second round of byte-string
substrates to recreate; the binding hash is ecosystem-agnostic at the
algorithm level — only the per-ecosystem extraction sites differ per
`contracts/binding-hash-v1.md` C-7.)

## Canonical envelope (C-2)

```text
{"algo":"v1","lockfile":"4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e","manifest":"8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da","vcs":"deadbeef0123456789abcdef0123456789abcdef"}
```

## Expected SHA-256 (C-3)

```text
745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111
```

## Strength derivation (C-4)

- `populated_count == 3`
- All three sides match
- → `strength: "verified"`

## Per-ecosystem extraction notes (C-7 golang)

- `vcs`: `git rev-parse HEAD` (source-tier) OR Go BuildInfo
  `vcs.revision` (binary-tier).
- `lockfile`: SHA-256 of `go.sum` bytes.
- `manifest`: SHA-256 of `go.mod` bytes.
