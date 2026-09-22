# Quickstart — Repo Observation Report

**Feature**: 924-repo-observation-report · **Date**: 2026-09-21

## 0. The self-test — run it against this repository first

This repository is the feature's hardest case and its primary fixture
(SC-001). `waybill-cli/tests/` holds **89 lockfiles** — 27 `go.mod`, 24
`package.json`, 21 `Cargo.toml` — none of which are waybill's dependencies.

```sh
waybill repo report --path . --output /tmp/waybill-self.json
```

A correct report says, for that directory: readers **did** claim those files,
**and** the result is ambiguous, naming the competing interpretations
(polyglot project / test fixtures / vendored examples).

Wrong in two distinguishable ways:

- treating them as real dependencies, or omitting them → the census is lying;
- reporting `claimed` with **no** ambiguity record → the two-field model
  (FR-012a/b) has been collapsed back into a single verdict, which is the
  defect the clarification session removed.

## 1. Does the census reconcile?

The invariant that makes the whole document trustworthy (C-4), checkable
without access to the repository:

```sh
jq '.totals | .files_walked == (.files_claimed + .files_unclaimed + ([.files_skipped[]] | add // 0))' \
  /tmp/waybill-self.json
```

Expect `true`. Anything else means the report is invalid, not merely
imperfect.

## 2. What did waybill fail to recognise?

```sh
jq -r '.directories[]
       | select(.claim_status == "unclaimed")
       | "\(.path)  files=\(.files_direct)  \(.ecosystems[]?.ecosystem // "unrecognised")"' \
  /tmp/waybill-self.json
```

Directories naming an ecosystem with `support: no_reader` are the actionable
ones — a known gap. Note they are typically *covered* (a marker makes a
directory its own project root), so `covered_by == null` is **not** the gap
query; it finds unknown territory:

```sh
jq -r '.directories[] | select(.covered_by == null and (.ecosystems | length) == 0)
       | .path' /tmp/waybill-self.json
```

Ones reporting nothing recognised carry the FR-011 detail instead:

```sh
jq -r '.directories[]
       | select(.observation != null)
       | "\(.path)  \(.observation.content_kind)  files=\(.observation.file_count)"' \
  /tmp/waybill-self.json
```

`predominantly_binary` versus `predominantly_text` is the difference between
"probably build output" and "probably source in a language we do not read".

## 3. Which readers engaged but produced nothing?

The FR-004 distinction, and the highest-signal line in the report for a
maintainer:

```sh
jq -r '.readers[] | select(.files_matched > 0 and .components_emitted == 0)
       | "\(.reader_id): matched \(.files_matched), emitted 0"' \
  /tmp/waybill-self.json
```

Any output here is a reader that saw candidates and produced nothing — a parse
failure or an unsupported dialect. That is a bug report, not a coverage gap,
and the two demand opposite responses.

## 4. Sharing a report

Default output retains repository-relative paths, which is what makes a report
useful to someone who cannot see your repository. It never contains absolute
paths or file contents, in any mode (C-6).

If directory names are themselves sensitive:

```sh
waybill repo report --path . --redact --output /tmp/waybill-redacted.json
```

Segments become stable identifiers — nesting and repetition survive, names do
not. Check what you are about to send:

```sh
jq -r '.redaction_mode' /tmp/waybill-redacted.json     # → "paths"
```

Nothing is ever transmitted automatically (C-8). Sharing is an action you take
after reading the file.

## 5. Determinism, before you diff two reports

```sh
waybill repo report --path . --output /tmp/a.json
waybill repo report --path . --output /tmp/b.json
jq -S 'del(.tool_version, .generated_at)' /tmp/a.json > /tmp/a.n
jq -S 'del(.tool_version, .generated_at)' /tmp/b.json > /tmp/b.n
diff /tmp/a.n /tmp/b.n && echo "deterministic"
```

Rather than hard-coding the deletions, read them from the document — the field
list is self-describing (C-5) and stays correct as the schema grows:

```sh
jq -r '.volatile_fields[]' /tmp/a.json
```

Before diffing two reports from *different* sources, check they agree on
`significance_threshold` (C-9). Reports produced under different thresholds
are not comparable.
