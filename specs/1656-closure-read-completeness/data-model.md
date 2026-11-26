# Data model

Everything here is in process for one scan. The only things that reach the wire are the C104 and C105 values.

## ContentsUnread

`{ unread: usize, members: usize }`, built from C213's `ContentsRecord`.
- `unread` is absent + partial + failed; `members` is the total.
- It exists only when `unread > 0`, which is today's `has_unread()`. Otherwise the field is `None`.
- It replaces `nix_closure_contents_unread: bool` in `ScanArtifacts` and in the CycloneDX builder.

## ClosureReadGaps

`{ declarations: Option<(declarations_unconsulted, derivations_affected, carrier_disagreements)>, contents: Option<ContentsUnread> }`.
- `declarations` is `Some` exactly when `ReadOutcome::has_gap()`.
- `any()` is the single predicate behind `closure_read_partial()`, CycloneDX's root claim, and the new reason codes.

## Reason codes (C105)

| Variant | Wire code | Detail |
|---|---|---|
| `ClosureDeclarationsUnconsulted { declarations, derivations, carrier_disagreements }` | `closure-declarations-unconsulted` | `<N> declaration(s) in <M> derivation(s)`, plus `, <K> carrier disagreement(s)` when K > 0 |
| `ClosureContentsUnread { unread, members }` | `closure-contents-unread` | `<U> of <T> member(s)` |

- **Order:** appended after every existing clause, declarations before contents.
- **Ecosystems:** neither degrades any (research R3).
