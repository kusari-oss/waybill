# Contract: which dependency attributes are consulted

## The set

Artifact-side — a reference here credits `artifact-input`:

```
buildInputs  propagatedBuildInputs  depsHostHost
depsTargetTarget  depsTargetTargetPropagated          <- added by this milestone
```

Tooling-side — a reference here credits `build-tooling`:

```
nativeBuildInputs  depsBuildBuild  depsBuildHost  nativeCheckInputs
depsBuildTarget  propagatedNativeBuildInputs          <- added by this milestone
```

Named by nixpkgs convention and consulted explicitly; `patches` is read by the
patch collector and is not a dependency attribute.

## Why the target triples are artifact-side

`depsTargetTarget` and `depsTargetTargetPropagated` name dependencies of the
**target** platform: what a compiler being built hands to things it later
compiles. They are neither in the artifact nor tooling that built it.

They are classified `artifact-input` because of what the alternative
**asserts**. Milestone 1277 emits CycloneDX `scope: "excluded"` for tooling
roles — a positive claim that the member is absent from the artifact — and a
cross-compiler ships its target libc inside its own output, so that claim can
be false. That is the defect class of #1342 and #1348. `artifact-input`
asserts no scope at all, which is the honest answer when waybill cannot tell.

The cost is accepted and recorded: `artifact-input` now covers two
distinguishable things. A fifth role value is the follow-up if a genuinely
cross closure makes that expensive, and it carries its own milestone — it
would change the C182 closed enum, every consumer of it, invariant I10's
population and C203's vocabulary.

`depsBuildTarget` is tooling by the same reading as `depsBuildBuild`: it runs
on the build machine.

## Properties

| # | Property | Why it is stated |
|---|---|---|
| P1 | A member referenced through any consulted attribute is credited to that side. | The feature. |
| P2 | The union rule for a member referenced both ways is unchanged. | Three members move `artifact-input`→`both` on the pinned closure; the rule is what makes that right rather than a coin toss. |
| P3 | An empty attribute credits nothing. | Four attributes are declared on every derivation of the darwin closure and hold zero paths. |
| P4 | An attribute consulted but never declared is not a gap. | `depsBuildHost` and `nativeCheckInputs` are declared on zero derivations of either closure. |
| P5 | The disclosure's `fields-unconsulted` is exactly the complement of this set, over what the closure declares. | Asserted by a test, not by inspection — a second hand-maintained copy is the drift that would make the disclosure lie. |
| P6 | `defaultNativeBuildInputs` and `defaultBuildInputs` are **not** consulted, and contribute zero census movement. | FR-012. It is what keeps the census attributable. |

## The test that must exist

The obvious test consults a new attribute and asserts a member's role. That
test passes while the feature is half-broken, because P5 is a separate claim:
the classifier could read an attribute the disclosure still reports as
unconsulted, and a consumer would then be told a gap exists where it does not.

So two assertions are required, not one:

1. a member referenced only through each newly-consulted attribute carries the
   expected role; and
2. none of those attribute names appears in `fields-unconsulted` for the same
   document, while the deliberately-excluded pair still does.

Each carries the control the classifier tests already carry: that the fixture
holds the derivations it claims and that something is referenced — a parse
that produced nothing satisfies every "not wrongly classified" assertion
vacuously.
