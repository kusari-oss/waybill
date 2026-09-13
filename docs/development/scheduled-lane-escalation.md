# Escalating a scheduled lane's failures

Every cron-scheduled workflow in this repo must announce its own
failures. A lane that only uploads an artifact and stops is not a check —
it is a check plus an assumption that someone reads the Actions tab.

## Why this is a written rule

`public-corpus.yml` fired correctly on cron 51 nights running and failed
all 51 times while the goldens rotted. Detection was never broken. Every
failure landed as a red square nobody opened, from 2026-07-24 to
2026-09-12 ([#859](https://github.com/kusari-oss/waybill/issues/859)).

`ebpf-canary.yml` — cron `0 6 * * *`, seventeen minutes earlier — opened
an issue on failure and is why [#685](https://github.com/kusari-oss/waybill/issues/685)
stayed visible the whole time. Same repo, same hour, same class of check,
opposite outcome. The entire difference was one step in a YAML file.

The sweep that followed found three more lanes with no escalation. One of
them (`quality-corpus.yml`) had already failed three nights running
without telling anyone; it went green again only because a separate PR
happened to correct the same root cause from another direction. The lane
itself informed no one at any point.

## Adding it to a new lane

Add a second job. The composite action does the rest:

```yaml
  report-failure:
    name: Report <lane> failure
    needs: <the job that can fail>
    if: failure() && github.event_name == 'schedule'
    runs-on: ubuntu-latest
    permissions:
      contents: read
      issues: write
    steps:
      - name: Checkout
        uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
        with:
          persist-credentials: false

      - uses: ./.github/actions/report-scheduled-failure
        with:
          title: '[canary] <what is wrong, in the present tense>'
          labels: 'canary,<lane>'
          detail: ${{ needs.<job>.outputs.detail }}   # optional
          triage: |
            What this lane guards, and what to do about a failure.
```

The `checkout` step is required — a local composite action cannot be
resolved without the repository on disk.

## The four things that matter

1. **Dedupe by exact title.** N failures become one issue plus N
   comments, never N issues. The action matches the title exactly:
   trailing whitespace or a changed capital starts a *new* issue and
   orphans the old one. Do not edit a live title.

2. **Gate on `github.event_name == 'schedule'`.** This one bites if
   missed. A manual dispatch is often *expected* to fail — the corpus
   lane's `regen_goldens=true` refresh does, by design — and escalating
   those drowns the signal within a week.

3. **Say what broke, not just that something did.** Tee the gating
   command to a log and extract the failing rows into a job output. A
   lane that cannot cheaply do that can omit `detail:` entirely; the
   action falls back to enumerating the run's failed jobs and steps from
   the API, which is the right answer for a matrix job anyway, since the
   arms would otherwise overwrite each other's output.

4. **Put the triage in the issue.** The person who opens it at 09:00 is
   not necessarily the person who wrote the lane. Say which failures are
   routine, which are suspicious, and which known issue a recurring one
   belongs to.

## The comment count is the staleness metric

Because dedupe appends, the comment count *is* how many consecutive runs
the lane has been red. The action states it outright ("Failure 13") and
adds a banner past seven, so nobody has to count. That number going
unread for seven weeks is the entire reason #859 exists.

Close the issue once the lane is green; the next failure opens a fresh
one and the count restarts.

## Do not silence a lane to make it green

Widening a quality range, re-baselining a benchmark, or regenerating a
golden will all turn a lane green without anyone learning why it went
red. Each destroys the comparison the lane exists to make. Understand the
numbers first, then change the bound deliberately and say so in the PR.

## Two lanes still carry their own copy

`ebpf-canary.yml` and `keyless-conformance.yml` predate the shared action
and have equivalent logic inlined. They work, and both have live dedupe
keys with real comment history, so migrating them is a follow-up rather
than a cleanup to do in passing. New lanes should use the shared action.
