# Contract: emitted interfaces

**Feature**: `1367-closure-component-cpes` | **Issue**: #1371

waybill's external interface is the documents it emits plus its CLI. This
feature changes one existing field's population, adds one document-scope row,
and adds no flags.

---

## 1. The CPE candidates themselves

A closure component whose name is a curated mapping row and whose version is
release-shaped gains CPE candidates. For `openssl 3.6.2`:

**CycloneDX** — primary in the native field, remainder in the existing C19
property:

```json
{
  "name": "openssl",
  "version": "3.6.2",
  "purl": "pkg:generic/openssl@3.6.2",
  "cpe": "cpe:2.3:a:openssl:openssl:3.6.2:*:*:*:*:*:*:*",
  "properties": [
    { "name": "waybill:cpe-candidates",
      "value": "[\"cpe:2.3:a:openssl:openssl:3.6.2:*:*:*:*:*:*:*\", …]" }
  ]
}
```

**SPDX 2.3** — `externalRefs[]` with `referenceType: cpe23Type`,
`referenceCategory: SECURITY`, plus the C19 annotation for the remainder.

**SPDX 3** — one `externalIdentifier` per candidate.

Constraints:

- **The PURL is unchanged.** A CPE is emitted *alongside* `pkg:generic`, never
  instead of it. A consumer matching on the PURL today sees no difference.
- **No new emitter code.** All three carriers already read
  `component.cpes`; this feature populates that field for components that
  reach it empty.
- **Candidates, not assertions.** The synthesizer emits several per component
  by design, because no authoritative CPE source exists and a downstream
  matcher takes the union. This feature inherits that posture unchanged.

### What does NOT gain a CPE

| population | count on the pinned closure | why |
| --- | --- | --- |
| closure members | 539 | not software; and 538 carry no version at all |
| non-release versions | 6+ | a candidate built from `0-unstable-2018-06-17` matches nothing |
| unmapped names | 117 | FR-007 silent-skip, unchanged from milestone 097 |
| components that already have CPEs | — | FR-004/FR-005; includes every `--supplement-cdx` value |

---

## 2. `C209 waybill:cpe-coverage` — document scope

```json
{
  "schema": "waybill-cpe-coverage/v1",
  "with_candidates": 6,
  "eligible_unmapped": 117
}
```

- **Carriers**: CDX `metadata.properties[]`; SPDX 2.3 and SPDX 3
  document-scope annotation. `Directionality::SymmetricEqual`,
  `order_sensitive: false`.
- **Emitted on every scan**, not only closure scans — the pass it describes
  runs for every component from every reader, and gating it would leave
  non-closure scans with the same blind spot this feature fixes.
- `with_candidates: 0, eligible_unmapped: 0` is "nothing to do".
  `with_candidates: 0, eligible_unmapped: 123` is **"the pass did not run"**.
  Distinguishing those is the row's entire purpose.

> The counts above are illustrative shape. `with_candidates: 6` is the
> measured floor from the curated table; research R7 predicts the real total
> is higher because the 74 `pkg:pypi` components take the pypi arm. **That
> prediction is unmeasured** and no artifact may quote a total as data until
> the measurement task records one.

A row added to `docs/reference/sbom-format-mapping.md` without a matching
entry in `parity/extractors/mod.rs::EXTRACTORS` fails
`every_catalog_row_has_an_extractor` and `holistic_parity`. The doc row and the
three extractors land in the same change.

**Native-field audit (Constitution Principle V)**: no CDX 1.6, SPDX 2.3 or
SPDX 3.0.1 construct records how many identifiers a generation pass
synthesized. CDX `metadata.lifecycles[]` names build phases;
`compositions[].aggregate` grades the completeness of a *component set*, not of
an identifier pass, and collapsing two counts into one aggregate word would
lose the distinction the row exists for. SPDX's `CreationInfo.comment` and
`comment` are untyped prose. Parity bridge, on the C192 / C204 / C208 template.

---

## 3. CLI

**No new flags.** The pass runs unconditionally, as the existing synthesis
pass does. There is nothing for an operator to turn on.

`--supplement-cdx` is unchanged and continues to win: a component whose CPEs
the operator supplied is never recomputed.

---

## 4. Compatibility

- **Additive for every consumer.** Components gain a `cpe` field and
  `externalRefs` entries they did not have. Nothing is removed, no PURL
  changes, no component is added or removed.
- **A consumer counting CPEs will see the number rise**, which is the point.
- **Goldens change** for any corpus target whose components newly receive
  candidates. `nix-closure-moat` is the certain one; whether any other target
  is affected is a measurement, not an assumption — the new pass runs for
  every scan, and a non-closure component that reached it with empty CPEs
  because it was unmapped will still be unmapped, so the expectation is "no
  other target changes". **That expectation is verified by the golden diff,
  not asserted here.**
