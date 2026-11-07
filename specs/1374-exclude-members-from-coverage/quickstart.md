# Quickstart: verifying this feature

**Feature**: `1374-exclude-members-from-coverage` | **Issue**: #1380

The whole feature is one number getting smaller, which is the easiest kind of
change to get wrong without noticing. Four controls, in the order they catch
things.

---

## Control 1 — the sum, against the raw document

```sh
jq -r '.metadata.properties[]|select(.name=="waybill:cpe-coverage").value' out.cdx.json
jq '.components|length' out.cdx.json
```

The five counts must sum to the component count **with no filtering**. This
is the property `/v3` exists to restore: under `/v2` the sum matched only a
subset the reader had to derive.

**If the sum is short**, a component reached the classifier and came out of
no arm — the failure the closed enum exists to prevent.

## Control 2 — recompute from the document alone

```sh
python3 -I specs/1374-exclude-members-from-coverage/measurements/partition.py \
    out.cdx.json specs/1373-hackage-cpe-decision/measurements/osv-ecosystems-2026-11-07.txt
```

Derives all five counts from the emitted document and the committed ecosystem
list. Must agree exactly (SC-001, SC-006).

**It must be able to disagree.** Run it against a `/v2` golden first and
confirm it says so; a checker that cannot fail is not a check.

## Control 3 — nothing left the metric

```sh
diff <(jq -S '[.components[]|{purl,cpe,cpes}]' before.cdx.json) \
     <(jq -S '[.components[]|{purl,cpe,cpes}]' after.cdx.json)
```

Must be empty (FR-006, SC-004). This feature changes a count; if an
identifier moved, the synthesizer was touched and it should not have been.

## Control 4 — `with_candidates` held, and no member carries a CPE

```sh
jq '[.components[]
     | select(((.properties//[])|map(select(.name=="waybill:closure-member-role"))|length) > 0)
     | select((.cpe//null) != null or ((.cpes//[])|length) > 0)] | length' out.cdx.json
```

Must be **0** on every target. This is the structural claim behind FR-004: the
synthesizer refuses members, so none can carry a CPE — 0 of 539 on darwin and
0 of 1,087 on linux today. If it is ever non-zero, the guard changed and
FR-003's "exclude by what it is, not what was done to it" is what keeps the
count right anyway.

---

## The expectation, and what disagreeing means

**aarch64-darwin**, the pinned closure:

```
with_candidates           78
addressable_without_cpe  166
unaddressable            194     (was 733)
excluded_members         539
excluded_files             0
                         ───
                         977  == .components | length
```

**x86_64-linux**, the corpus — *different closure, do not scale from darwin*:

```
6 / 162 / 360 / 1087 / 0  ==  1615
```

Two figures there are worth knowing in advance, because each looks like a bug:

- **`unaddressable` falls from 1,446 to 360.** That is the correction, not a
  regression — 1,087 of the old figure were patches and tarballs.
- **`addressable_without_cpe` falls 163 → 162.** Exactly one component:
  `pkg:hackage/time`, role `fragment`. Any other movement in that count is a
  finding.

---

## Why the golden diff is the weakest signal here

33 local and 48 corpus goldens change, and every one changes the same single
line. That makes the diff easy to skim and easy to approve without reading —
and it would look identical if `excluded_members` were double-counted, or if
the member exclusion had silently swallowed real software.

**Control 2 is the one that matters**: it recomputes the counts from the
components themselves, so it disagrees when the row is internally consistent
but wrong about the document. Control 1 only proves the row adds up.
