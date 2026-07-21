# maven-weak — `strength: weak`

Reference fixture for the milestone-072 binding-hash-v1 algorithm
contract — Maven ecosystem variant. Maven has no canonical lockfile
in waybill's milestone-070 emission pattern, so the `lockfile` side
is `null`; only VCS commit + `pom.xml` SHA-256 populate. Strength
caps at `weak` per `contracts/binding-hash-v1.md` C-7 maven row.

## Canonical input triple

| Side | Value |
|---|---|
| `vcs` | `deadbeef0123456789abcdef0123456789abcdef` |
| `lockfile` | `null` (Maven has no canonical lockfile) |
| `manifest` | `8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da` |

The `manifest` value is `SHA-256(b"manifest-payload-1")` — same byte-
string substrate as the cargo-verified / golang-verified fixtures so
external verifier authors share one substrate.

## Canonical envelope (C-2)

Note the explicit `null` for `lockfile`; per C-2 absent inputs are
serialized as JSON `null` (NOT an empty string, NOT a missing key).

```text
{"algo":"v1","lockfile":null,"manifest":"8dd2a3b941862cd07ac2ef4966064200a1efed2d6aa35fb38581984bc067e3da","vcs":"deadbeef0123456789abcdef0123456789abcdef"}
```

## Expected SHA-256 (C-3)

```text
59eca409058785ed39170de1fca456872afef52a1af7d1070719a6e36f672c35
```

## Strength derivation (C-4)

- `populated_count == 2` (vcs + manifest)
- Both populated sides match
- → `strength: "weak"` (capped per Maven's no-lockfile constraint)

## Per-ecosystem extraction notes (C-7 maven)

- `vcs`: `git rev-parse HEAD` (future: `<scm>` block in `pom.xml`).
- `lockfile`: NOT POPULATED. Maven has no canonical lockfile.
- `manifest`: SHA-256 of `pom.xml` bytes (resolved form per
  milestone 070).
