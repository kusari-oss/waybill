# T025: TheRock measure-source (SC-004)

Run 44943177224 of `measure-source` in the TheRock SDK fixture repository: `--ref main -f waybill_ref=47aef52a… -f base_ref=86bce894…`. The single `measure` job succeeded.

`refs.txt`:

```
base 86bce894306916953748a85b96b27318c7e059bc
branch 47aef52ae7a5ac6ca482ceb233f2ed213e70189f
therock b1cae390cc745283dce25581415353f9bc906162
```

## Output

All six documents were compared: CycloneDX, SPDX 2.3 and SPDX 3, with and without submodules. The per-run fields were ignored: `serialNumber`, `metadata.timestamp`, `creationInfo`, `documentNamespace`, SPDX 3 `created` and SPDX 2.3 `annotationDate`.

**All six are identical to `main`'s.**

| | `main` | branch |
|---|---|---|
| components (submodule scan, recursive, with `metadata.component`) | 4,215 | 4,215 |
| URLs with userinfo, any document | 0 | 0 |
| `ssh://git@` URLs | 0 | 0 |
| `stripped credentials from URL` warnings | — | 0 |

As a control, the branch's submodule scan carries 90 `waybill:download-url` properties and 681 `waybill:requirement-ranges`. URL-bearing fields are emitted and unchanged.

From `report.md`, every row is equal across the two sides:
- unreachable components without an orphan reason: 0;
- 69 own-declared PyPI packages, all reachable;
- root targets 46 / 630, with the formats agreeing.

## Time

Not gated by the spec; recorded.

| side | run 0 | run 1 | run 2 | median |
|---|---|---|---|---|
| `main` | 29.991 s | 29.942 s | 29.841 s | 29.942 s |
| branch | 29.807 s | 29.835 s | 30.024 s | 29.835 s |

Ratio 0.996.
