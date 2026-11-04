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


---

# Scorecard (task T045)

Scored against run **41882589598**, the second regen. The first,
**41880841818**, is not scored: it exposed finding F6 — a shape-only FR-014
filter reporting 98,342 unconsulted declarations — and its goldens were never
installed.

| # | prediction | outcome |
|---|---|---|
| P1 | only `nix-closure-moat` moves | **HIT** — 51 files, 48 byte-identical, the 3 changed all `nix-closure-moat` |
| P2 | census 270 / 118 / 42 / 1,319 | **HIT, all four cells exactly** |
| P3 | components 362 → 375 | **MISS** — 379 |
| P4 | `excluded` 108 → 98 | **MISS** — 113, and in the opposite direction |
| P5 | 5 / 4 / 3 / 2 by direction | **2 of 4** — `unreferenced`→`artifact-input` 4 ✓, `artifact-input`→`both` 2 ✓, `build-tooling`→`both` 2 (predicted 5), `unreferenced`→`build-tooling` 2 (predicted 3). Nothing moved toward `unreferenced`, as predicted |
| P6 | C204 present, `format-version: 4`, population unpredicted | **HIT**, and the population is the finding — see T048 below |
| P7 | the native `complete` claim disappears | **HIT** — `['complete']` → `[]` |
| P8 | patches 188 → 216 | **MISS** — 212. distinct CVEs held at 13, as required |
| P9 | invariant effect, deliberately unpredicted | open until the compare run |
| P10 | P3 and P8 are the likeliest misses | **HIT — both missed.** It did not anticipate P4 or half of P5 |

**8 of 14 scored cells hit; the census, which mattered most, hit exactly.**

## P4 — the miss worth understanding

Predicted down 9, observed **up 5**. Decomposed rather than explained away:

```
new components carrying excluded:          +13
removed components that had excluded:       -2
surviving components that LOST excluded:    -6
surviving components that GAINED excluded:  +0
                                      net:  +5   (observed +5)
```

The prediction reasoned only about retractions and never about the 23 new
components — most of which are build tooling, and are therefore correctly
excluded on arrival. A net count was the wrong instrument: the two populations
move independently and the prediction conflated them.

## P4 also corrects a count of mine — nine was six

SC-003 named nine components as carrying a false `excluded`. **Six did.**

```
bash-interactive     unreferenced  -> artifact-input
elfutils             unreferenced  -> artifact-input
keyutils             unreferenced  -> artifact-input
libedit              unreferenced  -> artifact-input
tzdata               build-tooling -> both
util-linux-minimal   build-tooling -> both
```

The other three were never false:

- **`autoconf`, `automake`** already carried `('both', no scope)` in the
  committed golden. There was no exclusion to retract.
- **`docbook-xml`** does not change role in the golden at all. It is the one
  component the local closure disagreed with the golden about — already noted
  as the 1-of-348 residue, and then used as evidence anyway.

**How the error was made**: I read `scope` from the **golden** while computing
roles from my **local** closure, and joined the two by component name. Name
keying also collapses same-named components of different versions. It is F1's
error in a new disguise — a population assembled from two different sources
and trusted because the arithmetic looked tidy.

Corrected in SC-003, in the corpus expectations, and in issue #1348.

## P3 and P8 — and the thing neither predicted

379 not 375, and 212 not 216. P10 called both, for the stated reason: they pass
through merging and patch attribution, where the local evaluation and the
lane's can differ.

What no row predicted, and is the most useful result after P2: **six
components gained a correct name/version split.**

```
ghc-9.10.3                  -> ghc
git-minimal-2.54.0          -> git-minimal
krb5-1.22.1                 -> krb5
python3-3.13.13             -> python3
python3-minimal-3.13.13     -> python3-minimal
python3.13-pystemmer-3.0.0  -> pystemmer
```

Their `pname` lived only in the typed carrier, so the emitter had been falling
back to the derivation's own name and producing a single blob where a name and
a version belong. That is US3 doing more than it was specified to do, and it
is why the component delta is +23/−6 rather than a clean +13.
