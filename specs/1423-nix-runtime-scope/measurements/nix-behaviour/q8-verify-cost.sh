#!/usr/bin/env bash
# Q8 (T002 / research R11): the cost of signature verification.
#
#   nix store verify --no-contents -r --store https://cache.nixos.org <path>
#
# timed for shellcheck, curl and haskellPackages.hlint, cold and warm, N runs
# each, alongside the `nix path-info -r` query it follows.
#
# The query and verify are passed the attribute's OUTPATH, not the installable:
# q9 found the installable form unusable (`--store <cache>` makes evaluation try
# to upload the .drv to the cache, and curl's installable fails outright), so
# the implementation queries store paths. The bounds waybill passes on every
# remote call (`connect-timeout 2`, `download-attempts 1`) are included.
#
#   warm : nix's narinfo disk cache allowed (default TTLs)
#   cold : --option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0
#
# Each case writes .out/.err/.exit/.secs under q8-out/; q8-summary.txt is the table.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q8-out; mkdir -p "$o"
N=${N:-5}
S=https://cache.nixos.org
B=(--option connect-timeout 2 --option download-attempts 1)
COLD=(--option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0)
ATTRS=(shellcheck curl haskellPackages.hlint)
for a in "${ATTRS[@]}"; do
  out=$(nix eval --raw "$R#$a.outPath"); echo "$out" > "$o/$a.outpath"
  for i in $(seq 1 "$N"); do
    python3 -I -B timed.py "$o/$a.query-cold.$i"  nix path-info -r --store "$S" "${B[@]}" "${COLD[@]}" "$out"
    python3 -I -B timed.py "$o/$a.verify-cold.$i" nix store verify --no-contents -r --store "$S" "${B[@]}" "${COLD[@]}" "$out"
    python3 -I -B timed.py "$o/$a.query-warm.$i"  nix path-info -r --store "$S" "${B[@]}" "$out"
    python3 -I -B timed.py "$o/$a.verify-warm.$i" nix store verify --no-contents -r --store "$S" "${B[@]}" "$out"
  done
done
python3 -I -B - "$o" "$N" "${ATTRS[@]}" > q8-summary.txt <<'PY'
import sys, statistics
o, n, attrs = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
print(f"{'attr':24} {'case':12} {'min':>7} {'median':>7} {'max':>7}  exits  paths  median-s/path")
for a in attrs:
    paths = sum(1 for l in open(f"{o}/{a}.query-warm.1.out") if l.strip())
    for m in ["query-cold", "verify-cold", "query-warm", "verify-warm"]:
        secs = [float(open(f"{o}/{a}.{m}.{i}.secs").read()) for i in range(1, n + 1)]
        exits = sorted({open(f"{o}/{a}.{m}.{i}.exit").read().strip() for i in range(1, n + 1)})
        med = statistics.median(secs)
        print(f"{a:24} {m:12} {min(secs):7.3f} {med:7.3f} {max(secs):7.3f}  {','.join(exits):5}  {paths:5}  {med / paths:.4f}")
PY
cat q8-summary.txt
