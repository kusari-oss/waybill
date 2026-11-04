# Measurements for 1278-closure-structured-attrs

Committed so the findings are reproducible, and so the next person can re-run
them when nix or nixpkgs moves. An undocumented property of an external system
is one that can change without notice — and this milestone exists because a
property of `nix derivation show`'s output changed without anyone noticing.

## Getting the input

Both probes take the output of the **same invocation the reader uses**
(`closure/mod.rs::closure_argv`), so a finding cannot be an artifact of a
differently-shaped query:

```sh
REPO=~/.cache/waybill/corpus/53983fe1021353b3/d06905558ba68b49b17f36c875e97f7234a9c19b/repo
nix derivation show -r --option allow-import-from-derivation false \
    "$REPO#packages.x86_64-linux.default" > closure-linux.json
nix derivation show -r --option allow-import-from-derivation false \
    "$REPO#packages.aarch64-darwin.default" > closure-darwin.json
```

That path is the `nix-closure-moat` corpus target at its pinned commit
`d0690555`, nixpkgs `a799d3e3` via its own `flake.lock`. The x86_64-linux
attribute is the one the committed goldens pin, because the corpus lane runs on
Linux. Evaluating the Linux attribute from macOS is fine — `derivation show` is
evaluation, not building.

Run 2026-11-04 with Determinate Nix 3.20.0, which emits **derivation-show
format version 4**.

## `role-census-probe.py` — the headline

Reimplements `closure/classify.rs::classify` twice: once reading only the flat
`env` map, as the shipped reader does, and once also reading the same seven
fields out of `structuredAttrs`.

| | env only | + structuredAttrs | delta |
|---|---|---|---|
| artifact-input | 256 | 270 | +14 |
| build-tooling | 101 | 118 | +17 |
| both | 39 | 42 | +3 |
| unreferenced | **1,353** | **1,319** | **−34** |

37 derivations change role on the Linux closure (84 on darwin), 33 of which
declare a name and so are emitted, covering 29 distinct identities.

### The control that matters

A probe that reimplements the thing it measures can be wrong in the same
direction as the thing. So: does the **env-only** arm reproduce the shipped
classifier? Against the committed `cdx.json` golden, **347 of 348** shared
components' roles agree. One disagrees (`docbook-xml`), and 9 golden names are
absent from the local closure — attributable to nix and nixpkgs drift since the
goldens were cut.

This control is doing double duty. It validates the probe, **and** it is the
evidence that the blind spot is live in CI: if CI's nix emitted the flat form
for these derivations, CI's roles would match the `+structuredAttrs` arm, not
the `env`-only arm. They match the `env`-only arm.

### What it also reconciles

The committed census in `specs/1035-nix-closure-sbom/research.md` and in the
corpus manifest's comment — 1,275 derivations, 264 / 134 / 52 / 825 — is
reproduced **exactly** by this probe's env-only arm on the **aarch64-darwin**
closure, not the x86_64-linux one the manifest comment names. The Linux closure
has 1,749 derivations. The census is a darwin measurement wearing a linux
label.

## `source-identity-probe.py` — how this was found

This probe was written to answer a different question: can the ~107
`source`-named derivations be given a real identity instead of being emitted
as 107 components all called `source`? The answer is yes — 104 of 106 carry
`owner`, `repo` and `rev`, keying to `pkg:github/<owner>/<repo>@<ref>`, and the
content-hash fallback is needed for none of them. Collisions are safe: the 16
colliding identities on darwin each cover derivations with one output content
hash, so they are the same bytes fetched twice.

It is committed here, in the milestone that came out of it, because of what it
found on the way: the `env` key census for all 106 `source` derivations came
back as literally `{out: 1}`. Everything else those derivations declare —
`owner`, `repo`, `rev`, `urls`, `patches`, `buildInputs` — was in
`structuredAttrs`. The identity question was answerable; the reader just could
not see any of the answers.

The forge-coordinate decision belongs to the separate emission work. The probe
stays here with the defect it surfaced.

## Blast radius, by accessor

Every read in `closure/` funnels through four accessors on the derivation
record, so the measurement is per accessor:

| accessor | what is invisible on the Linux closure | on darwin |
|---|---|---|
| `paths_in` (7 dependency fields) | 554 entries across 421 derivations | 879 across 596 |
| `paths_in("patches")` | 28 patch entries across 7 derivations | 96 across 25 |
| `pname` / `version` | 11 derivations | 66 |

## The nine false exclusions

Cross-referencing the corrected roles against the committed golden's
`waybill:scope-classification-source`: 9 components carry CDX
`scope: "excluded"` sourced from `"closure-role"` alone — no independent
lifecycle support — with a corrected role of `artifact-input` or `both`.

```
autoconf             build-tooling  -> both
automake             build-tooling  -> both
bash-interactive     unreferenced   -> artifact-input
docbook-xml          build-tooling  -> both
elfutils             unreferenced   -> artifact-input
keyutils             unreferenced   -> artifact-input
libedit              unreferenced   -> artifact-input
tzdata               build-tooling  -> both
util-linux-minimal   build-tooling  -> both
```

A first pass at this count said **16**, by selecting golden components whose
*corrected* role was `artifact-input` and never checking their role *before*.
That swept in `hspec`, `HUnit`, `hspec-discover` and friends, whose role does
not change at all — their `excluded` comes from dev-scope lifecycle
propagation, which is independent of this defect and remains correct. Tracing
one of them showed `hspec` referenced via `buildInputs` **from `env`**, a field
the reader already sees, which is what exposed the error. The filter on
`scope-classification-source = "closure-role"` is what makes the count
defensible: it isolates exclusions resting on the role and nothing else.
