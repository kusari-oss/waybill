# Contract: `waybill sbom edit --retier`

## Flag

```
--retier <FROM>=<TO>    repeatable; applied in command-line order, each component at most once
```

### Refusals

Each is refused before any output is written, with a non-zero exit:

| Input | Message names |
|---|---|
| `design` (no `=`) | the expected form `FROM=TO` |
| `=source`, `design=` | the empty side |
| `design=sources` | the six accepted values |
| `design=design` | that `FROM` equals `TO` |
| `--retier design=source --retier design=build` | the conflicting targets for `design` |

## Fixture cases (`waybill-cli/tests/sbom_edit_retier.rs`)

Each runs on a synthetic three-format scan with `design`, `source` and `build` components, edits each format, and checks `parity-check` across the three outputs.

| Case | Edit | Expected |
|---|---|---|
| R1 | `--retier design=source` | every former `design` is `source` with C243 `design`; everything else is byte-identical apart from C194 and the type |
| R2 | `--retier design=source --retier source=build` | original `design` becomes `source` and original `source` becomes `build`; no component moves twice |
| R3 | `--retier analyzed=build` on a scan with no `analyzed` | succeeds; C194 shows `matched: 0`; nothing else changes |
| R4 | R1, then the output edited with `--retier source=build` | C243 still says `design`; C194 has an `ancestors` entry; `verify-chain` passes |
| R5 | R1 with `--drop-annotations waybill:` before `--retier` | the tiers are gone, so `matched: 0`; C243 never appears |
| R6 | R1's output edited with `--drop-annotations waybill:retiered` | C243 is kept (protected) |
| T1 | R1 on a document whose type was derived | the type is recomputed in all three formats; `design` is gone |
| T2 | R1 on a document scanned with `--sbom-type build` | the type is kept, and the report says why |
| C1 | R1 on a document whose completeness reason names an ecosystem | the verdict and reason are unchanged |
| V1 | `verify-chain` on R1's outputs, signed with a key and unsigned | passes |

## SC-001 (`measurements/retier_consumer_model.py`)

For npm-express, hpack-ogmios-no-generated-cabal and haskell-language-server, the script:
1. copies the committed CycloneDX golden into the scratchpad;
2. runs `waybill sbom edit --retier design=source`;
3. applies the modelled consumer rule (drop `design` and `scope: excluded`, then orphans).

Expected: `required` lost is 0, 0 and 0, against 27, 95 and 325 on the unedited goldens.
