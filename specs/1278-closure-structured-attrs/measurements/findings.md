# Findings — 1278-closure-structured-attrs

Things this milestone corrected about its own reasoning. Recorded because an
abandoned or mistaken step that leaves no trace gets repeated.

## F0 — I read a sentence without its opening clause and filed the result

**The error.** Across the spec, `research.md`, this directory and **public
issue #1348**, I claimed the corpus manifest's closure census was "a darwin
measurement under a linux label" — that it quoted 1,275 derivations while
calling them x86_64-linux. It does not. The comment reads:

```
// moat is the project milestone 1034/1035 measured against. Measured
// here before adding (aarch64-darwin, so counts differ from CI's
// x86_64-linux closure, which the goldens pin): 1,275 derivations,
```

It names its system in the same sentence as its figures and warns about
exactly the confusion I accused it of causing.

**How it happened, precisely.** I read the manifest with `sed -n 215,265p`.
Line 215 begins `// x86_64-linux closure, which the goldens pin): 1,275
derivations,` — mid-sentence. The qualifying clause `(aarch64-darwin, so
counts differ from CI's` is on line 214, one line above the window. The
fragment I saw is a grammatical sentence that says the opposite of the real
one, so nothing looked truncated.

**Why it survived.** It was load-bearing in a story I liked: a milestone about
a document that fails to record its own provenance, finding a census that
failed to record its own provenance. The finding fitted the thesis, so I
stopped testing it — and then cited it four more times, each citation making
it look better established.

**What it cost.** A false claim about a colleague's work in a public issue,
filed under someone else's name. Corrected in place, with the retraction left
visible rather than edited out.

**The rules this earns.**

- A windowed read is not a read. When quoting a comment as evidence, re-read
  it from its first line — `sed -n 215,...` cannot tell you what line 214
  said, and a mid-sentence start can produce a well-formed sentence with the
  opposite meaning.
- Repetition is not corroboration. The claim appeared in five artifacts
  because I copied it forward, not because five things attested to it.
- Suspect the finding that flatters the thesis hardest. This repository
  already records "a right count can carry a wrong cause"; this is its
  sibling — a right-looking quote carrying a wrong claim.

**What was actually true**, and did not need the invented defect: the goldens'
own C184 record states `1,749 / 256 / 101 / 39 / 1,353`, which is byte-for-byte
the probe's env-only arm. That is the strong control, and it was available
without accusing anything of being mislabelled.

## F1 — a count that was right about the wrong population

The first pass at "how many components wrongly claim to be absent from the
artifact" said **16**. It selected golden components whose *corrected* role was
`artifact-input` and never checked their role *before*. That swept in `hspec`,
`HUnit`, `hspec-discover`, `call-stack`, `quickcheck-io` and `haskell-lexer` —
whose roles do not change at all. Their `excluded` comes from dev-scope
lifecycle propagation, which is independent of this defect and remains
correct.

Tracing one of them is what exposed it: `hspec` is referenced via
`buildInputs` **from `env`**, a carrier the reader already read. Nothing about
it could have changed.

The defensible count is **9**, isolated by filtering on
`waybill:scope-classification-source = "closure-role"` — the exclusions
resting on the role and nothing else.

**The rule**: when a fix changes a classification, a finding about it must
compare before against after for the *same* item. Selecting on the after-state
alone measures the population that ends up in a state, not the population the
fix moves. This is the recorded "a right count can carry a wrong cause"
failure with the direction reversed.

## F2 — the control that was available all along

The spec was written with a control of 347-of-348 agreement between the
probe's env-only arm and a per-component view of the committed golden. The
golden carries the derivation-level census directly, in its own C184
annotation, and it matches the probe **exactly**: 1,749 / 256 / 101 / 39 /
1,353.

The weaker control was constructed by re-deriving per-component roles when the
artifact already stated the figure being checked.

**The rule**: before building a comparison, read what the artifact already
asserts. A derived view agreeing 99% is weaker evidence than a stated figure
agreeing exactly, and it costs more to produce.

## F3 — two formats deciding one claim twice

Milestone 1069 routed the per-ecosystem dependency-completeness claims through
one helper so CycloneDX and SPDX 3 could not drift. The **root** claim was
left behind: `build_compositions` recomputed the predicate inline while
`dependency_claims` computed it for SPDX 3.

Found by demoting the claim and watching only SPDX 3 change. The m1069
cross-format test did not catch it, because it exercises the per-ecosystem
claims and not the root one.

**The rule**: "the two formats read the same answer" is a property of each
claim, not of a module. A helper that exists to prevent drift prevents it only
for the callers that actually call it.

## F4 — two test defects with the shape of the bug under repair

Both self-inflicted, both fixed, and both worth recording because they are the
*same* failure mode this milestone exists to fix: a lookup that finds nothing
is indistinguishable from a thing that is not there.

1. The annotation helper searched for a `values` map. The wire envelope is one
   `{schema, field, value}` per annotation. The test reported "absent from
   SPDX 2.3" for an annotation that was emitted correctly.
2. The US3 test asserted that a typed-only identity in the `unreferenced` role
   would be emitted. Unreferenced members that apply no patch are dropped by
   design, so it failed for a reason unrelated to the name it was testing. It
   now uses a *referenced* member whose derivation `name` deliberately differs
   from its typed `pname`, so the assertion distinguishes reading the pname
   from falling back to the name.

**The rule**: a test asserting absence must first prove the mechanism can
observe presence. Both of these would have been caught immediately by a
positive control, which the final versions carry.

## F5 — adding a required field has a blast radius

`ClassifiedClosure` and `RawDerivation` are constructed at eleven sites across
six files, nearly all of them tests in other modules. Adding one required
field to each meant eleven mechanical edits before anything compiled.

Not a defect, and not worth a refactor here. Recorded because it is the cost
of the seam being shared, which is the same property that made the fix itself
a four-accessor change rather than a four-consumer change. The cost landed in
the direction worth paying.

## F6 — a shape test where a naming convention was the contract

FR-014 asks for "every field a derivation declares that holds store paths and
that role classification does not consult". Implemented literally, with a pure
shape test — does the value contain `/nix/store/`? — and justified in code
comments, in the catalog row and in the spec on the argument that a name
allowlist is what fell out of date and produced #1348.

Measured on the refreshed corpus golden, that disclosure read:

```
"declarations-unconsulted": 98342,
"derivations-affected": 1610,
"fields-unconsulted": ["CPATH","FILECMD","LD_LIBRARY_PATH","PATH","SHELL",
                       "builder","buildPhase","mkdir","src","stdenv", ...113 total]
```

Nearly every attribute of a derivation mentions a store path. Almost none
declares a dependency. A consumer reading "98,342 declarations unconsulted"
would conclude the document was worthless, when the truth is 554 dependency
entries were missed — so the disclosure was worse than useless, it was
actively misleading, in the field whose entire job is to be trustworthy.

**The argument was wrong because it conflated two allowlists.** The one that
fell out of date is the list of fields we *classify* — seven of roughly
twenty, which is #1346. Deciding which fields *are* dependency attributes is a
different question, and nixpkgs' naming convention is the actual contract for
it. A shape test cannot answer it.

Fixed by matching the convention (`*Inputs`, `deps<Host><Target>` with an
optional `Propagated` suffix) rather than enumerating members, so an attribute
nixpkgs adds later is still reported.

**Why no test caught it.** All 27 tests passed before the fix and after it.
Every fixture held only tidy fields — `buildInputs`, `nativeBuildInputs`,
`depsBuildTarget` — because they were modelled on the *shape* of real
derivations without their *mess*. A fixture built by picking the interesting
fields out of real data inherits the shape and discards exactly the noise that
breaks a shape-based rule.

`real_world_noise.json` now carries `PATH`, `SHELL`, `builder`, `buildPhase`,
`src`, `stdenv` and a coreutils binary path alongside one real dependency
attribute, and asserts that exactly one field is reported.

**The rule**: when a rule's correctness depends on what real inputs look like,
the fixture has to include the parts that are not interesting. "Modelled on
real data" is not the same as "containing what real data contains", and the
difference is invisible until something counts 98,342 of something.
