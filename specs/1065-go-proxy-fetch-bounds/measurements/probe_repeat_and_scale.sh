#!/usr/bin/env bash
# #853 probe 2: (a) repeat-request latency for one missing module (proxy negative cache);
# (b) wall clock for N distinct missing modules fetched 16-way, as waybill does.
set -u
P=${1:-https://proxy.golang.org}
u="$P/github.com/waybill-probe-853/repeat-$RANDOM/@v/v1.0.0.mod"
for i in 1 2 3 4 5; do curl -s -o /dev/null -w "repeat $i %{http_code} %{time_total}\n" --max-time 30 "$u"; done
for N in 16 64 256; do
  tag=$RANDOM
  start=$(python3 -c 'import time;print(time.time())')
  seq 1 "$N" | xargs -P 16 -I{} curl -s -o /dev/null -w "%{http_code}\n" --connect-timeout 10 --max-time 30 \
    "$P/github.com/waybill-probe-853/scale-$tag-{}/@v/v1.0.0.mod" | sort | uniq -c | tr '\n' ' '
  end=$(python3 -c 'import time;print(time.time())')
  python3 -c "print(' N=$N wall=%.1fs  per-wave(16)=%.2fs' % ($end-$start, ($end-$start)/(( $N+15)//16)))"
done
