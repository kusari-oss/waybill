# Data Model: `fragment` requires evidence of a live-bootstrap function

This feature adds no entity, annotation or wire field. It adds one parsed field (`builder`) and one debug field (`member_role_rung`). The C206 classifier's last rung becomes three rungs.

## `RawDerivation` (`closure/derivation.rs`)

| field | type | source | new |
|---|---|---|---|
| `builder` | `Option<String>` | top-level `builder` of `nix derivation show -r` | **yes** (R5) |

## C206 member role: evidence order

For a closure member that declares no `pname`, the first matching rule wins.

| # | Evidence | Role | Rung (R6) | Changed |
|---|---|---|---|---|
| 1 | content hash (`outputs.*.hash` or `env.outputHash`) | `source` | `content-hash` | no |
| 2 | the output is listed in the `patches` of a closure derivation | `source` | `patches` | no |
| 3 | `env.text` | `scaffolding` | `env-text` | no |
| 4 | version-shaped name: `srcOnly` or a source-tree name → `source`; a check name → `check`; otherwise `package` | `source` / `check` / `package` | `version-name` | no |
| 5 | `builder`'s final path component is `kaem` | `fragment` | `live-bootstrap-builder` | **new** (was: no evidence) |
| 6 | `name` ends in `.sh` or `.bash` | `scaffolding` | `script-name` | **new** (R2, final after R7) |
| 7 | none | `package` | `default` | **new** (was `fragment`) |

The classifier stays total over the five frozen values (FR-003). No sixth value is added.

## Role definitions (FR-004, FR-011)

| Role | Before | After |
|---|---|---|
| `package` | a version-bearing bootstrap package | a built member that is none of the others: a version-bearing bootstrap package, a program, data, or a stdenv stage |
| `scaffolding` | `writeText` output | a build script: `writeText` output, or a hook or helper script named as one |
| `fragment` | a live-bootstrap C function | a member built by live-bootstrap's `kaem`: C functions (`strncpy`), libraries (`libc.a`) and tools (`mes`) |

`source` and `check` are unchanged.

The 215 moat members built by `kaem` include `libc.a` and `mes` as well as functions. The catalog sentence "`fragment` is now said only of live-bootstrap functions" is therefore reworded to say "of what live-bootstrap's `kaem` built". That keeps it true (FR-011) without stretching "function" to cover a library.

## Edge endpoint

A member is an edge endpoint iff its role is `source`, `package` or `check`. That rule is unchanged. Members on rung 7 are endpoints. Members on rungs 5 and 6 are not, as before.

## Expected effect on the pinned moat closure

The values below come from R1 and R2. The tasks verify them.

| | before | after |
|---|---|---|
| `fragment` | 240 | 215 |
| `scaffolding` | 267 | 277 |
| `package` | 23 | 38 |
| `source`, `check` | 558, 46 | unchanged |
| rung `default` members | n/a | 15 |

Component counts in C184 differ from these derivation counts wherever members merge by identity. The golden review reads the component counts back from the documents.
