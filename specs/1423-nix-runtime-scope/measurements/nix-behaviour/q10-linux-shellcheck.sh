#!/usr/bin/env bash
# Q10 (2026-11-11): does the x86_64-linux runtime closure of the
# nix-closure-shellcheck corpus target hold the static `ShellCheck-` Haskell
# build its binary was copied from?
#
# The static-link rule was measured only on aarch64-darwin, where the runtime
# closure holds it. The Linux corpus regen came back with
# `static-link-source: none` and every Haskell library linked into shellcheck
# marked excluded.
#
# Both systems are asked, so the difference is shown rather than asserted.
# For each: the default output (`outPath`) of the fixture flake's
# `packages.<system>.default`, its runtime closure from cache.nixos.org
# (`nix path-info -r --store`, which builds nothing), and the root
# derivation's Haskell-builder `inputs.drvs`, with whether the root names each
# one in a build-machine field. Evaluating another system's derivation
# instantiates its .drv files locally and builds nothing, so this runs from
# darwin.
#
# Writes q10-out/<system>.{outpath,drvpath,closure,err} and
# q10-out/summary.txt.
set -u
cd "$(dirname "$0")"
F=github:kusari-sandbox/waybill-fixture-nix-shellcheck/dc4e4509686df76a2e060a019097e2b7a8167073
o=q10-out; mkdir -p "$o"
B=(--option connect-timeout 5 --option download-attempts 2)

for sys in x86_64-linux aarch64-darwin; do
  nix eval --raw "$F#packages.$sys.default.outPath" > "$o/$sys.outpath" 2> "$o/$sys.err"
  nix eval --raw "$F#packages.$sys.default.drvPath" > "$o/$sys.drvpath" 2>> "$o/$sys.err"
  nix path-info -r --store https://cache.nixos.org "${B[@]}" "$(cat "$o/$sys.outpath")" \
    > "$o/$sys.closure" 2>> "$o/$sys.err"
  echo "$?" > "$o/$sys.closure.exit"
  nix derivation show "$(cat "$o/$sys.drvpath")" > "$o/$sys.root-drv.json" 2>> "$o/$sys.err"
  python3 -I -B - "$o/$sys.root-drv.json" > "$o/$sys.inputs.txt" <<'PY'
import json, subprocess, sys
d = json.load(open(sys.argv[1]))
d = d.get("derivations", d)
(key, root), = d.items()
env = root.get("env", {})
TOOLING = ["nativeBuildInputs", "depsBuildBuild", "depsBuildHost", "nativeCheckInputs",
           "depsBuildTarget", "propagatedNativeBuildInputs"]
ARTIFACT = ["buildInputs", "propagatedBuildInputs", "depsHostHost", "depsTargetTarget",
            "depsTargetTargetPropagated"]
named = lambda fields: " ".join(env.get(f, "") for f in fields)
inputs = root.get("inputs", {}).get("drvs", root.get("inputDrvs", {}))
for drv in sorted(inputs):
    path = drv if drv.startswith("/") else "/nix/store/" + drv
    shown = json.loads(subprocess.run(["nix", "derivation", "show", path],
                                      capture_output=True, text=True, check=True).stdout)
    shown = shown.get("derivations", shown)
    (_, inp), = shown.items()
    if "setupCompilerEnvironmentPhase" not in inp.get("env", {}):
        continue
    outs = [v.get("path", "") for v in inp.get("outputs", {}).values()]
    outs = [p if p.startswith("/") else "/nix/store/" + p for p in outs]
    in_tool = any(p and p in named(TOOLING) for p in outs)
    in_art = any(p and p in named(ARTIFACT) for p in outs)
    print(f"haskell-builder input {drv.rsplit('/', 1)[-1]}  "
          f"named-in-tooling-field={in_tool} named-in-artifact-field={in_art}")
PY
done

{
  date -u +%Y-%m-%dT%H:%M:%SZ
  nix --version
  echo "flake $F"
  for sys in x86_64-linux aarch64-darwin; do
    echo
    echo "== $sys"
    echo "outPath $(cat "$o/$sys.outpath")"
    echo "path-info exit $(cat "$o/$sys.closure.exit")"
    echo "runtime paths $(grep -c . "$o/$sys.closure")"
    n=$(grep -c -- '-ShellCheck-' "$o/$sys.closure")
    echo "ShellCheck- Haskell builds in runtime closure: $n"
    grep -- '-ShellCheck-' "$o/$sys.closure"
    echo "root's Haskell-builder inputs.drvs:"
    cat "$o/$sys.inputs.txt"
  done
} > "$o/summary.txt"
cat "$o/summary.txt"
