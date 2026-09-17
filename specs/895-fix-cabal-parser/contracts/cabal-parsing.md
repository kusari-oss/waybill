# Contract: `.cabal` dependency parsing

Each clause is stated so it can be asserted, with its status before and after.
Status "today" is measured against `waybill 0.7.0` using the #891 reproducer.

---

## A-1 — Nothing is invented

> Every emitted Haskell component's name MUST be a dependency name declared
> in the scanned `.cabal` file.

**Today**: violated. Three of four components from the reproducer carry names
or versions assembled from adjacent fields and comment text. On the real
project that surfaced this, two of twenty-four components do not exist on
Hackage.

This is the clause the feature exists for. A fabricated component is
indistinguishable from a real one to every downstream consumer.

---

## A-2 — A block ends where cabal says it ends

> A field block MUST terminate at the first subsequent non-blank line whose
> indentation is at most the field line's own.

**Today**: violated. Termination is blank-line / column-0 / EOF, so a block
runs into the following field.

Stated as the rule rather than as its symptoms, because the three
fabrication defects are one cause and a test per symptom would not prove the
cause was fixed.

---

## A-3 — An identifier names a package

> An emitted identifier MUST name a package. It MUST NOT contain a version
> constraint, and MUST NOT contain a placeholder standing in for a version.

**Today**: violated in both directions — `>=4.11_&&_<4.22` appears in the
version slot, and the codebase's `unspecified` sentinel is the same class of
error in milder form.

---

## A-4 — The constraint survives

> A constraint declared in the file MUST be recoverable from the emitted
> document, in every output format, verbatim.

**Today**: holds, via C20 `waybill:requirement-ranges`. A-3 removes the
*other* place it currently appears, which makes this clause load-bearing
where it was previously redundant. It must not regress.

---

## A-5 — A partial failure costs one entry, not a list

> An unreadable entry MUST NOT prevent its siblings from being emitted, and
> the number skipped MUST be reported, including when zero.

**Today**: n/a — nothing is currently detected as unreadable, because
anything that parses into two whitespace-separated halves is accepted as a
name and a version. That permissiveness is what produces A-1's violations.

---

## A-6 — A build tool is identified as one

> A build-tool declaration MUST emit an identifier naming the package alone,
> marked build-time, with the declared executable recoverable.

**Today**: violated. `hspec-discover:hspec-discover` is emitted as a Hackage
package name, which no registry will resolve.

---

## A-7 — Correct input stays correct

> A `.cabal` file the parser already reads correctly MUST emit an unchanged
> set of component names.

Identifiers change per A-3; **names** do not. The property that makes this
safe to land.

---

## A-8 — The verdict is measured against the file

> Accuracy claims MUST be asserted by comparing emitted component names
> against the names in the scanned `.cabal` file — not against a count, and
> not against the parser's own report of what it did.

The parser is the thing under test; asking it how many dependencies it found
is asking the change to grade itself. The same trap milestone 866 fell into
and milestone 868 was written to avoid.

---

## Status summary

| Clause | Today | After |
|---|---|---|
| A-1 nothing invented | **violated** — 3 of 4 on the reproducer | holds |
| A-2 block termination | **violated** | holds |
| A-3 identifier names a package | **violated** — constraint in version slot | holds |
| A-4 constraint survives | holds (redundantly) | holds (load-bearing) |
| A-5 partial failure is per-entry | n/a — nothing detected as unreadable | holds |
| A-6 build tool identified | **violated** | holds |
| A-7 correct input unchanged | holds | must not regress |
| A-8 measured against the file | n/a | holds |

---

## Verification

```bash
# A-1, A-2, A-3, A-6: the #891 reproducer
waybill --offline sbom scan --path <repro> --format cyclonedx-json \
  --output cyclonedx-json=out.json
jq -r '.components[].purl' out.json | sort
# expect exactly:
#   pkg:hackage/waybill-fixture-cmt
#   pkg:hackage/waybill-fixture-core
#   pkg:hackage/waybill-fixture-tool
#   pkg:hackage/waybill-fixture-vec

# A-1 generalised, and SC-002: every emitted name appears in the file
comm -23 \
  <(jq -r '.components[].purl' out.json | sed 's|pkg:hackage/||' | sort -u) \
  <(grep -oE '^[ ,]*[A-Za-z][A-Za-z0-9-]*' <repro>.cabal | tr -d ' ,' | sort -u)
# expect: empty

# A-4: the constraint is still reachable
jq -r '.components[].properties[]? | select(.name=="waybill:requirement-ranges")' out.json

# A-7: a file already parsed correctly emits the same names
#      (compare name sets, not identifiers — identifiers change by A-3)
```
