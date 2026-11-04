# Refresh prediction — 1278-closure-structured-attrs

**Committed before the `public-corpus.yml` dispatch (FR-016, task T043).** A
prediction written after the diff is a description of the diff, and cannot
catch a change nobody anticipated.

Scored in the "Outcome" column by task T045. Every row is falsifiable: it says
a number or a name, not "roughly".

## P1 — which targets move

Only `nix-closure-moat` changes materially. Every other corpus target is
byte-identical, because `RawDerivation` is constructed only from
`nix derivation show` output and no other target runs `--nix-closure`
(research R6).

**Outcome**: _pending_

## P2 — the derivation census

| role | today | predicted |
|---|---|---|
| artifact-input | 256 | 270 |
| build-tooling | 101 | 118 |
| both | 39 | 42 |
| unreferenced | 1,353 | 1,319 |

Read from the golden's own C184 record, which already states today's figures
exactly, so this is a prediction about a *delta* that was measured, not about
a baseline that was guessed.

**Outcome**: _pending_

## P3 — components emitted

362 → **375**. 13 identities referenced all along become emitted; no component
is removed. Named in SC-006: `hadrian`, `alex`, `hscolour`, `isl`, `gdbm`,
`byacc` among them.

**Outcome**: _pending_

## P4 — the nine retracted exclusions

`autoconf`, `automake`, `bash-interactive`, `docbook-xml`, `elfutils`,
`keyutils`, `libedit`, `tzdata`, `util-linux-minimal` lose their CycloneDX
`scope: "excluded"` and their `waybill:scope-classification-source`, because
their corrected role is `artifact-input` or `both` and m1277's rule asserts
nothing for those. The `excluded` count falls from 107 by exactly 9, to **98**.

**Risk flagged in advance**: 98 is the figure if and only if no *other*
component's role changes across the exclusion boundary. 14 components change
role in total and 5 of those moves are `build-tooling`→`both`, which stays
inside the exclusion set. If the count is not 98, the transition table is the
thing to re-derive, not this number.

**Outcome**: _pending_

## P5 — the transition table, per direction

5 `build-tooling`→`both`, 4 `unreferenced`→`artifact-input`, 3
`unreferenced`→`build-tooling`, 2 `artifact-input`→`both`. **Nothing moves
toward `unreferenced`.**

Predicted per direction deliberately: a total that matches by the wrong
transitions is the failure this milestone's own analyze pass flagged (F6).

**Outcome**: _pending_

## P6 — the disclosure

C204 `waybill:nix-closure-read` appears on `nix-closure-moat` in all three
formats with an identical payload, carrying `format-version: 4`.

**`fields-unconsulted` is deliberately unpredicted.** It is this milestone's
first measurement of #1346's coverage gap, and quoting a figure for it here
would be the derived-arithmetic-as-measurement error. Whatever it says is the
finding.

**Outcome**: _pending_

## P7 — the native claim

The `{"aggregate": "complete", "dependencies": ["nix-closure-moat@d069055"]}`
record disappears from the CDX document, and SPDX 3's matching
`Relationship.completeness` moves with it, because both now read
`claims.root_complete` from one helper.

**Outcome**: _pending_

## P8 — patches

Patch count rises from 188 by the 28 entries that 7 derivations declare in the
typed carrier, to **216**. The distinct-CVE count does not fall; it rises only
if one of those 28 filenames names a CVE, which has not been measured, so no
figure is predicted for it.

**Outcome**: _pending_

## P9 — invariants

The refresh may create new I-row offenders, since 13 new components arrive and
some may be unreachable or unexplained. **No prediction either way**: this is
genuinely unknown, and guessing it would put a number in a table headed as
data. T047 reconciles whatever appears.

**Outcome**: _pending_

## P10 — the most likely to be wrong

P3 and P8. Both assume the local closure and CI's agree on more than the
census, and the census is the only part confirmed byte-for-byte. Component and
patch counts pass through merging and patch attribution, where a difference in
nixpkgs between the local evaluation and the lane's would show up. If one row
misses, expect it to be one of these two.
