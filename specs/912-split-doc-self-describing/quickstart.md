# Quickstart: verifying split-document resolve identity

**Feature**: `912-split-doc-self-describing` (#914) | **Date**: 2026-09-18

---

## 0. See the defect first

```bash
waybill sbom scan --path waybill-cli/tests/fixtures/pants_discovered_resolves \
  --offline --format cyclonedx-json --split=resolve --output-dir /tmp/before
```

```bash
for f in /tmp/before/*.cdx.json; do
  echo "== $f"
  jq -r '.metadata.component.name' "$f"
  jq -r '.metadata.properties[]? | select(.name=="waybill:resolve-ownership") | .value' "$f"
done
```

**Pre-change**: both files name the repository, and both carry a
byte-identical ownership value listing *both* resolves. A reader holding one
file sees two resolve names and nothing saying which file this is. That is
the whole defect.

---

## 1. SC-001 / SC-003 — a document identifies itself, and the filename is irrelevant

```bash
cp /tmp/after/lint.generic.cdx.json /tmp/renamed-to-something-else.json
jq -r '.metadata.properties[]? | select(.name|test("resolve-identity")) | .value' /tmp/renamed-to-something-else.json
```

**Pass**: the resolve is recoverable from the content of a file whose name
tells you nothing. **Fail**: you had to look at the filename or the manifest —
which is the state this feature exists to leave.

---

## 2. SC-002 — two documents from one repository differ

```bash
diff <(jq -S '.metadata.properties' /tmp/after/default.generic.cdx.json) \
     <(jq -S '.metadata.properties' /tmp/after/lint.generic.cdx.json)
```

**Pass**: they differ on the identity and **agree** on the ownership value.
Both halves matter: the first is the fix, the second is FR-007 — closing this
gap must not cost the reader the repository-wide picture.

---

## 3. SC-004 — one reading procedure, no branch on provenance

Read the identity from a **declared** document and a **discovered** one with
the same expression:

```bash
for f in /tmp/declared/*.cdx.json /tmp/after/*.cdx.json; do
  jq -r --arg f "$f" '$f + ": " + (.metadata.properties[]? | select(.name|test("resolve-identity")) | .value)' "$f"
done
```

**Pass**: every file answers. **Fail**: any file where you had to fall back to
`metadata.component` — that is the two-code-paths outcome FR-004 exists to
prevent, and a consumer would have to establish provenance to know which to
read, which is the question it came to ask.

---

## 4. SC-005 — where a root also names the resolve, they agree

```bash
jq -r '[.metadata.component.name,
        (.metadata.properties[]? | select(.name|test("resolve-identity")) | .value)]
       | @tsv' /tmp/declared/app.generic.cdx.json
```

**Pass**: both name `app`. Two fields stating one fact drift; this is the
check that stops it.

---

## 5. SC-002a / C-2 — namespaces are distinguishable

Needs a fixture declaring one name under **both** `[python.resolves]` and
`[jvm.resolves]`.

**Pass**: the two identities differ. **Fail**: identical identities — which
today also means the two resolves were merged into one document, because
that is #919, still open. Until #919 lands this check verifies the identity's
*shape*, not the split's correctness.

---

## 6. R3 — the JVM case, which is not optional

Anchoring is Python-only: the coursier/JVM reader emits no resolve anchor, so
**every** document from a JVM Pants repository names the repository,
declared or not.

```bash
waybill sbom scan --path <jvm-pants-fixture> --offline \
  --format cyclonedx-json --split=resolve --output-dir /tmp/jvm
```

**Pass**: each document states its resolve. **Fail**: a Python-only
implementation that looks complete against Python-only fixtures — which is
exactly what a missing JVM fixture would let through.

---

## 7. SC-007 / SC-008 — nothing invented, nothing over-claimed

```bash
# component counts unchanged from before the feature
for f in /tmp/before/*.cdx.json /tmp/after/*.cdx.json; do
  echo "$f $(jq '.components | length' "$f")"
done

# and no identity where there is no resolve to identify
waybill sbom scan --path <any-repo> --offline --format cyclonedx-json --output /tmp/unsplit.json
jq '.metadata.properties[]? | select(.name|test("resolve-identity"))' /tmp/unsplit.json
```

**Pass**: counts match, and the unsplit document's query returns nothing at
all — absent, not present-and-empty. **Fail**: a synthesised component
appears, or an empty identity does.

---

## 8. Cross-format parity

```bash
waybill sbom scan --path <fixture> --offline \
  --format cyclonedx-json,spdx-2.3-json,spdx-3-json --split=resolve --output-dir /tmp/three
```

Extract the identity from each format for the same resolve and compare
**decoded values**, not bytes — CycloneDX carries a property value as a
string; SPDX carries structure. m911 established that the decoded value is
the contract.

---

## What "done" looks like

| Check | Expectation |
|---|---|
| A renamed document | still names its resolve |
| Two documents, one repository | differ on identity, agree on ownership |
| Declared and discovered | answered by one expression |
| Declared document | identity and root agree |
| JVM Pants repository | every document states its resolve |
| Unsplit / workspace / directory | no identity at all |
| Component counts | unchanged |

## Before building any of it

R6 records a question nobody has answered: **is there a consumer that reads a
split document without its manifest?** The manifest already maps every file
to its resolve. If no such reader exists, this feature is tidiness rather than
value, and the cheapest way to find out is to ask before building.
