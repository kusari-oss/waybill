# Quickstart

How to see the defect, and how to know when it is fixed. ~10 minutes.

## Prerequisites

- a `waybill` release binary built from the commit under test — **rebuild
  it**; a stale binary is how three claims in an earlier spec in this repo
  came to describe a product state that no longer existed
- `git`, `jq`

## 1. Reproduce

```bash
W=$(mktemp -d); cd "$W"
git init -q android && cd android
git remote add origin https://github.com/bitwarden/android
git fetch -q --depth 1 origin d817f6b4bf7c17172a74fabca1e09e738c7ec6c9
git checkout -q FETCH_HEAD && cd ..

# --root-name matters: it is what the quality-corpus harness passes, and
# without it the main module becomes the root, which changes which node the
# fallback hangs its fabricated edges off.
waybill --offline sbom scan --path "$W/android" \
  --format cyclonedx-json --output cyclonedx-json="$W/out.json" \
  --root-name gradle-bitwarden-android --root-version d817f6b
```

Look at what the application main module depends on:

```bash
jq -r '.dependencies[] | select(.ref|test("generic/android")) | (.dependsOn//[])[]' "$W/out.json"
```

Before the fix: **nothing**. The component is in the document, its
`Gemfile.lock` declares nine dependencies, all nine are present as components,
and it has no outgoing edges.

Confirm the declarations are real:

```bash
awk '/^DEPENDENCIES/,/^$/' "$W/android/Gemfile.lock"
```

## 2. Prove the cause, not just the symptom

```bash
waybill --offline sbom scan --path "$W/android" \
  --format cyclonedx-json --output cyclonedx-json="$W/xeco.json" \
  --root-name gradle-bitwarden-android --root-version d817f6b \
  --experimental-cross-ecosystem-edges

jq -r '.dependencies[] | select(.ref|test("generic/android")) | (.dependsOn//[])[]' "$W/xeco.json"
```

Nine edges appear, and they are exactly the `DEPENDENCIES` block. That is the
whole diagnosis in one command: the data was read, the components exist, and
only the ecosystem used for lookup stood between them.

## 3. Run the control — the part people skip

A change that makes edges appear is not obviously correct; it could be
attaching things that should not be attached.

```bash
# Same-ecosystem project: must not move at all.
jq -r '[.dependencies[].dependsOn//[]|length]|add' <any committed corpus cdx.json>
```

Before and after must be **byte-identical** for every reader that has not
adopted. That is not a hopeful expectation — FR-001a makes it structural, and
if it does not hold, the adoption gate is not working.

## 4. What "fixed" looks like

| | before | after |
|---|---|---|
| main-module outgoing edges | 0 | **9** |
| those 9 | — | exactly the `DEPENDENCIES` block |
| gem components reachable from root | no | **yes** |
| document reports flat | yes | **no** |
| same scan with the inference flag | 9 | **9 — must not move** |
| unadopted readers' output | — | **byte-identical** |
| unresolved count in document | absent | present, **0** here |

The two rows marked "must not move" are the ones that catch a fix that went
too far.

## 5. At scale

`lablup/backend.ai` @ `809fcd394dd8e39456986dd742e7d51c6aedd647` has the same
visible symptom — a large unreachable island, depth 1, flat — and **this
feature will not fix it**. Its root manifest genuinely declares nothing; the
gap there is that a lockfile resolve has no owning component. Different cause,
tracked separately.

It is worth scanning anyway, as a check that this change does not
accidentally attach something there. Its edge count should not move.

## Traps

- **A stale binary.** Rebuild before measuring.
- **Omitting `--root-name`.** Without it the main module becomes the root and
  the primary-dependency fallback fires against it, fabricating ~245 edges and
  hiding the defect completely. The corpus harness always passes it; a
  reproduction that does not is measuring a different thing.
- **Reading the unresolved count as the proof.** That count is produced by the
  code being changed. Assert edge presence against the emitted graph (D-7).
- **Assuming the corpus bound is the target.** `gradle-bitwarden-android`'s
  current `edges 346..424` was authored against fabricated fallback edges and
  is not what correct looks like. Re-author it after this lands, from a
  measurement, not toward the old number.
