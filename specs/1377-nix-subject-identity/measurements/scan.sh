#!/usr/bin/env bash
# Spike: run grype, trivy and osv-scanner over every generated SBOM.
# Raw JSON lands next to each SBOM as <sbom>.<scanner>.json; exit codes and
# timings are printed so a scanner that silently failed is not read as
# "found nothing" (feedback_zero_results_can_mean_broken).
set -uo pipefail
W=/Users/mlieberman/Projects/mikebom/target/nix-vuln-spike/out
GRYPE=/nix/store/7r24kzxg8in0g0vydh63z6ybrm5p7cmy-grype-0.120.0/bin/grype
for t in curl openssh httpie gh ripgrep moat; do
  for f in sbomnix-bt sbomnix-rt waybill; do
    s="$W/$t/$f.cdx.json"; [ -s "$s" ] || continue
    st=$(date +%s); "$GRYPE" "sbom:$s" -o json --file "$s.grype.json" >/dev/null 2>"$s.grype.err"
    echo "$t $f grype exit=$? $(( $(date +%s)-st ))s"
    st=$(date +%s); trivy sbom --quiet --format json --output "$s.trivy.json" "$s" 2>"$s.trivy.err"
    echo "$t $f trivy exit=$? $(( $(date +%s)-st ))s"
    # osv-scanner exits 1 when it FINDS vulns; >1 is an error.
    st=$(date +%s); osv-scanner scan source -L "$s" --format json >"$s.osv.json" 2>"$s.osv.err"
    echo "$t $f osv exit=$? $(( $(date +%s)-st ))s"
  done
done
