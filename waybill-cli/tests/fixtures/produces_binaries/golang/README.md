# golang fixtures — `mikebom:produces-binaries` (milestone 116 PR-C)

| Sub-fixture | Layout | Expected |
|---|---|---|
| `cmd-layout/` | `go.mod` + `cmd/baz/main.go` + `cmd/baz-helper/main.go` | `["baz", "baz-helper"]` |
| `root-main/` | `go.mod` + top-level `main.go` (`package main`) | `["root-main"]` (the fixture dir's basename) |
| `library-only/` | `go.mod` + `lib.go` with `package foo` only (no `package main`) | property OMITTED |

`cmd-layout/main.go` covers FR-010 acceptance scenarios 1 + 2; `root-main/main.go` covers acceptance scenario 3.
