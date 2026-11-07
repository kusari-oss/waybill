# Quickstart: verifying this feature

**Feature**: `1375-generic-cpe-table` | **Issue**: #1381

48 table rows is a large, dull diff that is easy to approve and hard to read.
Every control below asks a question the diff cannot answer.

---

## Control 1 — the vendor is NVD's, not the name's

```sh
jq -r '.components[] | select(.name=="bash") | .cpe' out.cdx.json
```

Must be `cpe:2.3:a:gnu:bash:…` — **not** `bash:bash`. A wrong vendor returns
zero from every scanner, which looks exactly like a component with no known
vulnerabilities.

Spot-check the ones where the vendor is least guessable: `expat`→`libexpat`,
`libxml2`→`xmlsoft`, `brotli`→`google`, `gawk`→`fossies`.

## Control 2 — the three rejections emit nothing

```sh
for n in ninja cmake libxcrypt; do
  jq -r --arg n "$n" '.components[] | select(.name==$n) | "\($n): \(.cpe // "none")"' out.cdx.json
done
```

All three must print `none`. **This is the control that earns its keep.** Each
is attractive — `ninja` has 126 dictionary entries and a matching name — and
each would be a false claim that software is present which is not.

## Control 3 — the C209 delta is exactly ±N

```sh
jq -r '.metadata.properties[]|select(.name=="waybill:cpe-coverage").value' out.cdx.json
```

`with_candidates` up by N, `unaddressable` down by N, same N, everything else
unchanged, sum still equal to `.components | length`. On aarch64-darwin that
is `78 → 127` and `194 → 145`, N = 49.

**If any other count moves**, the change leaked out of the generic arm.

## Control 4 — no existing CPE changed

```sh
diff <(jq -S '[.components[]|select((.cpe//null)!=null)|{name,cpe}]' before.cdx.json) \
     <(jq -S '[.components[]|select((.cpe//null)!=null)|{name,cpe}]' after.cdx.json)
```

Only additions. A **modified** line means an existing row was altered rather
than a new one added (SC-005).

---

## The one thing a test cannot check

`every_table_pair_exists_in_the_nvd_dictionary` proves each pair is *in* the
dictionary. It cannot prove the pair is the *right product* — `ninja` would
satisfy it three times over.

That judgement lives in `research.md` R4 and `measurements/nvd-titles.txt`,
and the titles are committed so a reviewer can **disagree with a specific
call** rather than re-derive all 51. The three worth a second opinion:

- **`gawk` → `fossies`** — accepted, though Fossies is a source-archive site
  rather than GNU. It is the only vendor NVD has.
- **`libxcrypt` → `elektrobit`** — rejected on a single entry. The version in
  the title is a real upstream release, so this may be upstream coverage
  filed under a downstream vendor, or a vendored copy. Unknowable from the
  dictionary; rejected because a missing CPE understates and a wrong one
  asserts.
- **`file` → `gentoo`** — accepted alongside two others. It is a distro
  packaging of the same utility.

---

## Re-running the probes

```sh
# the vendor behind a name
python3 -I specs/1375-generic-cpe-table/measurements/cpe-discover.py bash ninja

# the titles that decide a row
python3 -I specs/1375-generic-cpe-table/measurements/cpe-titles.py ninja cmake flex

# and the control that settles cmake
curl -sS --get https://services.nvd.nist.gov/rest/json/cpes/2.0 \
  --data-urlencode 'cpeMatchString=cpe:2.3:a:kitware:cmake'
```

**Controls are not optional in these probes.** Earlier sweeps in this area
produced plausible wrong answers three times — an orphaned run that exited 0
after 4 of 171 pairs, a run that fused tracebacks into its data rows, and a
probe that returned `ERR` for every input including its positive control.
