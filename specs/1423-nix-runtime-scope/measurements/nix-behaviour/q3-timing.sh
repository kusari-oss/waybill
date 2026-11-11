#!/usr/bin/env bash
# Q3: wall time of `nix path-info -r` from the local store and from cache.nixos.org.
# 5 runs per cell. Three modes:
#   local      : default (daemon) store
#   cache-warm : --store https://cache.nixos.org, nix's narinfo disk cache allowed (default TTLs)
#   cache-cold : same, with --option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0
#                so every narinfo is fetched over the network
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q3-out; mkdir -p "$o"
N=${N:-5}
for a in shellcheck curl haskellPackages.hlint pandoc; do
  out=$(nix eval --raw "$R#$a.outPath"); echo "$out" > "$o/$a.outpath"
  for i in $(seq 1 "$N"); do
    python3 -I -B timed.py "$o/$a.local.$i" nix path-info -r "$out"
    python3 -I -B timed.py "$o/$a.cache-warm.$i" nix path-info -r --store https://cache.nixos.org "$out"
    python3 -I -B timed.py "$o/$a.cache-cold.$i" nix path-info -r --store https://cache.nixos.org \
      --option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0 "$out"
  done
done
python3 -I -B - "$o" "$N" > q3-summary.txt <<'PY'
import sys, statistics, glob
o, n = sys.argv[1], int(sys.argv[2])
print(f"{'attr':24} {'mode':11} {'min':>7} {'median':>7} {'max':>7}  exits  paths")
for a in ["shellcheck", "curl", "haskellPackages.hlint", "pandoc"]:
    for m in ["local", "cache-warm", "cache-cold"]:
        secs = [float(open(f"{o}/{a}.{m}.{i}.secs").read()) for i in range(1, n+1)]
        exits = sorted({open(f"{o}/{a}.{m}.{i}.exit").read().strip() for i in range(1, n+1)})
        paths = sorted({sum(1 for l in open(f"{o}/{a}.{m}.{i}.out") if l.strip()) for i in range(1, n+1)})
        print(f"{a:24} {m:11} {min(secs):7.3f} {statistics.median(secs):7.3f} {max(secs):7.3f}  {','.join(exits):5}  {paths}")
PY
