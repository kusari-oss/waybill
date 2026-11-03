# What the corpus refresh should change — written before the run reported

Run: `public-corpus.yml` 41720889486, branch `1276-boot-library-scope`,
`regen_goldens=true`. Recorded before reading its artifact, so the diff is
checked against a committed prediction rather than one reconstructed after
the fact.

The reason for the ceremony: in milestone 1275 I predicted a scope change on
`nix-closure-moat` from finding F12, it did not happen, and my explanation
for why — runtime-reachability immunity — then measured false (0 of 23
targets are runtime-reachable). Being wrong twice about the same diff is
what a written prediction is for. Any mismatch below is a finding to run
down, not a detail to absorb.

## Measured, so stated flatly

**P1 — ten components lose their non-runtime scope**, across two targets:
`haskell-security-advisories` (8) and `nix-closure-moat` (2). This is a
direct count over the emitted documents (`baseline.md`), not an inference.
All ten were `transitive`.

**P2 — the other targets' goldens do not move at all.** Gating C202 on
applicability was adopted precisely because emitting it unconditionally
failed 35 golden byte-identity tests across 11 ecosystems. If a non-Haskell
target's golden moves, that gate is not doing what the 35-failure
measurement said it does.

## Inferred, so held loosely

**P3 — C202 appears on three targets**: `nix-closure-moat`,
`haskell-security-advisories`, `haskell-language-server` — the ones whose
`reader_ecosystems` include both Haskell and Nix, and so plausibly read a
pinned compiler configuration. **`haskell-aeson` is the uncertain one**: it
is Haskell without Nix in its reader set, and I have not checked whether it
carries a `flake.lock`. If C202 appears on it, the gate is keyed to
something broader than I think it is; if it appears on fewer than three, the
pinned-config read is narrower.

**P4 — C202's value is a component count, not a correction count.**
`compiler_supplied()` returns the set of boot-library components present in
the document, and `immune_compiler_supplied` is that set's length. So
`haskell-language-server` should carry a **nonzero** C202 with **no** scope
diff: the rule applied, exempted components, and none of them had been
mis-scoped. A zero there would mean either no boot libraries in the document
or a gate I have misread.

**P5 — the two I9 declarations in `invariant-declarations.toml` become
stale and must be deleted.** They exist only because the committed goldens
predate the fix, which is the recurring shape named three times in this
milestone pair: *a check that reads a committed golden cannot detect a
change the golden predates.* This is T027, deliberately left open because it
is verifiable only in this diff.

## How the diff gets audited

Per #1286, by enumerating the files each target actually wrote — a directory
upload contributes committed bytes for targets that never ran, so "51 files
changed, 0 differing" once hid a stale target. Per #1287, by the files
written rather than any SHA or branch the run reports, since a
`workflow_dispatch` run's `github.sha` is the workflow definition's commit,
not the dispatched ref's.
