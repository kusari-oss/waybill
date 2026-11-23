# T010: census at the log-only commit

## Runs

- **Log-only commit**: `56d12eea1bf6a0d5008e9fc85083d589281a62d3` (T009). Output equals `main`'s (`control.md`).
- **TheRock and the quality corpus**: `census-versions.yml` run 45240732755 in kusari-sandbox/waybill-fixture-therock-sdk, dispatched `--ref main -f waybill_ref=56d12eea…`.
  - The fixture repository was at `1443cd6`, which adds spec 1648's parsing and control.
  - 20 of 20 jobs succeeded, and all 20 `refs.txt` files name `56d12eea`.
- **Public corpus**: `public-corpus.yml` run 45240738665, dispatched with `-f branch=1648-binary-kind-technique`.
  - The scanned commit is `56d12eea1bf6a0d5008e9fc85083d589281a62d3`.
  - `test result: ok. 166 passed; 0 failed`, so the log-only commit changes no public output.

## Positive controls

The `control` job built two programs on the runner and scanned them:
- a dynamically linked C program (`gcc`): 1 `dynamic-linkage` component, `pkg:generic/libc.so.6`, from glibc's versioned imports;
- a Go program (`go build`): 2 `go-buildinfo` events, the main module and `stdlib`.

Its assertion passed: `spec 1648 control ok: log-only ['pkg:generic/libc.so.6'] 2 go-buildinfo`. The `kind_technique_change` and `go-buildinfo` counters fire on real binaries, so the zeros below mean absence.

The same job's earlier controls still pass, including spec 1646's, whose `technique_change` checks the new event name was chosen to avoid (analyze I2):
- `control ok` (spec 1644);
- `identity control ok` (spec 1645);
- `spec 1646 control ok: wired`;
- `spec 1647 control ok: wired`.

The repository's synthetic cases K1–K7 cover every kind, including `embedded-source-path`, `cargo-auditable` and `elf-note-package` (`control.md`).

## Counts

Per target: `kind_technique_change` events / distinct PURLs, by kind; `kind_merge` events; and Go, as `detected-go` beside the origin-based `go-buildinfo`, in events / PURLs.

| Target | `dynamic-linkage` | `embedded-source-path` | `kind_merge` | `detected-go` | `go-buildinfo` |
|---|---|---|---|---|---|
| TheRock SDK | 49 / 49 | 645 / 15 | 0 | 0 | 0 |
| pants-backend-ai | 8 / 8 | 0 | 0 | 0 | 0 |
| cpp-mongo | 3 / 3 | 0 | 0 | 0 | 0 |
| go-kubernetes | 0 | 0 | 0 | 38 / 38 | **0** |
| go-cobra | 0 | 0 | 0 | 1 / 1 | **0** |
| the other 12 quality-corpus targets | 0 | 0 | 0 | 0 | 0 |

- **The technique changes match spec 1646's census exactly**: TheRock 49 and 645 / 15, pants-backend-ai 8, cpp-mongo 3 (SC-005's prediction).
- **TheRock's 645 `embedded-source-path` entries are 15 PURLs**, and no `kind_merge` fired. Each entry is a versionless component nested under the binary that carries the paths (spec 1534), so equal PURLs under different binaries are not merged. T017 counts the document's components.
- **No merge with a newly binary kind happens on any counted tree.** FR-004 holds as a rule (T014's unit test), but no counted tree triggers it. That matches `control.md`'s K6 finding: these kinds' PURLs carry no version, while a record's does, and repeated imports are joined before deduplication.
- **FR-005 is confirmed.** Spec 1646's "go-buildinfo" figures (go-kubernetes 38, go-cobra 1) are `detected-go`: Go source-tree main modules. Neither target has a Go binary, so the true `go-buildinfo` count is 0 for both. The runner's Go control shows the origin-based count does fire.

## Checkpoint (T011)

No count contradicts FR-002 to FR-005, and no amendment is needed. The expected effect of wiring:
- TheRock: 49 + 15 distinct PURLs change technique; components and edges are unchanged.
- pants-backend-ai: 8 PURLs; cpp-mongo: 3 PURLs.
- Public corpus: no change.
