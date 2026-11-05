# Measurements — 1279 (#1346)

Three probes, each with its input and system in its own header. **The system
label is load-bearing**: the x86_64-linux and aarch64-darwin closures of the
same project do not merely differ in magnitude on this milestone's two
populations — they populate *different attributes*. A figure quoted without
its system is the mistake #1348's research made, and the first draft of
FR-012 repeated it one level up by enumerating the darwin pair of stdenv
attributes and missing the one populated on linux.

| probe | answers |
|---|---|
| `attribute-census.py` | which dependency attributes a closure declares, and how many store paths each holds, against the **seven** the classifier consulted before this milestone |
| `role-delta-probe.py` | the role census before and after the widening, by reimplementing the classifier twice and differing only in the field set |
| `remainder-census.py` | what is left unread **after** the widening, and which components carry the C205 marker |

Input to all three is the output of the same invocation the reader uses, so a
finding cannot be an artifact of a differently-shaped query:

```sh
nix derivation show -r --option allow-import-from-derivation false \
    <repo>#packages.<system>.default > closure.json
```

(see `waybill-cli/src/scan_fs/package_db/nix/closure/mod.rs::closure_argv`)

## The control that makes these trustworthy

`remainder-census.py`'s **before** arm reproduces the committed golden's own
C204 record exactly — 6 fields, 230 declarations, 23 derivations affected,
field list identical — and `role-delta-probe.py`'s before arm reproduces the
golden's C184 role census exactly, 270 / 118 / 42 / 1,319. Neither probe
imports waybill; both reimplement the rule from the source as data. Agreeing
with the shipped artifact on the pre-change state is what makes the
post-change figures evidence rather than arithmetic.

## Attribute census — x86_64-linux, nixpkgs `a799d3e3`, format version 4

`nix-closure-moat` at its pin, 1,749 derivations. 15 dependency attributes are
declared; 10 hold at least one store path.

| attribute | declared on | store paths | consulted before | consulted after |
|---|---|---|---|---|
| `nativeBuildInputs` | 932 | 2,181 | yes | yes |
| `propagatedBuildInputs` | 932 | 560 | yes | yes |
| `buildInputs` | 932 | 464 | yes | yes |
| **`defaultNativeBuildInputs`** | 13 | **208** | no | **no — FR-012** |
| `depsBuildBuild` | 932 | 191 | yes | yes |
| **`depsBuildTarget`** | 932 | **5** | no | **yes — FR-011** |
| **`depsTargetTarget`** | 932 | **5** | no | **yes — FR-010** |
| **`depsTargetTargetPropagated`** | 932 | **5** | no | **yes — FR-010** |
| **`propagatedNativeBuildInputs`** | 932 | **4** | no | **yes — FR-003** |
| **`extraNativeBuildInputs`** | 3 | **3** | no | **no — FR-012** |
| `defaultBuildInputs` | 13 | 0 | no | no — FR-012 |
| `depsBuildBuildPropagated` | 932 | 0 | no | no |
| `depsBuildTargetPropagated` | 932 | 0 | no | no |
| `depsHostHost` | 932 | 0 | **yes** | yes |
| `depsHostHostPropagated` | 932 | 0 | no | no |

Two states in that table are easy to miss and both are tested (contract P3,
P4): `depsHostHost` is **consulted and declared but holds nothing**, and five
attributes are declared on nearly every derivation while holding nothing at
all. A declared-but-empty attribute is not a gap, and a consulted attribute
that is declared nowhere is not a gap either — a disclosure that reported
either would cry wolf in the mechanism whose whole value is being trustworthy
when it stays silent.

**`defaultNativeBuildInputs` holds 208 of the 230 unread declarations**, which
is the whole reason FR-012 excludes it rather than classifying it. Reading it
would have moved the census by an amount that swamps the four attributes this
milestone is accountable for, and SC-004 asks that every role change be
traceable to one of the four.

## Role census — x86_64-linux

Derivation level, from `role-delta-probe.py`:

| role | before | after | delta |
|---|---|---|---|
| `artifact-input` | 270 | 268 | **−2** |
| `build-tooling` | 118 | 121 | +3 |
| `both` | 42 | 45 | +3 |
| `unreferenced` | 1,319 | 1,315 | −4 |

Seven derivations change role, and **`artifact-input` goes down**, which looks
wrong until the seven are named: three of its members gain a tooling reference
and become `both`, while only one arrives. A plan that assumed every delta was
positive would have mis-specified SC-011.

All seven, with the attribute that reaches each (SC-004's accounting clause):

| identity | before → after | reached through |
|---|---|---|
| `binutils-patchelfed-ld-wrapper 2.46` | `artifact-input` → `both` | `depsBuildTarget` |
| `bootstrap-stage0-binutils-wrapper 2.46.0` | `artifact-input` → `both` | `depsBuildTarget` |
| `find-xml-catalogs-hook 26.05pre-git` | `artifact-input` → `both` | `propagatedNativeBuildInputs` |
| `numactl 2.0.18` | `unreferenced` → `artifact-input` | `depsTargetTarget` **and** `depsTargetTargetPropagated` |
| `patchelf 0.15.2` (two derivations) | `unreferenced` → `build-tooling` | `depsBuildTarget` |
| `setup-debug-info-dirs-hook 26.05pre-git` | `unreferenced` → `build-tooling` | `propagatedNativeBuildInputs` |

That last column is **measured per member, not inferred from the role**. The
first draft of this table inferred it, and got `patchelf` wrong — a tooling
role is reachable through either new tooling-side attribute, and attributing
it to the wrong one would have left SC-004's accounting clause satisfied in
form and false in substance. A matching total can carry a wrong cause.

Six further members are reached by a newly-consulted attribute **without**
changing role, because an attribute the classifier already read reaches them
too: `gcc-wrapper` through `depsBuildTarget`, `nss-cacert` through
`propagatedNativeBuildInputs`, and `elfutils`, `gmp-with-cxx`, `libffi` and
`ncurses` through both target triples. Those four plus `numactl` are the five
triple-reachable members SC-010 names — and the reason it names all five
rather than only the one that moves: the other four already carry no scope
and would acquire one together if the triples were ever reclassified as
tooling, so a criterion checking only `numactl` would pass for four fifths of
the population it is about.

No member changes role on the strength of a stdenv-injected attribute, which
is FR-012 and SC-011 holding in the measurement rather than only in a test.
The probe also reports the size of what FR-012 declines: had the stdenv pair
been consulted too, a further 3 derivations would have moved.

## What is left unread — x86_64-linux

| | before | after |
|---|---|---|
| attributes unconsulted and populated | 6 | **2** |
| store-path declarations unread | 230 | **211** |
| derivations affected | 23 | **16** |

The declaration count barely moves — 230 → 211 — and that is the honest shape
of this milestone. The four attributes it consults hold 19 declarations
between them; the two it leaves hold 211. What changed is not how much is
unread but whether a consumer can **tell**: the 211 are now named per
component by C205, and the four that are read no longer leave members looking
like nothing depends on them.

## The C205 population — x86_64-linux

| | count |
|---|---|
| declarations in unconsulted attributes | 211 |
| closure members those declarations name | 16 |
| of those, members emitted as components at all | 13 |
| after build-variant merging on `(pname, version)` | **9** |

**The declaration count is not the marker count**, and the contract was
drafted as though it were — it said 3. Each step of that chain loses something
for a different reason: 211 → 16 because `defaultNativeBuildInputs` names the
same few members from 13 derivations; 16 → 13 because an `unreferenced` member
applying no patch is not emitted as a component at all (C182 / m1035 FR-004a),
so the marker has nowhere to land; 13 → 9 because a closure holds several
derivations per identity.

The nine: six `gcc-wrapper` variants, `patchelf` and
`update-autotools-gnu-config-scripts-hook` through `defaultNativeBuildInputs`;
`glibc` through `extraNativeBuildInputs`. `patchelf` is also one of the two
components this milestone newly emits, so it carries a role **and** the
marker — the gating row the fixture tests as T044, present in the real
closure as well.

### The one member #1346's ask still cannot reach

Of the three members that are named but not emitted, two — a `patchelf` and an
`update-autotools-gnu-config-scripts-hook` variant — have sibling variants
that do emit, so their identity carries the marker anyway. The third does not:

> **`xz 5.8.3`** is `unreferenced`, is named only by
> `defaultNativeBuildInputs`, and applies no patch. It is therefore not
> emitted as a component at all, and there is nothing for the marker to land
> on.

This is the reach limitation stated plainly rather than left for a reader to
deduce from the counts. It is **not** a mislabelled component — the component
is simply absent from the document, which makes no claim about it either way
— but it is the residue of #1346's ask that this milestone does not close.
Closing it would mean changing C182's emission rule so that a marked
`unreferenced` member emits, which is milestone 1035's rule and a separate
decision: it would add components to every closure scan on the strength of a
reference waybill has declined to classify.

## Reproducing

```sh
cd specs/1279-closure-dep-field-coverage/measurements
python3 -I attribute-census.py  /path/to/closure.json
python3 -I role-delta-probe.py  /path/to/closure.json
python3 -I remainder-census.py  /path/to/closure.json
```

`-I` is deliberate: the input is a 5.7 MB JSON document fetched by evaluating
a third-party flake, and an interpreter that loads modules from the current
directory alongside it is a hazard the flag removes.
