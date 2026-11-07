# Feature Specification: `unaddressable` counts patches and tarballs as software

**Feature Branch**: `1374-exclude-members-from-coverage`
**Created**: 2026-11-07
**Status**: Draft
**Issue**: #1380

> **Numbering note.** Spec directory numbers and GitHub issue numbers are
> separate sequences. This is spec **1374**; it fixes issue **#1380**.
> Issue **#1374** is the unrelated member-role-by-name-shape question.

## Summary

Milestone 1373 (#1375) replaced `eligible_unmapped` with a three-way
partition so that "software no feed indexes" would stop being conflated with
anything else. On the pinned closure its `unaddressable` reads **733**.

**539 of those are closure members** — `aeson-2.2.4.1.tar.gz`,
`CVE-2019-13232-1.patch`, `Python-3.13.13.tar.xz`, `add-flags.sh`. They are
not software. The honest figure is **194**.

The CPE synthesizer already excludes them and says why, at
`waybill-cli/src/cli/scan_cmd.rs:6616`:

> a closure member is a patch file, a source tarball or a build fragment,
> **not installed software**. `bash-5.3.tar.gz` is named after bash and is not
> bash.

Milestone 1373's counting loop does not apply that exclusion, so a thing the
synthesizer declines *on the grounds that it is not software* is then counted
as software no feed can address. `unaddressable` answers two questions at
once, which is the exact conflation C209 exists to prevent.

## This is the same error, one population over

During milestone 1373's implementation, nine corpus targets disagreed with
their own emitted row because **file-tier** components were being counted.
The requirement was scoped to "every **resolved** component" with this
reasoning:

> `utils.sh` is a file, not software a vulnerability feed could address, and
> counting one `unaddressable` would make that word mean two different things
> in one row.

That applies verbatim to a closure member. The neighbouring case was missed.

**It is worth recording why it was missed**, because the difference is
instructive: the file-tier over-count made the emitted row **disagree with
the emitted document**, so a cross-check found it. The member over-count
makes nothing disagree — both sides count them — so no check in the suite
could see it. It surfaced only when someone asked what the unaddressable
components *were*.

## The delta, measured

`nix-closure-moat` at `d069055`, aarch64-darwin, waybill `7f47ba66`.

| bucket | now | after | delta |
| --- | --- | --- | --- |
| `with_candidates` | 78 | **78** | 0 |
| `addressable_without_cpe` | 166 | **166** | 0 |
| `unaddressable` | 733 | **194** | **−539** |
| total in the partition | 977 | **438** | −539 |

**Only `unaddressable` moves**, and that is a property worth asserting rather
than observing: no member carries a CPE (the synthesizer's guard), and every
member is `pkg:generic`, which no feed indexes. A member landing in either
other bucket would mean one of those two facts had changed.

Excluded by role: `source` 455, `fragment` 65, `package` 12, `scaffolding` 7.
Even the twelve with role `package` are a patch file, four LLVM source trees
and a Python env — which is #1374's observation from the other side.

## The excluded population is already in the document

`waybill:nix-closure` (C204) carries `member-roles` with exactly these four
counts summing to 539, and `components-without-member-role: 411`. The
arithmetic closes: **411 non-member closure components + 27 from the
project's own readers = 438**, the new partition total.

So nothing is hidden by the exclusion. What changes is that a reader checking
C209's sum against the component count will now find a gap, and must know to
look at C204 to explain it.

## Clarifications

### Session 2026-11-07

- Q: Removing 539 members leaves C209's total short of the resolved component count. How should the gap be closed? → A: C209 carries an `excluded_members` count of its own, so the record stays verifiable without a cross-reference
- Q: File-tier components are excluded too and get no count — 117 on `image-postgres16`, present on 11 of 17 corpus targets. Should they be counted? → A: Yes, a separate `excluded_files`, so the sum holds against the raw document on every target

### The resulting shape

```json
{
  "schema": "waybill-cpe-coverage/v3",
  "with_candidates": 78,
  "addressable_without_cpe": 166,
  "unaddressable": 194,
  "excluded_members": 539,
  "excluded_files": 0
}
```

**Five counts, summing to the document's own component count** — not to a
filtered subset a reader has to derive. On the pinned closure
`78 + 166 + 194 + 539 + 0 = 977`; on `rust-ripgrep` `61 + 0 + 7 + 0 + 7 = 68`.
The property milestone 1373 was built for — a row whose sum a reader can
check — survives this exclusion instead of being traded away for it.

`excluded_members` duplicates a total C204 already implies, and that is
accepted deliberately: C204 carries the *per-role* breakdown (`source` 455,
`fragment` 65, `package` 12, `scaffolding` 7) while C209 carries only the
total it needs to balance its own books. A reader wanting to know *what* was
excluded goes to C204; a reader wanting to know the row adds up does not
have to. It also keeps the property true where C204 does not exist at all: a
non-closure scan has no closure record, and `excluded_members: 0` states
"nothing was excluded" where a missing field leaves it to inference.

**Two exclusions, two counts, deliberately not one.** A single `excluded`
field would sum identically and collapse "this is a source tarball" into
"this is a file" — the same conflation this milestone exists to undo, one
level down.

### Measured: file-tier is not a hypothetical population

| target | components | file-tier |
| --- | --- | --- |
| `image-postgres16` | 263 | **117** |
| `haskell-language-server` | 399 | **29** |
| `maven-guice` | 61 | 13 |
| `rust-ripgrep` | 68 | 7 |

Present on **11 of 17** corpus targets. The current row already excludes them
— verified: `row-sum + file-tier = total components` holds exactly on
`rust-ripgrep` (61 + 7 = 68) and `haskell-language-server` (370 + 29 = 399) —
so the count is well defined today and merely unreported.

### The schema goes to v3, not an amended v2

`waybill-cpe-coverage/v2` shipped in **no release**: the newest tag
(`v0.14.1-nightly.20261107`) was built at 2026-11-06T23:12 and milestone 1373
merged at 2026-11-07T07:35. Amending v2 in place was therefore available and
is **rejected**: v2 is already in 33 local and 48 corpus goldens in git
history, and reusing one version string for two different shapes is the exact
failure a version string exists to prevent. A version that lived for one day
is a cheaper thing to spend than a version number that means two things.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — an operator sizes the CPE gap (P1)

An operator reads `unaddressable` to decide whether curated-CPE work is worth
doing, and how much.

**Why this priority**: it is the number the row exists to provide, and it is
currently wrong by 3.8×. Issue #1381 scopes exactly this work and would quote
733 where the real target is 194 across 185 names.

**Independent test**: scan the pinned closure, read C209, and confirm the
unaddressable count matches a hand count of components that are software,
carry no CPE, and whose PURL type no feed indexes.

**Acceptance**
1. **Given** a closure with members, **When** C209 is read, **Then** no
   component carrying a member role is in any of the three counts.
2. **Given** the same scan, **When** C204's `member-roles` is read, **Then**
   its total plus C209's total equals the resolved component count.

### User Story 2 — the row still adds up on its own (P2)

**Why this priority**: milestone 1373's selling point was a row whose sum a
reader can verify without leaving it. `excluded_members` is what preserves
that across this change.

**Independent test**: the four counts sum to the resolved component count,
and the check needs no other record — including on a document that has no
closure record at all.

### Edge cases

- **A non-closure scan** — there are no members, so the counts are unchanged
  and the gap is zero. This is the common case and must stay byte-identical.
- **A closure where every component is a member** — all three counts are 0.
  They must be emitted as 0, not omitted; absence means something else.
- **A member that somehow carries a CPE** — cannot happen today (the
  synthesizer's guard), but if the guard changed, the member must still be
  excluded, because membership is about what the component *is*, not about
  whether something was synthesized for it.

## Requirements *(mandatory)*

- **FR-001**: Components carrying a closure member role MUST be excluded from
  all three C209 counts.
- **FR-002**: The **five** counts MUST partition the document's components —
  `with_candidates`, `addressable_without_cpe` and `unaddressable` over
  software, `excluded_members` over closure members, `excluded_files` over
  file-tier entries — summing to the emitted component count and to nothing
  else. The sum MUST be checkable against the document as it stands, with no
  filtering rule the reader has to know.
- **FR-003**: The exclusion MUST be by membership, not by proxy. A member is
  excluded whether or not it carries a CPE, a version, or an addressable PURL
  type, because the reason is what it is rather than what was done to it.
- **FR-004**: `with_candidates` and `addressable_without_cpe` MUST NOT change
  on the pinned closure. Only `unaddressable` moves.
- **FR-005**: A scan with no closure MUST produce a byte-identical document.
- **FR-006**: No component's identifiers change. No `cpes` entry is added,
  removed or modified.
- **FR-007**: `excluded_members` MUST equal the total of C204's
  `member-roles` on any document carrying both, and the relationship MUST be
  documented so a reader knows where the per-role breakdown lives.
- **FR-008**: The schema version MUST become `waybill-cpe-coverage/v3`. v2
  MUST NOT be amended in place even though it has shipped in no release,
  because it is already in 81 committed goldens and one version string must
  never describe two shapes.
- **FR-009**: `excluded_files` MUST be counted where the file-tier set is
  known. It is **not** available where the other counts are computed —
  file-inventory mode is parsed well after that point — so the record is
  finalised later rather than the count being guessed or omitted.

## Success Criteria *(mandatory)*

- **SC-001**: On the pinned closure the row reads `78 / 166 / 194`, and the
  194 is reproduced by an independent recompute from the emitted document.
- **SC-002**: The five counts sum to the emitted component count on **every**
  corpus target — `78 + 166 + 194 + 539 + 0 = 977` on the pinned closure,
  `61 + 0 + 7 + 0 + 7 = 68` on `rust-ripgrep`; asserted, not commented. On any
  document carrying C204, `excluded_members` equals its `member-roles` total.
- **SC-003**: `with_candidates` and `addressable_without_cpe` are unchanged;
  measured before and after, not argued.
- **SC-004**: Zero components change `{purl, cpe, cpes}`, compared as a set.
- **SC-005**: A scan with no closure differs in no JSON path at all.
- **SC-006**: Every corpus target's row agrees with an independent recompute
  of the same document.

## Assumptions

- The member-role annotation is the authoritative signal for "not software".
  Issue #1374 questions whether the *role values* are assigned correctly, but
  not whether a member is a member; this feature depends only on the latter.
- Closure members are the last such population. File-tier entries and members
  are now both excluded; if a third appears, the same reasoning applies and
  the same check — recomputing the row from the document — would find it.

## Out of scope

- Re-examining how member **roles** are assigned (#1374).
- Growing `GENERIC_LIBRARY_CPES` (#1381), which this feature's corrected
  figure is an input to.
- Any change to which components are emitted, or to their identifiers.
