# Baselines and recorded observations — m779

Evidence file for tasks that record rather than assert. Every entry is
something that was run, not something expected.

## T001 — pre-PR gate on the untouched branch (2026-09-08)

Branch `779-signing-identity-shapes` at `39eaf9b` (main, v0.7.0), no
source changes.

```text
suites:        297
passed:        5622
failed:        0
ignored:       16
stdout blocks: 0
>>> all pre-PR checks passed.
```

These are the numbers T041 must reproduce or exceed. Compare against them
directly rather than reading the trailing summary line — two gate runs
earlier in this project reported green while having silently executed a
fraction of the suite, and were caught only because the counts looked
wrong.

**Harness note**: the first attempt recorded only `EXIT=0`, because the
command redirected its own output to a file and the task log captured
just the exit line. `exit 0` with zero `test result:` lines is
indistinguishable from a real pass if only the exit code is checked. Read
the log the run actually wrote.

## T003 — fork clone verified (2026-09-08)

`kusari-sandbox/sigstore-rs` at tag `v0.11.0-waybill-1`, HEAD `f3ac508`.
`diff -r` against the vendored checkout at
`~/.cargo/git/checkouts/sigstore-rs-fe366a804a4aa93b/f3ac508/src`:
identical. The patch is being written against exactly what waybill builds.

## T007 — fork tests, and proof they have teeth (2026-09-08)

Post-patch, 7/7 pass:

```text
google_issuer_resolves_via_email          ok
github_actions_issuer_resolves_via_sub    ok
unknown_issuer_falls_back_to_sub          ok
no_usable_claim_names_what_was_looked_for ok
empty_claim_value_is_not_a_usable_identity ok
federated_issuer_prefers_connector_id     ok
federated_issuer_falls_back_to_iss        ok
```

Pre-patch probe — the same GHA-shaped token, against unmodified
`v0.11.0-waybill-1`:

```text
FAILED: Unable to parse identity token: Malformed JWT: claims JSON malformed
```

That is the exact string the defect produces in the field, reproduced
before the fix and gone after it. A test suite that passes on both the
broken and the fixed code proves nothing; this one does not.

## T009 — pin bump, and a regression the existing suite caught (2026-09-08)

First bump to `v0.11.0-waybill-2` failed the gate. `cisa_2026_signing`'s
m222 cleanup test expected a network-level failure and got an
identity-resolution failure instead:

```text
identity token carries no usable `sub` claim (issuer ``  selects it);
claims actually present: email
```

**A real regression in the patch, not a stale expectation.** Keying the
identity claim on the issuer and defaulting every unmapped issuer to
`sub` broke tokens that carry an email and no sub — GitLab, self-hosted
dex, and any token omitting `iss`. Those signed before this feature and
must keep signing (FR-002).

Fix: for an issuer *in* the map the mapping stays authoritative and
there is no cross-fallback, because Fulcio will use the mapped claim and
recording a different one would produce an identity that disagrees with
the certificate. For an issuer *not* in the map, `sub` is only a
default, so fall back to `email`.

Re-tagged `v0.11.0-waybill-3` (`b6d9765`). Fork tests 9/9, including two
new regression tests covering both halves of the rule.
`cisa_2026_signing` back to 15 passed / 0 failed.

Worth recording: this was caught by a test written for a *different*
milestone, asserting something incidental to its own purpose. It is the
clearest argument in this feature for not narrowing a gate run to the
suites you think you touched.

## T041 — final gate (2026-09-08)

```text
suites:        297   (baseline 297)
passed:        5633  (baseline 5622, +11 new m779 unit tests)
failed:        0
ignored:       17    (baseline 16, +1 new identity-gated test)
stdout blocks: 0
>>> all pre-PR checks passed.
```

Every delta reconciles against T001. Two lines matching `^error` appear
in the log; both are captured CLI stderr from tests that assert on
invalid-argument handling, not clippy failures. Checked rather than
assumed.

## Pending — require a staging credential

- **T002** baseline capture of current keyless behaviour
- **T010** no-regression probe with an email-carrying token
- **T011** the load-bearing probe: a token with no `email` claim

## Pending — require the pushed fork tag

- **T009** onward (pin bump). T008 is outward-facing and awaits confirmation.
