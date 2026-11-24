# Read every .conf of a nixpkgs GHC output from cache.nixos.org via `nix store cat` (range reads).
import sys, subprocess, json, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from conf_deps import parse
out_path, ghcdir = sys.argv[1], sys.argv[2]
d = f"{out_path}/lib/{ghcdir}"
store = ["--store", "https://cache.nixos.org"]
names = [l.strip().lstrip("./") for l in subprocess.run(["nix", "store", "ls", *store, d], capture_output=True, text=True, check=True).stdout.splitlines() if l.strip().endswith(".conf")]
out = {}
for n in names:
    t = subprocess.run(["nix", "store", "cat", *store, f"{d}/{n}"], capture_output=True, text=True, check=True).stdout
    name, ver, deps = parse(t)
    out[name] = {"version": ver, "depends": deps}
json.dump(out, sys.stdout, indent=1, sort_keys=True)
