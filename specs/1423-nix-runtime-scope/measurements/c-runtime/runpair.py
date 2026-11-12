"""usage: runpair.py BINARY LABEL SYSTEM FLAKES_DIR OUT_DIR TARGET...

Per wrapper-flake target, two `waybill sbom scan --nix-closure` runs of
`packages.<SYSTEM>.default`:

  rt    the configured substituters: the runtime closure, answered and
        signature-verified by cache.nixos.org (C216 method runtime-closure)
  walk  NIX_CONFIG="substituters =": no store answers, so the reachability
        walk decides (C216 method reachability-walk)

The rt run is the truth the walk run is scored against (scopediff.py).
"""
import os, subprocess, sys

binary, label, system, flakes, outdir, targets = (
    sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6:])
for t in targets:
    for mode in ("rt", "walk"):
        out = f"{outdir}/{label}/{system}/{t}.{mode}"
        os.makedirs(out, exist_ok=True)
        env = dict(os.environ)
        env.pop("NIX_CONFIG", None)
        if mode == "walk":
            env["NIX_CONFIG"] = "substituters ="
        argv = [binary, "sbom", "scan", "--nix-closure", "--nix-closure-attr",
                f"packages.{system}.default", "--no-deps-dev", "--no-clearly-defined",
                "--path", f"{flakes}/{t}", "--format", "cyclonedx-json",
                "--output", f"cyclonedx-json={out}/cdx.json"]
        with open(f"{out}/stderr.log", "w") as log:
            r = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT, env=env)
        print(t, mode, "exit", r.returncode, flush=True)
