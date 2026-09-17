# Phase 0 Research: Trustworthy `.cabal` dependency parsing

Every empirical claim below was measured against real files during planning.
Where a number or rule is quoted, the command that produced it is shown so
it can be re-run when the inputs move.

---

## R1 — Where does a cabal field block actually end?

**Decision**: A field block ends at the first subsequent non-blank line whose
indentation is **less than or equal to the indentation of the field line
itself**.

**Rationale**: Measured on two real `.cabal` files with different generators,
which produce structurally different layouts. Both obey this one rule.

Layout A — hpack-generated, field name alone on its line:

```
L44 indent=2: build-tool-depends:
L45 indent=6:   hspec-discover:hspec-discover
L46 indent=2: build-depends:          <== terminates (2 <= 2)
L47 indent=6:     base >=4.11 && <4.22
L48 indent=4:   , bytestring >=0.10 && <0.13
...
L63 indent=2: default-language: Haskell2010   <== terminates (2 <= 2)
```

Layout B — `cabal init`-style, first entry inline with the field name:

```
L23 indent=2:  build-depends:       base >=4.14 && <4.15
L24 indent=21:                    , moat
L25 indent=2:  -- hs-source-dirs:      <== terminates (2 <= 2)
```

**Two things this rules out**, both of which a plausible implementation would
get wrong:

1. **"The block continues while lines are more indented than the first
   entry"** is wrong. In Layout A the first entry sits at indent 6 and every
   subsequent entry at indent 4 — all part of the same block. The reference
   point is the *field* line, not the first entry.
2. **Comment stripping is not what fixes the comment defect.** In Layout B
   the comment sits at indent 2, so the indentation rule terminates the block
   before reaching it. Comment stripping is still required, but only for
   comments *inside* a block (indent greater than the field's), which is a
   different and rarer case than the one that produced the observed bug.

**Alternatives considered**: matching against a closed list of known cabal
field names as terminators — rejected, because it fails on any field the list
does not know, and the cabal format permits `x-`-prefixed custom fields.

**Reproduce**:

```bash
python3 - <<'PY'
import re
def indent(l): return len(l) - len(l.lstrip(" \t"))
lines = open("<some>.cabal").read().splitlines()
for i, l in enumerate(lines):
    if re.match(r"^[ \t]+build-(tool-)?depends:", l):
        fi = indent(l)
        for j in range(i+1, len(lines)):
            if not lines[j].strip(): continue
            if indent(lines[j]) <= fi:
                print(f"field L{i+1} (indent {fi}) ends at L{j+1}: {lines[j].strip()[:40]}")
                break
PY
```

---

## R2 — How much existing test surface moves?

**Decision**: The versionless change (FR-005) is scoped to dependencies
declared in a `.cabal` dependency list. It does **not** touch the `ghc` /
stackage-resolver placeholder components, which use the same `unspecified`
token on a different code path.

**Rationale**: Measured — exactly one existing integration assertion pins the
sentinel, and it is on the out-of-scope path:

```
waybill-cli/tests/haskell_stack_discrimination.rs:201
    component_with_purl(&doc, "pkg:generic/ghc-9.6.4@unspecified")
```

`haskell.rs` contains 6 occurrences of `unspecified`; the spec's scope
boundary (`.cabal` dependency path only) already excludes the resolver ones.
Changing them would be scope creep into a path this feature does not claim to
fix, and would break a passing test for no stated requirement.

**Consequence for planning**: the blast radius of FR-005 inside the existing
suite is smaller than the raw grep suggests. Confirm per-occurrence at
implement time rather than assuming the count.

---

## R3 — Does the constraint record need a new catalogue row?

**Decision**: No. **C20 already exists** and its extractors are correct. The
work is a one-word documentation fix, not a new row.

**Rationale**: The catalogue row's label column reads
`waybill:requirement-range` (singular) while the emitters and all three
extractors use `waybill:requirement-ranges` (plural):

```
docs/reference/sbom-format-mapping.md      | C20 | `waybill:requirement-range`
generate/{cyclonedx,spdx}/…                      "waybill:requirement-ranges"
parity/extractors/{cdx,spdx2,spdx3}.rs     c20_* "waybill:requirement-ranges"
```

`every_catalog_row_has_an_extractor` keys on `row_id`, not on the label, so
the mismatch has never failed a gate and parity has been functional
throughout. It is a typo with no runtime effect — but FR-009 makes this field
the **sole** carrier of information that was previously (incorrectly) visible
in the identifier, so the row that documents it should name it correctly.

**Correction to an earlier claim**: planning initially recorded "no catalogue
row exists for `waybill:requirement-ranges`". That was a grep for the plural
against a doc that spells it singular. FR-009 is therefore mostly already
satisfied; what remains is the label fix plus verifying the annotation still
emits once the version slot stops carrying the constraint.

**Principle V audit** (required because FR-009 keeps an annotation rather
than moving to a native field): no CycloneDX 1.6, SPDX 2.3 or SPDX 3.0.1
native field carries a *declared version constraint* as distinct from a
resolved version. CDX `component.version` is the resolved version by
definition; SPDX 2.3 `Package.versionInfo` likewise; SPDX 3
`software_packageVersion` likewise. A constraint describes an acceptable set,
a version names one member — the formats model only the latter. **KEEP-NO-NATIVE**,
inherited from C20's original audit, unchanged.

---

## R4 — Which project becomes the Haskell corpus target?

**Decision**: A project under the `haskell` GitHub organisation, mirrored to
`kusari-sandbox` and pinned by SHA. `haskell/aeson` is the primary candidate,
`haskell/text` the fallback.

**Rationale**: Three constraints, all checked:

| Constraint | Why | Checked |
|---|---|---|
| Must be cabal-only | A repo carrying `stack.yaml.lock` or `cabal.project.freeze` exercises the lockfile path, not the path this feature fixes | measured, below |
| Must be neutrally governed | External-name policy — a committed artifact may name an ecosystem's community organisation, but not a company (the ubiquitous-tech-vendor carve-out, e.g. Google OSV, does not apply here) | `haskell` is the language's own community org, the same category as `rust` and `pantsbuild` |
| Must be mirrored | A pin that can move underneath the gate is not a pin | matches the existing `kusari-sandbox/example-javascript` precedent |

Measured via `gh api repos/<r>/contents`:

| candidate | root files | verdict |
|---|---|---|
| `haskell/aeson` | `aeson.cabal`, `cabal.project` | **suitable** — no freeze, no stack lock |
| `haskell/text` | `text.cabal`, `cabal.project` | suitable, fallback |
| `haskell-servant/servant` | `servant.cabal`, `cabal.project`, `stack.yaml`, `stack.yaml.lock` | **unsuitable** — the lockfile path would win |

Note `cabal.project` alone is not a lockfile — only `cabal.project.freeze`
pins versions. Both candidates therefore resolve nothing and exercise the
`.cabal` declaration path end to end, which is what SC-002 needs.

**Open at implement time**: confirm the chosen repo's `.cabal` uses a layout
this feature must handle, and record its dependency count so SC-002's "zero
fabricated names" has a denominator. Do not assume from the repo name.

**Alternatives considered**: the project that surfaced this bug is published
by a company, and not one in the ubiquitous-tech-vendor category, so it is
out of bounds for a committed artifact per the spec's Assumptions —
regardless of the project itself being open source. Synthetic fixtures alone were rejected in
clarification — they cannot satisfy SC-002, which is about real-world
layouts.

---

## R5 — Blast radius on committed goldens

**Decision**: No existing golden changes. The feature *adds* one target's
goldens rather than modifying any.

**Rationale**: Measured — no Haskell project appears in
`waybill-cli/tests/corpus_harness_195/manifest.rs` or
`xtask/corpus/quality-corpus.toml`. There is no `pkg:hackage/` component in
any committed golden.

**Consequence**: the new target's goldens are generated **in CI**, never
locally. Goldens embed runner-absolute paths in `waybill:source-files`;
generating them on a developer machine produces goldens that pass only on
that machine. This is documented at
`docs/development/refreshing-corpus-goldens.md` and was observed first-hand
during the #890 refresh, where a local comparison showed a false diff per
format for exactly this reason.

---

## R6 — Sequencing

**Decision**: The parser fix and the corpus target are independent and should
land in that order.

**Rationale**: The parser fix is self-contained — source, unit tests, and
synthetic integration fixtures, all inside the repository. The corpus target
needs an external mirror created, a pin chosen, and goldens produced by a CI
round-trip. Coupling them makes the accuracy fix wait on repository
administration it does not need.

Landing the parser first also means the corpus target's first goldens record
**correct** output. Adding the target first would commit goldens containing
the fabricated components, and then immediately refresh them — which is how
the drift this project keeps fighting accumulates.
