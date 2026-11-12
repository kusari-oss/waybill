"""For each named library, list Haskell derivations whose env.propagatedBuildInputs carry it.
usage: python3 -I -B q6-fp-why.py <drv-closure.json> <name-version>..."""
import json, os, sys
drvs = json.load(open(sys.argv[1]))["derivations"]
for want in sys.argv[2:]:
    outs = {o["path"] for d, v in drvs.items() if v["name"] == want for o in v["outputs"].values()}
    for d, v in sorted(drvs.items()):
        e = v.get("env", {})
        if "setupCompilerEnvironmentPhase" not in e: continue
        for f in ("propagatedBuildInputs", "buildInputs"):
            if outs & {os.path.basename(p) for p in e.get(f, "").split()}:
                print(f"{want}: in {f} of {v['name']}")
