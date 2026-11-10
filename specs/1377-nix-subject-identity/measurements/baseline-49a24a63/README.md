# Control baseline — base commit `49a24a63`, before any milestone-1377 code (T004)

Run 2026-11-09, debug build of `49a24a63`, nixpkgs `a799d3e3`, aarch64-darwin.
grype 0.120.0 with DB schema v6.1.10 **built 2026-11-08T06:19:57Z** (the same DB
as the spike), Trivy 0.71.1, osv-scanner 2.7.0. Produced by `gen-sboms.sh
waybill`, `scan.sh`, then `summarize.sh` with `DEST` set to this directory.

It reproduces the spike (`../scanner-outcome-spike.md`) exactly. `precision.py`
on this run: waybill grype 1,203 findings, 36 patched (3%), runtime 232 / 1;
sbomnix 1,475 / 115, runtime 288 / 1. `analyze.py`: openssh grype 174 waybill,
246 sbomnix, 168 shared.

| file | contents |
| --- | --- |
| `<target>.<scanner>.tsv` | one `id`, `package`, `version` row per finding, `LC_ALL=C sort -u` |
| `<target>.subject.json` | CycloneDX `metadata.component` name, version, purl, cpe, and C69 if emitted |
| `<target>.c184.json` | the `waybill:nix-closure` record, parsed |

Every subject here is `filesystem-scan 0.0.0` with the `waybill:` placeholder
CPE; that is the #1398 defect this milestone removes.

The osv TSVs list every advisory ID osv-scanner reports, aliases included, so
they hold more rows than `analyze.py`'s de-duplicated osv column (openssh: 5
rows, 3 findings). Compare TSV to TSV, and `analyze.py` to `analyze.py`.

T035 re-runs the same three scripts with the same grype DB. A newer DB changes
the finding sets on its own, so check `grype db status` before comparing.
