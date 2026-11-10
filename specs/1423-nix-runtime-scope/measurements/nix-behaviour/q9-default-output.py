"""Compare the head of a derivation's `outputs` attribute with the attribute's outPath."""
import json, sys

j = json.load(open(sys.argv[1]))
outpath = open(sys.argv[2]).read().strip()
drvs = j.get("derivations", j)
((key, d),) = drvs.items()
sa = d.get("structuredAttrs") or {}
outs = d.get("env", {}).get("outputs")
outs = outs.split() if isinstance(outs, str) else (sa.get("outputs") or ["out"])
head = outs[0]
p = d["outputs"][head]["path"]
base = p.rsplit("/", 1)[-1]
print(
    f"{key.rsplit('/', 1)[-1]} outputs-attr={outs} head={head} head-path={base} "
    f"outPath={outpath.rsplit('/', 1)[-1]} agree={outpath.endswith(base)}"
)
