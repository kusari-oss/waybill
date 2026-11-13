# Contract: C206 `waybill:closure-member-role` (spec 1380)

**Scope.** Per-component, on a closure member that declares no `pname`. The CDX property, SPDX 2.3 annotation and SPDX 3 annotation carry the same value. Parity is `SymmetricEqual`.

**Values.** These five, frozen again after this change:

- `source`: a fetch (content-hash identity), or a source tree, patch or vendored tree that was built rather than fetched (recipe-hash identity).
- `package`: a built, version-bearing package that is not one of the below.
- `check`: a version-check (`*-get-version-<v>`) or test-program (`*-simple-program-<v>`) derivation.
- `scaffolding`: `writeText` output.
- `fragment`: a member nothing else claims.

**Graph.** `source`, `package` and `check` are dependency-graph endpoints. The projection walks through `scaffolding` and `fragment`, which is why those carry an orphan reason (FR-014 of m1282).

**Rules disclosed in the catalog row:**
- **By evidence:** a fetch's content hash; `writeText`'s `text`; being listed in another derivation's `patches`; nixpkgs `srcOnly` phases.
- **By name, where no evidence exists:** the `-source`/`-src-<v>`/`-vendor` suffixes and the two check segments, applied only within the version-token population.

**C184.** `member-roles` reports a count for each of the five values.

**Compatibility.** A consumer matching on the four earlier values sees members that were `package` become `source` or `check`. No member that held any other value changes.
