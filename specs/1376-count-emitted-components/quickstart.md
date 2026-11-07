# Quickstart: verifying this feature

**Feature**: `1376-count-emitted-components` | **Issue**: #1385

Nothing emitted changes, so the usual controls — golden diffs, component
counts — are all expected to show **nothing at all**. That makes them useless
here and makes one control carry the whole feature.

---

## Control 1 — the documented expression works where it used to fail

```sh
cd waybill-cli/tests/fixtures/pnpm_v9_scope
waybill sbom scan --path . --format cyclonedx-json --output cyclonedx-json=out.cdx.json

jq -r '.metadata.properties[]|select(.name=="waybill:cpe-coverage").value' out.cdx.json
jq '.components|length' out.cdx.json                       # 9  — the OLD check, short by 1
jq '[.components[]."bom-ref"] as $r
    | (.components|length)
      + (if ((.metadata.component.properties // [])|length) > 0
            and ($r | index(.metadata.component."bom-ref") | not)
         then 1 else 0 end)' out.cdx.json                   # 10 — agrees with the row
```

**The old expression must still be seen to fail**, or the fix is being
verified against a case that never broke.

## Control 2 — the expression also holds where it already worked

Run the same comparison on a `--root-name` scan and on a multi-main-module
project. Both must agree, and neither must need a different rule.

| shape | expected |
| --- | --- |
| synthetic root (`--root-name`) | +0 adjustment |
| promoted root | +1 |
| duplicated root (live `moat`) | +0 |

**A fix that only handles the promoted case is wrong in the other two**, and
each over-counts by exactly 1 — the same magnitude as the bug, in the
opposite direction, on documents that were previously correct.

## Control 3 — nothing moved

```sh
diff before.cdx.json after.cdx.json    # must be empty
```

Byte-identical. This feature changes prose and adds a test; a document that
differs means something leaked.

## Control 4 — the new test fails without the fix

Check out the test against the **old** wording's expression and confirm it
fails on `pnpm_v9_scope`. A test written after the fix, never seen red, is a
test of nothing.

---

## What is deliberately not checked

**That the row's numbers are right.** They were never wrong. Milestone 1374
verified them and this feature does not revisit them — see FR-002. If an
implementation changes a count, that is out of scope and a defect.

---

## The trap this feature exists because of

The `debug_assert` in `cpe_coverage` compares the counts to the **slice**.
That is a true assertion, it has always passed, and it says nothing about the
emitted document.

Its comment claimed otherwise, and the documentation row was written from
that comment. **A true assertion with a false description is worse than no
assertion**, because it transfers unearned confidence to whoever reads it
next — which is precisely what happened across three milestones.

When reading the corrected comment, check it describes the comparison in the
line below it and nothing more.
