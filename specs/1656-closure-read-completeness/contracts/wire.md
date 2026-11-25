# Wire contract

## C105 `waybill:graph-completeness-reason`: two new codes

- `closure-declarations-unconsulted: <N> declaration(s) in <M> derivation(s)[, <K> carrier disagreement(s)]`
  - Present exactly when `waybill:nix-closure-read` is present. Its N and M equal that row's `declarations-unconsulted` and `derivations-affected`.
- `closure-contents-unread: <U> of <T> member(s)`
  - Present exactly when C213 `waybill:nix-closure-contents` has `absent + partial + failed > 0`.
  - U equals that sum, and T equals C213's `members`.

Both are appended after every other clause, in that order. There are counts only, never paths or field names: the field names stay in `waybill:nix-closure-read`.

## C104 `waybill:graph-completeness`

`partial` whenever either code is present. An empty component set that is already `unknown` stays `unknown`, the rule spec 1652 set for the scope drop.

## Unchanged

- CycloneDX `compositions[]`, SPDX 3 relationship `completeness`, and the SPDX annotations that mirror the root claim. These already withheld the claim; the predicate behind them is now shared but its value is the same.
- `degraded_ecosystems`.
- Every non-closure document.
- Catalog row count. C105 is an existing row; only its vocabulary grows, and the row text and `specs/158-graph-completeness/contracts/graph-completeness-vocabulary.md` list the two codes.
