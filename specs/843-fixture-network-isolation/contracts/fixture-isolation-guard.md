# Contract: fixture network-isolation guard

Feature: `843-fixture-network-isolation` · Covers FR-007, FR-008.

The guard exists so the defect cannot come back quietly. It came in
originally not by decision but by accretion — each new Go fixture
declared a module path like the ones before it, and nothing objected.

---

## G-1 — What it detects

**G-1.1** A Go fixture manifest declaring a `require` with no
corresponding local `replace` MUST fail the guard.

**G-1.2** The failure message MUST name the offending manifest and say
what to do about it. A guard that says only "fixture isolation
violated" costs the next contributor the same investigation this
feature spent.

**G-1.3** The guard MUST run in the ordinary test suite, not only in a
dedicated CI lane. A check that runs somewhere a contributor does not
look is a check they discover by being told they broke it.

---

## G-2 — What it must not do

**G-2.1** The guard MUST NOT require network access to run. A network
check that needs the network to decide whether the network is needed
fails in exactly the environment it is meant to protect.

**G-2.2** The guard MUST NOT assert on wall time. Timing assertions in a
parallel suite measure the host — see #849, filed the same day for
exactly that failure in two other tests.

**G-2.3** The guard MUST NOT be satisfied by a `replace` whose target is
absolute. Absolute paths pass locally and fail after `git archive`
extraction, which is the shape of a check that only works where it was
written.

---

## G-3 — Deliberately unresolvable fixtures

**G-3.1** A fixture whose unresolvability is the subject of a test MUST
still satisfy the guard, by replacing to a **missing local path** rather
than by exemption.

**G-3.2** If an exemption list is nonetheless needed, each entry MUST
carry a reason. An unexplained allowlist entry is indistinguishable
from an oversight after six months — the walker-audit allowlist is the
in-repo precedent worth copying, and worth copying carefully.

---

## G-4 — Proving it works

**G-4.1** The guard MUST be shown to fail when a network-reachable
fixture is introduced, and pass when it is removed. Asserted by doing
it, not by inspection.

*Test*: add a manifest with an unreplaced `require`, observe the guard
fail with the offending path named, remove it, observe the guard pass.
