# cargo-verified — `strength: verified`

Reference fixture for the milestone-072 binding-hash-v1 algorithm
contract (`contracts/binding-hash-v1.md`). All three input sides are
populated: VCS commit + lockfile SHA-256 + manifest SHA-256.

## Canonical input triple

| Side | Value |
|---|---|
| `vcs` | `deadbeef0123456789abcdef0123456789abcdef` |
| `lockfile` | `4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e` |
| `manifest` | `8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da` |

The `lockfile` value is `SHA-256(b"lockfile-payload-1")`; the
`manifest` value is `SHA-256(b"manifest-payload-1")`. These come from
the `pinned_vec_all_three_sides` test in
`mikebom-cli/src/binding/hash.rs::tests` — same input substrate so an
external verifier can recreate the canonical envelope without needing
the upstream source tree.

## Canonical envelope (C-2)

```text
{"algo":"v1","lockfile":"4c975d294781b5e5f49b946bc5f94da8638b4c60f1c1f3a8c35fa9534744712e","manifest":"8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da","vcs":"deadbeef0123456789abcdef0123456789abcdef"}
```

## Expected SHA-256 (C-3)

```text
745289decaf84d67e5cc9b333b435e8cc341ac19f7ab16673f05133d459a6111
```

This value is the `binding_hash` field on the matched component in
both `source.cdx.json` and `image.cdx.json`.

## Strength derivation (C-4)

- `populated_count == 3`
- All three sides match (source-tier == image-tier)
- → `strength: "verified"`
