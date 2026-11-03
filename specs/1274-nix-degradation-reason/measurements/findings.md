# Phase 0 findings — milestone 1274

What was measured, and what survived contact with the implementation.
Re-runnable via `./reproduce-1283.sh` and `./reproduce-1283.sh --raw-stderr`.

Environment: Determinate Nix 3.20.0 on `aarch64-darwin`, nixpkgs
`a799d3e3886da994fa307f817a6bc705ae538eeb`, waybill 0.14.0.

---

## F1 — nix prints the stack trace first and the cause last

The finding the whole milestone rests on. `salient_stderr` took `position()`
— the **first** line matching its error predicate — and kept 300 characters
forward.

Measured on the real `callCabal2nix` refusal, as the `--raw-stderr` mode
prints it:

```
cabal2nix-refusal.stderr: 29 lines, 1411B, error lines [4, 28]
   position()  would start at line 4
   rposition() would start at line 28
   the IFD clause starts at byte 1281 of 1342
   under position():  window covers bytes 126..426   -> clause OUTSIDE
   under rposition(): window covers bytes 1152..1342 -> clause INSIDE
```

The first match is a bare `error:` opener; the explanatory clause is on the
last line. `position()` fills the window with the stack trace.

**The research doc quoted "byte 1320 of 1382, 1,155 past the window" and the
committed fixture measures 1281 of 1342, 855 past.** The difference is the
length of the temp-directory path interpolated into nix's messages, which
varies per run. The structural finding — first match is the opener, clause is
outside the window, `rposition` reaches it — is invariant, which is why the
probe prints the offsets rather than the spec asserting them.

### F1a — it is not a one-word change

`data-model.md` §3 presents the fix as `position` → `rposition`, one word.
`rposition` requires `ExactSizeIterator`, which `str::Lines` does not
implement, so the lines must be collected before the search. One decision,
four lines. Recorded because the next person reading the plan will reach for
the same edit and get `E0277`.

It also bit twice in one session: `cargo build --bin waybill` compiled fine
with the same mistake still present in a `#[cfg(test)]` body, because
`--bin` does not compile test code. The gate's `clippy --all-targets` does.

## F2 — the false positive is real, and it is ordinary use

A flake whose `nixConfig` requests the setting and then fails for an
unrelated cause:

```
warning: Git tree '/FIXTURE' has uncommitted changes
warning: ignoring untrusted flake configuration setting 'allow-import-from-derivation'.
Pass '--accept-flake-config' to trust it
error: attribute 'waybillFixtureNoSuchAttribute' missing
       at /FIXTURE/flake.nix:6:40
```

The setting's **bare name is present**; the consequence clause is **absent**.
A classifier matching the name anywhere in stderr reports a missing attribute
as a policy refusal — one uninformative answer traded for one misleading
answer, which FR-005 calls worse than the status quo. `slack-web` sets this
option, so this is not a contrivance.

Both fixtures are committed verbatim at
`waybill-cli/tests/fixtures/nix_ifd/`, with absolute temp paths rewritten to
`/FIXTURE` and nothing else edited.

## F3 — the blast radius was not where the spec implied

The spec expected corpus goldens to carry C190. Measured before touching
anything (`baseline-0.14.0.md`):

```
$ grep -rl "nix-closure-degraded" waybill-cli/tests/fixtures/public_corpus/
(no output)
```

Only a target scanned with `--nix-closure` can carry it — `nix-closure-moat`
alone among the 17 — and its evaluation succeeds. The real FR-016 population
was **six assertions in one suite**, `nix_config_closure.rs`, which were
corrected to read the `reason` field rather than pinned. Confirmed after the
change: `git diff --name-only waybill-cli/tests/fixtures/public_corpus/` is
empty.

## F4 — the control's numbers, for SC-003

The handwritten control flake, pre-change: **1,170 derivations → 304
components, no degradation recorded.** Re-measured post-change in the probe's
own output, which is how SC-003 is checked rather than asserted.

## F5 — the planning documents were wrong about the coupling

Research R2 claimed the selection rule and the classifier were not
independent: *"with `position()` the classifier would be fed the bare
`error:` opener and could never match the clause… R1 has to land for R2 to be
testable at all."* `tasks.md` built its phase ordering on that, and
`data-model.md` §4 said FR-005a's error-line condition was *"satisfied by the
selection rather than by a separate check"*.

**FR-005c dissolved the coupling.** Because the refusal must win even when
another error is printed after it, the classifier has to search every error
line for the clause, independently of where the detail starts — and so it
checks `is_error_line` itself rather than inheriting it. Mutation M1
confirmed this directly: reverting to `position()` broke the *detail* tests
and left `refusal_is_named_in_the_document` passing.

The ordering decision was therefore unnecessary, though harmless. See
`mutations.md` for the measurement.

## F6 — one rule, including a log field nobody was looking at

FR-008 asked for one selection rule across both failure paths. Implementing
the "exactly one rule" assertion as a source-level test found a **third**
site: `closure/mod.rs`'s `tracing::debug!` for an unlistable output, still
carrying `stderr.trim().chars().take(200)`.

It is a debug field rather than a document detail, so it was outside the
milestone's scope — but it had the same defect, and the alternative was an
exception in the test. It now uses the one rule. An exception is how the next
copy gets in.

## F7 — a single nix version, stated as a limit

The wording was not verified across nix versions. One version is what this
repository builds against; a second would not establish a range; and the
requirement is already "degrade on a rewording" (FR-005b) rather than "match
every version", which is satisfied structurally — a literal clause match with
no fallback guesswork. The guard is that the probe is committed and exits
non-zero when nix no longer refuses, so a wording change surfaces as a loud
probe failure rather than as a silently unreachable classifier.
