# Data Model: A scan of this repository must not depend on network reachability

Feature: `843-fixture-network-isolation` · Spec: [spec.md](./spec.md)

No runtime state. These are properties of files in the test tree and of
what the toolchain does when it reads them.

---

## Fixture module

One module declared under `waybill-cli/tests/fixtures/`.

| Field | Description |
|---|---|
| manifest | Path to its `go.mod` |
| intent | `incidental` \| `deliberate` |
| requires | Module paths it depends on |
| replacements | Local paths those requires resolve to |

**Validation**
- `intent` is recorded per module, not inferred at read time (FR-005).
  Inferring it means re-deriving it, and the first attempt to derive it
  in research was wrong — a name-based grep returned 25 files where a
  path-based one returns 3.
- Every entry in `requires` has a matching entry in `replacements`. A
  require without one reaches the network, which is the defect.
- `incidental` means the module's identity is scaffolding: tests using
  it care about something else. `deliberate` means its unresolvability
  is the subject of an assertion.
- The deliberate set is **empty today**. That is a fact about today, not
  a reason to drop the distinction.

---

## Replacement

The local path a require resolves to.

| Field | Description |
|---|---|
| target | Path relative to the declaring manifest |
| exists | Whether a module lives there |

**Validation**
- Relative, never absolute. Absolute paths do not survive `git archive`
  extraction to another location, which is how the benchmark and corpus
  harnesses obtain the tree (FR-006).
- `exists = true` → the module resolves; `go mod graph` prints a graph.
  `exists = false` → resolution fails locally with `no such file or
  directory`. **Both cost 0.01s and neither touches the network**, which
  is why the incidental/deliberate split does not force a
  speed-versus-fidelity trade.
- A missing target is the right shape when a golden covers the fixture:
  the failure and its annotations are preserved, so nothing churns.
- A `replacements` entry whose target was deleted or moved must surface
  as a test failure, not as a silent fallback to the network (FR-007) —
  otherwise the original defect returns wearing a repair.

---

## Resolution attempt

One thing the toolchain does when asked about a module.

| Field | Description |
|---|---|
| reaches network | Whether it leaves the machine |
| cost | Wall time |

**Validation**
- Local: ~0.01s, whether it succeeds or fails.
- Proxy: ~1.4s per module and variable with network conditions. The
  variance is the harm, more than the mean — a stable 1.4s would be
  annoying; an unpredictable 0.05–20s makes any measurement taken over
  it unreliable, which is how this feature's own clarification produced
  two wrong attributions.
- Module path does not determine whether the network is reached. The
  toolchain consults the proxy first, so a module named for an
  unroutable host still costs a proxy round-trip (research R3).

---

## Scan floor

Wall time of a scan with all enrichment disabled.

| Field | Description |
|---|---|
| value | Median of at least three runs |
| network | Whether the machine had one |

**Validation**
- **Never a single sample.** The floor's variance is the subject of the
  feature; one reading of it is not a measurement. Two wrong
  attributions were made during clarification by ignoring this, both by
  someone who had already written the rule down.
- Should be a property of the repository. Today it is a property of the
  network: 0.50s offline against 5.26s networked.
