# Research: hpack `!include` and YAML merge keys

Observations are at `main` `ea149065`. "Measured" means executed, with hpack 0.38.3 (`nix run nixpkgs#hpack`) or on the #1677 corpus repositories cloned at their pins. "Read" means read from code. Anything else is labelled inference.

## R1. How anchors cross files: splice the text (decision)

**The constraint.** serde_yaml parses one document and fails on an alias whose anchor it has not seen. That is today's `unknown anchor`, and it happens before any `!include` tag can be handled, because the anchor lives in the file the tag names.

**hpack's behaviour, measured.** All four results below are what splicing the included file's content into the including file, at the `!include` position, produces:

| Case | hpack |
|---|---|
| The includer uses an anchor the included file defines (upward) | resolved (lootbox `code/network`: `*lib-common`, two includes away) |
| The included file uses an anchor its includer defined earlier (downward) | resolved (`base`, `text`) |
| An anchor defined, then redefined later in the same file | the later definition wins |
| An anchor defined, then redefined in a file included later | the included definition wins |

**Decision**: before parsing, replace each `!include "<path>"` in **block context** with the included file's text, re-indented to that position, recursively and relative to the file that names it. The result is one YAML document with every anchor in document order, which is what hpack's event-level splice produces.

- `key: !include "x"` becomes `key:` followed by `x`'s lines, indented by the key's column + 2. A key is any plain or quoted YAML scalar ending at the first `: `, slashes included: the corpus's commonest form is `_common/lib: !include "../package-common.yaml"` (32 of 49 includes are key values). The path may be quoted or unquoted.
- `<<: !include "x"` is the same, with key `<<`.
- `- !include "x"` becomes `-` followed by the indented lines.
- Document markers (`---`, `...`) and a leading BOM in the included file are dropped.

An `!include` in any other position (flow syntax, mid-line) is left in place. The existing tag-stripping path then announces it as an unresolved include (FR-004).

**Alternatives considered**:
- **Parse each file to a value and substitute values.** This cannot carry an anchor from one file into another's parse, which is the defect.
- **An event-level YAML parser.** serde_yaml exposes no events. Writing a parser, or depending on serde_yaml's internal libyaml binding, is far more code than the splice, for no behaviour the corpus needs (R2).

## R2. What the corpus uses (measured)

Over every `package.yaml` in the four #1677 repositories and every file they include:

| Measure | Result |
|---|---|
| `!include` positions | 17 as a merge value (`<<: !include …`), 32 as a key's value (`key: !include …`, e.g. `_common/lib: !include "../package-common.yaml"`). None in flow syntax or a sequence. |
| Include depth | 0 for 10 manifests, 1 for 23, **2** for 13. No deeper. |
| Aliases in an included file that it neither defines nor includes | none (only the upward direction is used) |
| `<<:` inside a block scalar (`|` or `>`) | none |

Every corpus include is in a position R1 splices. FR-004's depth limit of 8 is four times the deepest chain.

## R3. Merge keys: rename, then apply hpack's precedence (decision)

**Precedence, measured** (spec Background):
- an explicit key beats a merged one, wherever it is written;
- among merges, the first wins, for repeated `<<` keys and the sequence form alike.

**One more case, measured:** `<<: *s` where `*s` is a sequence of strings is **ignored** by hpack, which exits 0 and adds no dependencies.

**Decision**:
- **Before parsing**, rename each block-mapping key `<<` to `<<__waybill_merge_<n>`, numbered in document order. This lets two of them coexist in one mapping, where serde_yaml would reject the duplicate.
- **After parsing**, walk the value bottom-up. In each mapping, take the renamed keys in number order. Each key's value is a mapping, or a sequence of mappings taken in order. Insert each of their entries only where the key is still absent, then remove the renamed keys.
- That gives exactly the measured precedence: explicit keys are present first, and the first merge claims a key before later ones.
- A merge value that is not a mapping (or, in a sequence, an item that is not one) contributes nothing, as in hpack.

serde_yaml's own `Value::apply_merge` is not used. It cannot see the renamed keys, and its precedence would be one more behaviour to measure rather than the one already measured.

## R4. The rename touches only `<<` keys (decision)

The rename is a line-based pass with three rules:
- it renames `<<` only when it is the key on a line (after indentation, optionally after `- `);
- it skips the body of a block scalar (a line ending in `|` or `>`, optionally with a chomping or indentation indicator, and every following line indented more than that line);
- it skips lines inside a multi-line quoted scalar.

**Tests**:
- every non-`<<` line of a fixture is byte-identical after the pass;
- a `<<:` inside a block scalar or a quoted string is not renamed;
- reversing the rename restores the input exactly.

R2 found no hazard in the corpus, so these tests guard the general case.

## R5. Bounds and failure (decision)

- **The root.** `haskell::read` has `rootfs` and threads it to `hpack::read_manifest`. An include's path is canonicalised, which resolves symlinks, and must lie under the canonicalised root.
- **Cycles.** The resolver keeps the chain of canonical paths and refuses an include already on it.
- **Depth.** 8 levels, from R2.
- **Failure.** Any include that cannot be resolved (missing, outside the root, a cycle, too deep, unreadable, not in a splice position) is left as its `!include` tag. The existing `strip_tagged` path then removes it, and the manifest is `partial` (`unmodelled-construct: include`), with the include and its reason recorded (R6).
- **One limit on "keep what can be read"** (inference from R1): if the content after an unresolved include uses an anchor that only that include defines, the document cannot be parsed and is `unread` with serde_yaml's message, as today. Nothing in a missing file can be recovered, so the spec's edge cases record this.

## R6. C200 gains the includes (decision)

C200 is one JSON value, built in `HpackReadOutcome::to_document_value` (`hpack.rs:181`). All three formats carry it through the same annotation, and the parity extractors compare it whole (`parity/extractors/mod.rs:711`: `c200_cdx`, `c200_spdx23`, `c200_spdx3`). So added keys reach every format identically with no catalog change.

Two keys are added, each sorted by file then include:
- `includes-resolved`: `[{"manifest", "file", "include"}]`;
- `includes-unresolved`: `[{"manifest", "file", "include", "reason"}]`, with reason `missing`, `outside-root`, `cycle`, `too-deep`, `unreadable` or `unsupported-position`.

Both are omitted when empty. That keeps C200 byte-identical for a scan with no `!include` (FR-007), and leaves every #1677 target with an include as the only C200 change.

## R7. The positive control: agreement with hpack (decision)

`measurements/hpack_agreement.py` runs hpack 0.38.3 over each corpus `package.yaml`, in a copy of the repository with its generated `.cabal` removed (#1677's `Prepare`). It takes the dependency **names** of every `build-depends` stanza, unioned per package. It compares those with the names waybill's documents give the same package: the dependency edges of its `pkg:hackage` main module.

- Both sides come from the real tools; the script only compares sets.
- Version constraints are not compared. waybill keeps a range as declared (the reader's FR-007).
- Each mismatch is listed with its package, and explained or fixed (SC-002).

## R8. serde_yaml behaviour the design relies on (to verify at T-time)

Each of these becomes a unit test before the code that relies on it:
- serde_yaml 0.9.34 accepts an anchor redefined later in one document, and an alias takes the most recent definition (R1's measured hpack rule);
- a spliced document of the corpus's size is within serde_yaml's alias-expansion limits;
- a renamed key `<<__waybill_merge_<n>` survives parsing as an ordinary key.

## R9. Measure before the goldens move (decision)

FR-001's counts need the new reader, so the order is:
1. implement;
2. measure on local copies of the four public repositories, as committed and with the generated `.cabal` removed;
3. run R7's agreement;
4. compare everything with the committed #1677 goldens;
5. checkpoint, amending the spec if a count contradicts it;
6. only then regenerate the goldens through `public-corpus.yml`.

TheRock is not involved: it has no hpack manifests.
