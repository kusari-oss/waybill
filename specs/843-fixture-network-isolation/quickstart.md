# Adding a Go fixture without reintroducing #843

Feature: `843-fixture-network-isolation`

A Go fixture that declares a dependency it cannot resolve locally will
be looked up over the network — every scan, forever, whether or not the
lookup can succeed. That is how this problem arrived: not by decision,
but because each new fixture copied the shape of the last one and
nothing objected.

## The rule

**Every `require` needs a `replace` pointing somewhere inside the
repository.**

```go
module example.com/waybill-fixture-app

go 1.21

require example.com/waybill-fixture-lib v1.0.0

// Without this line the toolchain asks proxy.golang.org about a module
// that does not exist, and waits.
replace example.com/waybill-fixture-lib => ../lib
```

## Two shapes, both free

| you want | point `replace` at | result |
|---|---|---|
| the dependency to resolve | a real module in the tree | graph resolves, 0.01s |
| the dependency to stay unresolvable | a path that does not exist | fails locally, 0.01s |

The second is not a hack. If your fixture exists to exercise what
waybill does with an unresolvable module, you want the failure — you
just do not want to pay a network round-trip for it. A missing local
target gives the failure immediately, and keeps whatever annotations a
golden already records.

Prefer the missing-target shape when a golden covers your fixture:
making a previously-unresolvable module resolve changes
`waybill:go-transitive-coverage` and churns the golden.

## What not to do

**Do not rename the module to an unroutable host.** `.invalid`,
`.example`, a domain you own — none of it helps. The toolchain asks the
module proxy before it ever contacts the module's own host, so the name
is not what costs. Measured: 1.40s against 1.31s, about 6%.

**Do not rely on `GOPROXY=off` in the environment.** It works, and it
is how CI can be belt-and-braces, but it only protects the person who
remembered to set it. A contributor scanning by hand gets nothing.

**Do not use an absolute path in `replace`.** It passes on your machine
and fails after `git archive` extraction, which is how the benchmark
and corpus harnesses obtain the tree.

## Checking your work

```bash
# Should print a graph, or a local "no such file or directory" —
# and should take about ten milliseconds either way.
cd waybill-cli/tests/fixtures/<your fixture>
time go mod graph
```

If it takes a second or more, it is reaching the network and the guard
will reject it.

## Why the rule is worth the nuisance

The scan floor for this repository was **5.26s networked against 0.50s
offline**, and it varied enough between runs that measurements taken
over it were unreliable. Three wrong performance figures in milestone
839 and two more during this feature's own planning came from
attributing changes to code when the difference was the network.

An unstable measurement is worse than a slow one. A slow scan wastes
time; an unstable one produces confident, wrong conclusions.
