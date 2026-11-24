# T018: agreement with hpack on the branch (SC-002)

`hpack_agreement.py` on each no-generated-cabal copy, against the branch binary's CycloneDX document (hpack 0.38.3).

| Repository | hpack packages | agree | differ | `pkg:exe` names kept whole |
|---|---|---|---|---|
| hs-opentelemetry | 24 | 24 | 0 | [] |
| lootbox | 7 | 7 | 0 | [] |
| hs-temporal-sdk | 7 | 7 | 0 | [] |
| ogmios | 8 | 8 | 0 | [] |

**46 of 46 packages agree**, and no waybill name keeps a `:qualifier`. On `main` 16 of 46 agreed, and hs-temporal-sdk kept `hspec-discover:hspec-discover` (`agreement-main.md`).

The comparison splits a `pkg:exe` name before comparing (`agreement-main.md`), so the last column is what keeps that defect visible. It is empty on the branch.
