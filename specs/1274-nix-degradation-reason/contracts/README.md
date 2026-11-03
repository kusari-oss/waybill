# Contracts: the nix-closure tier says why it degraded

**Milestone**: 1274

waybill exposes no API. Its interfaces are the **emitted document** and the
**CLI**, so these are stated as properties of documents, each testable by
reading one document with no access to the scan log — which is what FR-002
and FR-002a are for.

Each contract names its requirement and the behaviour whose removal must
break a test (FR-015).

## C1 — the CLI surface does not change

FR-011, FR-012. No flag, no subcommand, no environment variable. `waybill
sbom scan --help` byte-identical. The refusal does not become configurable:
whether to offer an opt-in is #1283's second question and depends on #971 and
#1034.

## C2 — a refusal is named as a refusal, in the document

FR-001, FR-002, SC-001. Scanning a flake whose default package requires
import-from-derivation records
`reason = "import-from-derivation-required"` in the emitted document.

Measured today: `evaluation-failed`, with no mention of
import-from-derivation anywhere in the document.

## C3 — the document carries the explanatory line

FR-002a, FR-007, SC-002. The same document's C190 value carries a `detail`
containing Nix's own clause about the disabled option, readable **without the
scan log**.

Measured today: the document carries no detail at all, and the log's detail
contains `warning: Git tree '…' has uncommitted changes` while the
explanatory line appears nowhere.

## C4 — the detail is the cause, not the stack trace

FR-007. Where Nix emits a stack trace before the cause, the recorded detail
contains the **cause**.

This is the contract R1 exists for. Nix prints the trace first and the cause
last: on the measured refusal the cause is on line 25 of 26, byte 1320 of
1382, and a rule starting at the first error line fills its window with
1,155 characters of trace instead. A test asserting only "the detail is not
the git warning" would pass on the trace, so the assertion must name the
clause.

## C5 — one rule, both paths

FR-008, SC-005. The closure path and the version-resolution path select the
explanatory line by the same rule, and produce the same detail for the same
stderr.

The defect this milestone fixes is that they do not. Two implementations of
the rule would reintroduce it, which is why the rule moves rather than being
copied — and why `guard()`'s own comment, two definitions from the drift,
says *"two copies would drift, and the drift would be exactly the thing the
guard prevents."*

## C6 — a non-refusal keeps the generic reason

FR-005, FR-005a, SC-004. An evaluation failure that is not a refusal records
`evaluation-failed`.

Including the measured false positive: a flake whose `nixConfig` requests
`allow-import-from-derivation` and then fails on a missing attribute emits a
**warning** naming the setting. A classifier keyed on the bare setting name
reports that as a policy refusal, and FR-005 is explicit that trading one
uninformative answer for one misleading answer is worse than the status quo.
`slack-web` sets this option, so the case is ordinary rather than contrived.

## C7 — a refusal alongside another error is still a refusal

FR-005c. Where stderr carries both, the reason is the refusal and the detail
retains the other error's line.

Precedence is fixed by the requirement, not by whichever branch runs first,
and **not** by the order Nix printed the lines in — that order is not a
stability contract. Keeping both lines is what stops the policy note masking
a genuine flake bug.

## C8 — a reworded Nix message degrades, it does not mislead

FR-005b, FR-006. If the clause changes, the reason falls back to the generic
one. No panic, no failed scan, no guess.

The matched text is an external behaviour of a tool outside this repository,
and `measurements/reproduce-1283.sh` is committed so the match can be
re-verified when Nix changes.

## C9 — a flake that evaluates is untouched

FR-010, SC-003. A flake that does not need import-from-derivation produces
byte-identical output: no degradation recorded, and the same components.

Measured on the control: 1,170 derivations classified, 304 components. The
control is not decoration — a `callCabal2nix` flake that degrades proves
nothing on its own, because a broken fixture degrades too.

## C10 — every other reason keeps its wire string

FR-013. `offline-requested`, `tool-absent`, `tool-unusable`,
`ifd-refusal-unverified`, `revision-unfetchable`, `no-evaluable-attribute`,
`several-system-configurations`, `evaluation-failed` and `budget-exceeded`
are unchanged in value and meaning.

`ifd-refusal-unverified` is the one to watch: it means the refusal could not
be **confirmed to be in effect**, which is close to the opposite of the new
reason. Conflating them would tell an operator their Nix is too old when in
fact their flake needs a grant they have not given.

## C11 — no catalog row is added, and no parallel channel

FR-002b, FR-004, FR-014, R3. FR-004 forbids a second channel: the reason and
the detail ride the annotation consumers already key on, rather than a new
one beside it. C190 already exists with `c190_cdx`, `c190_spdx23` and
`c190_spdx3`. Extending a row is not adding one.

The row's documented prose says "document-scope string" and must change with
the shape — enforced by `mapping_doc_bidirectional`, so it is a task rather
than a hazard.

## C12 — the old shape's assertions are corrected, not pinned

FR-016. `tests/nix_config_closure.rs` asserts C190's bare-string value in six
places across five tests. Each is corrected to read the `reason` field; the
two `None` cases still mean "no degradation" and change only in type.

That suite is also where C10 is enforced, so the corrected assertions are
what prove the existing wire strings survived.

## C13 — every signal is logged as well as emitted

FR-003. The reason and the detail both reach the log, so an operator can act
before the SBOM is published rather than only discover it afterwards.

Asserted on the log separately from C2/C3, which are asserted on the document
with the log discarded. Milestone 1273 found that a single assertion covering
both halves lets either one rot.
