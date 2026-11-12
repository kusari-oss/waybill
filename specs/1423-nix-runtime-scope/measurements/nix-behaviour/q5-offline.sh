#!/usr/bin/env bash
# Q5: behaviour with the network unavailable to nix's client.
# Network loss is simulated with proxy variables (this host's network was not switched off):
#   refused   : HTTP(S)_PROXY=http://127.0.0.1:9   (nothing listens; connection refused at once)
#   blackhole : HTTP(S)_PROXY=http://192.0.2.1:9   (RFC 5737 TEST-NET-1; SYNs go unanswered)
# Both with NO_PROXY=localhost,127.0.0.1,::1 and the lowercase variants.
# NOTE: the proxy only reaches nix's *client* process. Queries the client forwards to the
# nix daemon (the default store) are made by the daemon, whose environment is unchanged.
set -u
cd "$(dirname "$0")"
R=github:NixOS/nixpkgs/a799d3e3886da994fa307f817a6bc705ae538eeb
o=q5-out; mkdir -p "$o"
N=${N:-3}
SC=$(nix eval --raw "$R#shellcheck.outPath")
CURL=$(nix eval --raw "$R#curl.outPath")
HL=$(nix eval --raw "$R#haskellPackages.hlint.outPath")
COLD=(--option narinfo-cache-positive-ttl 0 --option narinfo-cache-negative-ttl 0)
pxenv() { local p=$1; echo env HTTPS_PROXY="$p" HTTP_PROXY="$p" https_proxy="$p" http_proxy="$p" \
  NO_PROXY=localhost,127.0.0.1,::1 no_proxy=localhost,127.0.0.1,::1; }
case_() { local name=$1 n=$2; shift 2; echo "$*" > "$o/$name.cmd"
  for i in $(seq 1 "$n"); do python3 -I -B timed.py "$o/$name.$i" "$@"; done; }
read -ra PXREF <<< "$(pxenv http://127.0.0.1:9)"; read -ra PXBH <<< "$(pxenv http://192.0.2.1:9)"

# control: proxy vars honoured? (curl the cache through the refused proxy)
case_ control-curl-refused 1 "${PXREF[@]}" curl -sS -m 5 -o /dev/null https://cache.nixos.org/nix-cache-info

case_ refused-local-present        $N "${PXREF[@]}" nix path-info -r "$CURL"
case_ refused-local-absent         $N "${PXREF[@]}" nix path-info -r "$SC"
case_ refused-cache-warm           $N "${PXREF[@]}" nix path-info -r --store https://cache.nixos.org "$SC"
case_ refused-cache-cold           $N "${PXREF[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" "$SC"
case_ refused-cache-cold-a1        $N "${PXREF[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" --option download-attempts 1 "$SC"
case_ refused-cache-cold-offline   $N "${PXREF[@]}" nix path-info -r --offline --store https://cache.nixos.org "${COLD[@]}" "$SC"
case_ refused-cache-warm-offline   $N "${PXREF[@]}" nix path-info -r --offline --store https://cache.nixos.org "$SC"
case_ blackhole-cache-cold         $N "${PXBH[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" "$SC"
case_ blackhole-cache-cold-c2      $N "${PXBH[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" --option connect-timeout 2 "$SC"
case_ blackhole-cache-cold-c2-a1   $N "${PXBH[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" --option connect-timeout 2 --option download-attempts 1 "$SC"
case_ blackhole-cache-cold-c2-a2   $N "${PXBH[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" --option connect-timeout 2 --option download-attempts 2 "$SC"
case_ blackhole-hlint-cold-c2-a1   $N "${PXBH[@]}" nix path-info -r --store https://cache.nixos.org "${COLD[@]}" --option connect-timeout 2 --option download-attempts 1 "$HL"
case_ blackhole-local-absent       $N "${PXBH[@]}" nix path-info -r "$SC"

python3 -I -B - "$o" > q5-summary.txt <<'PY'
import sys, os, statistics, glob, re
o = sys.argv[1]
names = sorted({re.sub(r"\.\d+\.exit$", "", os.path.basename(f)) for f in glob.glob(f"{o}/*.exit")})
for n in names:
    runs = sorted(glob.glob(f"{o}/{n}.*.exit"))
    secs = [float(open(f[:-5] + ".secs").read()) for f in runs]
    exits = sorted({open(f).read().strip() for f in runs})
    lines = sorted({sum(1 for l in open(f[:-5] + ".out") if l.strip()) for f in runs})
    err = open(runs[0][:-5] + ".err").read().strip().splitlines()
    last = err[-1] if err else ""
    print(f"{n:30} n={len(runs)} min={min(secs):7.3f} med={statistics.median(secs):7.3f} max={max(secs):7.3f} exit={','.join(exits)} out_lines={lines}\n    cmd: {open(f'{o}/{n}.cmd').read().strip()}\n    last stderr: {last[:300]}")
PY
