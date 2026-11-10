# Quickstart: verifying closure binary contents

Needs nix and network. Uses the wrapper flakes in `measurements/targets/` and
the counting probe in `measurements/probe/count.jq`.

```sh
cargo build --release -p waybill
W=$PWD/target/release/waybill
M=$PWD/specs/1378-closure-binary-contents/measurements
```

The flake directories must be git repositories for nix to read them:
`git -C $M/targets/gh init -q && git -C $M/targets/gh add -A` (in a scratch
copy, not in the repository).

## 1. Positive: output present (SC-001, SC-002, SC-005)

```sh
cd <scratch>/gh && nix build .#default --no-link
$W sbom scan --nix-closure --no-deep-hash --path . \
   --format cyclonedx-json --output cyclonedx-json=/tmp/gh.cdx.json
jq -f $M/probe/count.jq /tmp/gh.cdx.json
```

Expect `golang` = 164 with deps.dev on (163 recovered + gh itself promoted),
or 163 with `--no-deps-dev`; exactly one component named `gh`; C213 `root:
"read"`. For ripgrep: `cargo` = 42 with deps.dev on (41 recovered + ripgrep
itself promoted), or 41 with `--no-deps-dev` (ripgrep stays `pkg:generic`).

## 2. Scanner control (SC-003)

```sh
osv-scanner scan source --format json -L /tmp/gh.cdx.json
```

Go findings equal those on the standalone scan of gh's output.

## 3. Negative: root not present (SC-004)

**Do not use `NIX_STORE_DIR` for this with a real nix.** nix honours the
variable too, so pointing it somewhere empty makes the closure tier degrade
(measured: `no-evaluable-attribute`, 2 components, no C213) — the run then
says nothing about this feature. Use a package whose output
is genuinely not in the store instead:

```sh
nix eval --raw github:NixOS/nixpkgs/<rev>#gum.outPath   # then: test -e <path>
```

Copy `targets/gh`, point it at that package, and scan. Expect exit 0, C213
`root: "absent"`, C214 `absent` on the root, and a `WARN` naming it. Any
modules present come from other members that happen to be in the store.

## 4. Strict (SC-004a)

Same target with `--nix-closure-require-root-contents`. Expect a non-zero
exit, an error naming the root, and no output file.

## 5. Parity (SC-006)

Emit all three formats and compare recovered PURLs and member→module edges.

## 6. Unchanged closure (SC-007)

Scan a flake with no Go or Rust member before and after; the only differences
are C213, C214 and the root component.
