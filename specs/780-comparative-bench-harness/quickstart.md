# Quickstart: `xtask compare`

**Feature**: 780-comparative-bench-harness

## What this is, and is not

It measures waybill against other SBOM tools you nominate, and produces
figures you can act on internally.

It does not produce a claim. It will not tell you waybill is faster or more
accurate, and its output is deliberately shaped so you cannot paste it
somewhere as though it had. That is the point: the comparison that motivated
this harness produced six different conclusions in an afternoon, each looking
authoritative in isolation.

## Setup

Results and configuration are private. Nothing here is committed.

```bash
cp xtask/compare/tools.example.toml xtask/compare/tools.local.toml
```

Edit it to name the tools you have installed:

```toml
[[tools]]
id      = "waybill"
argv    = ["target/release/waybill", "--offline", "sbom", "scan",
           "--path", "{target}", "--format", "cyclonedx-json",
           "--output", "{out}", "--no-deep-hash"]
network = "offline"
version_argv = ["target/release/waybill", "--version"]

[[tools]]
id      = "tool-a"
argv    = ["...", "{target}", "{out}"]
network = "offline"
version_argv = ["...", "--version"]
```

`tools.local.toml` is gitignored. The committed example names no real tool.

## Run

```bash
cargo run -p xtask --release -- compare
```

The self-check runs first. If the harness cannot recover a known answer from
its own fixture, it stops there and measures nothing.

## Reading the output

```text
self-check: 5 expected, 5 recovered  OK
measuring 3 tool(s) across 1 target(s), 3 interleaved repeats
  host: Darwin some-host 25.5.0 arm64 (Noisy)
  tool-a: <unavailable: No such file or directory (os error 2)>
  waybill: waybill 0.7.0

target ripgrep @ 0e8390a66fbc
  round 1/3 done  round 2/3 done  round 3/3 done
  waybill            0.05s  distinct=61  raw=68  identityless=7   accuracy not scored: no truth-derivation method is declared for this target (ecosystem: cargo)
  tool-a             0.00s  distinct=0   raw=0   identityless=0   accuracy not scored: the tool did not succeed (ToolAbsent); a tool that failed found nothing BECAUSE it failed

VERDICT: withheld (4 reason(s))
  - host is Noisy, not reference class — coverage and accuracy figures are still exact; only timing is affected
  - comparing Enriched against Offline sets one tool's cheapest mode against another's richest
  - timing spread for waybill exceeded 1.25 (observed 1.45)
  - tool-a did not produce usable output
Figures above are recorded for context and are NOT a comparison.

timing (within-session ratios, interleaved)
  (fewer than two comparable measurements)

wrote /path/to/repo/target/compare/run-20260909T192436Z.json
```

That is real output, captured from a run. Note what it does *not* say: no
tool is called faster or more accurate than another, and the verdict is
withheld rather than qualified.

Three things to read carefully:

**`raw` versus `distinct`.** Above, waybill reports 68 raw for 61 distinct.
A tool reporting 2,355 raw for 427 distinct — as one did during the
comparison that motivated this harness — emits the same package once per
manifest requiring it. Comparing raw counts across tools is meaningless, and
doing so is how that comparison concluded waybill found 22% of the packages
when it in fact found the most.

**`identityless`.** Components with no package identity. High counts here say
something about the tool that produced them, not about the target.

**"accuracy not scored".** Two different things produce this line and they
are not interchangeable: the target declared no truth method (as with the
cargo target above), or the tool did not succeed. Both state the reason,
because a blank or a zero in an accuracy column reads as "the tool found
nothing" — which is exactly wrong when the tool crashed.

**The superset note.** Where accuracy *is* scored against `go.sum`, the
output carries `[SUPERSET]`. `go.sum` holds hashes for modules merely
considered during resolution, so scoring against it rewards a tool for
reporting more, including things the build never links. It is the best
offline truth available for Go, not a correct one.

## When the verdict is withheld

That is the harness working. Common reasons:

- **Host not reference class** — you are on a laptop. Coverage and accuracy
  figures are still exact; only timing is affected.
- **Timing spread exceeded** — something else was running. Close it and
  re-run.
- **Coverage not reproducible** — two repeats of one tool disagreed on
  package count. This is a defect, not noise: either the tool is
  non-deterministic or the harness has a bug.

## Before quoting a number outside this repository

Check: is the verdict `comparable`? Was the host reference class? Is the
truth method a superset? Are the modes matched?

If any answer is no, the figure is for your own decisions, not for anyone
else's.
