#!/bin/bash
# usage: network_probe.sh <list of tests/*.rs files>
# For each candidate test binary (spec 1657, #1715): run it normally, then
# with outbound HTTP(S) sent to an unreachable proxy (localhost exempt, for
# the local OCI registry tests). Records wall time and the test result line
# of each. A much faster blocked run means the tests were waiting on the
# network; failures when blocked mean they depend on it.
cd "$(git rev-parse --show-toplevel)"
while read -r f; do
  name=$(basename "$f" .rs)
  bin=$(ls -t target/debug/deps/"$name"-* 2>/dev/null | grep -v '\.d$' | head -1)
  [ -z "$bin" ] && { echo "$name	NO-BINARY"; continue; }
  s=$(date +%s.%N); r1=$("$bin" --quiet 2>&1 | grep -E '^test result' | tail -1); e=$(date +%s.%N)
  t1=$(echo "$e - $s" | bc)
  s=$(date +%s.%N); r2=$(HTTPS_PROXY=http://127.0.0.1:9 HTTP_PROXY=http://127.0.0.1:9 https_proxy=http://127.0.0.1:9 http_proxy=http://127.0.0.1:9 NO_PROXY=localhost,127.0.0.1 no_proxy=localhost,127.0.0.1 "$bin" --quiet 2>&1 | grep -E '^test result' | tail -1); e=$(date +%s.%N)
  t2=$(echo "$e - $s" | bc)
  printf "%s\tnormal=%.1fs\tblocked=%.1fs\t%s\t|\t%s\n" "$name" "$t1" "$t2" "$(echo $r1 | sed 's/test result: //;s/; 0 measured.*//')" "$(echo $r2 | sed 's/test result: //;s/; 0 measured.*//')"
done < "$1"
