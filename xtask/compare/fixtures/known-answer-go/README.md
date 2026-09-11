# Known-answer fixture (Go)

The harness scores *itself* against this before it scores any tool
(FR-012). If it cannot recover the set in `EXPECTED.txt` exactly, the run
aborts and nothing is measured.

Every wrong conclusion in the comparison that motivated milestone 780 was a
harness defect rather than a tool defect. An instrument that cannot
demonstrate its own accuracy has no standing to rank anything.

The module set is fixed **by construction**: the files below are the whole
truth, there is no resolution step, and `EXPECTED.txt` is hand-authored
rather than generated. A generated expectation would move together with the
bug it exists to catch.

Two versions of `waybill-fixture-multi` are present deliberately. They are
what makes a regression to version-stripped identity fail here, loudly,
rather than silently changing every subsequent number.

Names use the `waybill-fixture-*` convention: no real coordinates, so the
fixture never trips an advisory scan.
