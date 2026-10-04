#!/usr/bin/env bash
# #853 probe 1: latency of GOPROXY .mod fetches by outcome class.
# Usage: probe_latency.sh <proxy-base> <samples>
set -u
P=${1:-https://proxy.golang.org}; N=${2:-30}
ok=(github.com/spf13/cobra/@v/v1.8.0 github.com/stretchr/testify/@v/v1.9.0 golang.org/x/sys/@v/v0.20.0 github.com/google/uuid/@v/v1.6.0 gopkg.in/yaml.v3/@v/v3.0.1)
measure() { # class url
  curl -s -o /dev/null -w "$1 %{http_code} %{time_total}\n" --connect-timeout 10 --max-time 30 "$2"
}
for i in $(seq 1 "$N"); do
  m=${ok[$((i % ${#ok[@]}))]}
  measure ok "$P/$m.mod"
  measure missing-path "$P/github.com/waybill-probe-853/nonexistent-$RANDOM$i/@v/v1.0.0.mod"
  measure missing-version "$P/github.com/spf13/cobra/@v/v0.0.$((900+i)).mod"
done
