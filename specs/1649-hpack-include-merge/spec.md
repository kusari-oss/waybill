# Feature Specification: hpack `!include` and YAML merge keys are read as hpack reads them (#1673, #1674)

**Feature Branch**: `1649-hpack-include-merge`
**Created**: 2026-11-23
**Status**: Draft
**Input**: #1673 (an anchor defined in an `!include`d file makes a `package.yaml` unreadable, and `!include` is never resolved) and #1674 (YAML merge keys are never applied). Done together, because on real projects one construct is always used through the other. Refs #1677, whose corpus targets measure both.

## Background

waybill's hpack reader (`waybill-cli/src/scan_fs/package_db/haskell/hpack.rs`) parses each `package.yaml` alone into a YAML value, and then into a manifest.
- **`!include` is not resolved.** It is an "unmodelled construct": the tagged entry is removed and the manifest is reported `partial` in C200 (`waybill:hpack-read-outcome`). Anchors defined in an included file are unknown to the including file, so a manifest that uses one fails to parse at all and is reported `unread`.
- **Merge keys (`<<`) are never applied.** serde_yaml does not apply them unless asked, so a `<<: *anchor` is kept as an ordinary key named `<<` and ignored. Two `<<` keys in one mapping are a duplicate key, which serde_yaml rejects while parsing.

**Measured at `main` `ea149065`**, waybill on #1674's synthetic fixtures:

| Fixture | waybill | C200 |
|---|---|---|
| `library: { <<: *a }`, where `*a` holds `dependencies: [base, containers]` | the package's main module only; `base` and `containers` lost | `read`, not `partial` |
| `library: { <<: *a, <<: *b }` | nothing, the package itself included | `unread`, `malformed-yaml`: `duplicate entry with key "<<"` |

**How hpack resolves both, measured with hpack 0.38.3** (`nix run nixpkgs#hpack`):

| Input | hpack's `build-depends` | Rule |
|---|---|---|
| `<<: *a` then `<<: *b`, both setting `dependencies` (`[base]`, `[text]`) | `base` | among merges, the **first** wins |
| `<<: [*a, *b]` | `base` | the sequence form behaves the same as repeated keys, in order |
| `<<: *a` (`[base]`) with an explicit `dependencies: [text]`, either order | `text` | an **explicit** key beats a merged one, wherever it is written |
| `<<: *a`, `*a` holding `[base, containers]` | `base, containers` | a single merge |

- **On serokell/lootbox's `code/network/package.yaml`** (pinned in #1677): hpack resolves `<<: !include "../base/hpack/lib.yaml"`, which itself does `<<: !include "definitions.yaml"`, relative to `base/hpack/`. It then resolves `*lib-common`, an anchor defined two includes away, and renders the full `build-depends` (`async`, `base-noprelude`, `bytestring`, …).
- **So includes nest**, a path resolves relative to the **including** file, and anchors defined in an included file are visible to the content after the include.

**Why both issues land together**, counted in the #1677 corpus repositories:

| Repository | `package.yaml` | with `!include` | with `<<` | with both |
|---|---|---|---|---|
| hs-opentelemetry | 24 | 16 | 16 | 16 |
| lootbox | 7 | 6 | 6 | 6 |
| hs-temporal-sdk | 7 | 6 | 6 | 6 |
| ogmios | 8 | 8 | 0 | 0 |

- **hpack's pattern is `<<: !include "<defaults>.yaml"`.** Resolving includes without applying merges would turn those manifests from `unread` into `read` while silently missing every merged key, which is #1674's quiet failure, at scale.
- **Applying merges without includes changes no corpus component.** lootbox's one duplicate-`<<` manifest, `code/network`, would then fail on `*lib-common`.
- **ogmios uses `!include` with no merge**, so it isolates the include arm.

**The baseline is the #1677 goldens.** They are committed and filtered to `pkg:hackage/*`, with C200 counts given as seen / read / partial / unread:

| Target | Hackage | Main modules | C200 |
|---|---|---|---|
| `hpack-hs-opentelemetry` | 127 | 37 | 24 / 8 / 0 / 16 |
| `hpack-hs-opentelemetry-no-generated-cabal` | 89 | 21 | 24 / 8 / 0 / 16 |
| `hpack-lootbox` | 62 | 7 | 7 / 1 / 0 / 6 |
| `hpack-lootbox-no-generated-cabal` | 8 | 1 | 7 / 1 / 0 / 6 |
| `hpack-hs-temporal-sdk` | 73 | 7 | 7 / 7 / 6 / 0 |
| `hpack-ogmios` | 102 | 7 | 8 / 0 / 0 / 8 |
| `hpack-ogmios-no-generated-cabal` | 0 | 0 | 8 / 0 / 0 / 8 |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A manifest that merges shared settings keeps them (Priority: P1)

An operator scans a Haskell project whose `package.yaml` files pull shared settings in with `<<: *anchor`. Today the merged dependencies vanish, and the document says the manifest was read completely.

**Why this priority**: A silent loss reported as a complete read is the worst kind of defect: nothing tells the consumer the answer is wrong.

**Independent Test**: #1674's two fixtures, scanned. The single merge gives `base` and `containers`; the double merge is read, not unread.

**Acceptance Scenarios**:

1. **Given** `library: { <<: *a, source-dirs: src }`, with `*a` holding `dependencies: [base, containers]`, **When** it is scanned, **Then** `base` and `containers` are dependencies of the package, and C200 lists the manifest as `read`.
2. **Given** a mapping with two `<<` keys, **When** it is scanned, **Then** the manifest is `read`, and each merged mapping's keys apply.
3. **Given** two merges that both set `dependencies`, **When** it is scanned, **Then** the first merge's value is used, as hpack does.
4. **Given** a merge and an explicit `dependencies` in the same mapping, in either order, **When** it is scanned, **Then** the explicit value is used, as hpack does.

---

### User Story 2 - A manifest that includes shared files is read through them (Priority: P1)

An operator scans a project whose `package.yaml` files include a shared defaults file, which may define anchors and include further files. Today such a manifest is `unread`, and the package and its dependencies are missing.

**Why this priority**: This is the largest loss the #1677 corpus measures: 16 of 24 manifests in hs-opentelemetry, 6 of 7 in lootbox, and 8 of 8 in ogmios.

**Independent Test**: Synthetic `waybill-fixture-*` trees where a `package.yaml` includes a file that defines an anchor and includes another file. Each is read fully, and its dependency set equals hpack's for the same tree.

**Acceptance Scenarios**:

1. **Given** a `package.yaml` that includes a file defining `&common`, then uses `*common`, **When** it is scanned, **Then** the manifest is `read` and the anchored dependencies are emitted.
2. **Given** an include of a file that itself includes a sibling, **When** it is scanned, **Then** the nested path is resolved relative to the file that names it.
3. **Given** `<<: !include "defaults.yaml"` at the top level, **When** it is scanned, **Then** the included mapping merges into the manifest under the merge rules of User Story 1.
4. **Given** each corpus manifest that hpack renders, **When** it is scanned, **Then** waybill's dependency set for each component equals the `build-depends` hpack generates (SC-002).

---

### User Story 3 - An include that cannot be resolved is reported, not guessed (Priority: P2)

An include can name a missing file, a file outside the scanned tree, or a cycle. waybill must say so and keep what it could read, not invent the rest or drop the manifest.

**Why this priority**: The reader's existing contract (C200 `partial`) is that a knowably incomplete answer says it is incomplete.

**Independent Test**: Fixtures with each kind of failed include. Each manifest is `partial`, and the reason names the include and why.

**Acceptance Scenarios**:

1. **Given** an include of a file that does not exist, **When** it is scanned, **Then** the manifest is `partial`, and C200 records the unresolved include with its reason.
2. **Given** an include whose path resolves outside the scanned root, **When** it is scanned, **Then** it is not read, and the manifest is `partial`.
3. **Given** two files that include each other, **When** it is scanned, **Then** the cycle is detected, the manifest is `partial`, and the scan terminates.
4. **Given** a chain of includes deeper than the limit (FR-004), **When** it is scanned, **Then** the manifest is `partial`.

---

### User Story 4 - Nothing else changes (Priority: P2)

Manifests that use neither construct produce the same documents as on `main`.

**Why this priority**: The hpack reader runs on every `package.yaml`, and the change must be traceable.

**Independent Test**: The existing hpack tests pass unchanged. Every corpus difference against `main` is traced to a manifest that uses `!include` or `<<`.

**Acceptance Scenarios**:

1. **Given** a `package.yaml` with no `!include` and no `<<`, **When** it is scanned, **Then** the output is byte-identical to `main`'s.
2. **Given** the corpus, **When** it is scanned at this branch and at `main`, **Then** every difference is in a target whose manifests use either construct, and each is traced.

---

### Edge Cases

- **An include used as a value, not a merge** (`dependencies: !include "deps.yaml"`). hpack splices the included content at that position. waybill does the same, and the merge rules do not apply.
- **An anchor defined twice** (in the manifest and in an included file). The definition nearest before the alias wins, as in YAML. Research confirms this against hpack before it is relied on.
- **A merge whose value is not a mapping**, for example an alias to a sequence. waybill reports this as hpack does; research measures what hpack does.
- **`defaults:`** stays unmodelled: it is a network fetch (FR-001c of the reader). A manifest using it stays `partial`.
- **A tag other than `!include`** stays as today: removed and announced.
- **A generated `.cabal` beside the `package.yaml`.** The existing suppression rule is unchanged. A manifest that becomes readable suppresses its generated `.cabal`, as a manifest read today does.
- **Symlinks.** An include path is resolved to its real location before the scanned-root check (FR-004).
- **An anchor that only an unresolved include defines.** If the content after an unresolved include uses such an anchor, the document cannot be parsed, and the manifest stays `unread` with the parser's message, as today. Nothing in a missing file can be recovered (research R5).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001 (count before the goldens move)**: Before the corpus goldens are refreshed, the work MUST measure on the seven `hpack-*` corpus targets, using the new reader on local copies of the public repositories (research R9: the counts need the new reader, so they cannot come earlier):
  - per target, manifests moving between `read`, `partial` and `unread`;
  - the `pkg:hackage` components each would gain or lose, against the committed goldens;
  - **the positive control:** for every corpus `package.yaml` that hpack 0.38.3 renders, hpack's generated `build-depends` per component, compared with waybill's dependency set for the same component. This measures agreement; the same comparison is the acceptance test (SC-002).

  Where a count contradicts a requirement below, the requirement MUST be amended in this spec with the evidence before implementation.

- **FR-002 (merge keys)**: `<<` keys MUST be applied with hpack's measured precedence:
  - an explicit key in a mapping beats a merged one, wherever either is written;
  - among merges, the first wins, for repeated `<<` keys and for the sequence form `<<: [a, b]` alike.

  Repeated `<<` keys in one mapping MUST be read, not rejected. Whatever makes them readable MUST change nothing but `<<` keys, and a test MUST show that every other key, value, comment position and line in a manifest is unaffected.

- **FR-003 (`!include`)**: `!include "<path>"` MUST be resolved from disk:
  - relative to the directory of the file that contains it, at any nesting depth;
  - spliced at the position it appears, as a value or as a merge source;
  - with anchors defined in an included file visible to the content after the include, as hpack makes them.

- **FR-004 (bounds)**: An include MUST NOT be read when its resolved real path lies outside the scanned root, when it closes a cycle, or when it is more than **8** levels deep (research confirms 8 covers every counted corpus chain). In each case, and when the file cannot be read or parsed, the including manifest MUST be `partial`. Content that could be read is kept, and nothing is invented for the unresolved include.

- **FR-005 (reporting)**: C200 MUST record, per manifest, the includes it resolved and those it could not, with the reason for each unresolved one. Each entry names the manifest it belongs to as well as the file containing the include, so a nested include is attributed to its manifest. A manifest whose every include resolved and that uses no other unmodelled construct is `read`. The added content MUST appear identically in all three formats.

- **FR-006 (no network)**: Resolution reads only local files under the scanned root. `defaults:` stays unmodelled and `partial`.

- **FR-007 (byte identity)**: Output MUST be byte-identical to `main` for every `package.yaml` that uses neither `!include` nor `<<`.

- **FR-008 (no new field beyond C200)**: The dependency changes appear through the existing component and edge emission. The only document-level change is FR-005's addition to C200, which goes through the same parity path in all three formats.

### Key Entities

- **Include**: a `!include "<path>"` in a manifest or an included file, resolved or unresolved, with a reason when unresolved.
- **Merge key**: a `<<` entry whose value, an alias, an include or a sequence of them, contributes keys to its mapping under FR-002's precedence.
- **C200** (`waybill:hpack-read-outcome`): the document-scope record of each `package.yaml`: `read`, `partial` or `unread`, and now its includes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: #1674's fixtures: the single merge gives `base` and `containers`, and the double merge is `read`. The include fixtures, with anchors across files and nested includes, are `read` with the expected dependencies.
- **SC-002**: For every `package.yaml` in the seven `hpack-*` corpus targets that hpack 0.38.3 renders, waybill's per-component dependency names equal hpack's `build-depends` names. Every mismatch is listed in `measurements/` and either fixed or explained.
- **SC-003**: The corpus golden diffs against `main` are traced, component by component, to manifests that use `!include` or `<<`. The expected direction: `unread` → `read` in hs-opentelemetry (16), lootbox (6) and ogmios (8), and `partial` → `read` in hs-temporal-sdk (6), with `pkg:hackage` components recovered in the no-generated-cabal targets. The exact counts are FR-001's.
- **SC-004**: Each failed-include case (missing, outside the root, cycle, too deep) gives `partial` with its reason, in fixtures.
- **SC-005**: Mutations each fail at least one test: merge precedence reversed, explicit-over-merge reversed, the `<<` rewrite touching another key, includes resolved relative to the package rather than the including file, and the root bound removed.
- **SC-006**: `waybill sbom parity-check` agrees for every fixture and corpus target.
- **SC-007**: Scan time on the hpack corpus targets is within 1.05× `main`'s, median of at least 3 runs per arm.

## Assumptions

- hpack 0.38.3 (`nix run nixpkgs#hpack`) is the reference behaviour. Measurements run locally, because the corpus repositories are public; TheRock is not involved.
- Committed fixtures are synthetic `waybill-fixture-*` trees. Corpus repositories are measured, never copied into the repository.
- The #1677 goldens are the baseline and are refreshed through the `public-corpus.yml` regeneration, never locally.
- Out of scope: `defaults:` (network), #1675 (salvaging a name from an unreadable manifest), #1676 (root-only discovery), and #1678 (URL-pinned Stack snapshots).
- Zero new Cargo dependencies.
