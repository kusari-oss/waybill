#!/bin/sh
# Spec 1645: the version forms NVD's CVE configurations use for the CPE
# products of the two split libraries. Re-run when the decision needs
# re-checking; NVD's records change. Usage: sh nvd-version-forms.sh OUTDIR
set -eu
out=${1:?outdir}; mkdir -p "$out"
for p in invisible-island:ncurses llvm:llvm; do
  f="$out/$(echo "$p" | tr : _).json"
  curl -s "https://services.nvd.nist.gov/rest/json/cves/2.0?virtualMatchString=cpe:2.3:a:$p&resultsPerPage=2000" -o "$f"
  echo "== cpe:2.3:a:$p  totalResults=$(jq .totalResults "$f")"
  echo "   count  version:update | startIncluding | endExcluding | endIncluding"
  jq -r '[.vulnerabilities[].cve.configurations[]?.nodes[]?.cpeMatch[]?
          | select(.criteria|test(":(ncurses|llvm):"))
          | [(.criteria|split(":")[5:7]|join(":")), (.versionStartIncluding//""), (.versionEndExcluding//""), (.versionEndIncluding//"")]
          | join(" | ")] | .[]' "$f" | sort | uniq -c | sort -rn
  sleep 7
done
