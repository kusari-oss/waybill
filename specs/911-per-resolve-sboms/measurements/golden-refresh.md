# Corpus golden refresh (T037, T038)

Regenerated in CI — run
[`35308515189`](https://github.com/kusari-oss/waybill/actions/runs/35308515189),
commit `4a23997d`, `regen_goldens=true`. Never regenerated locally.

## 33 files changed, from TWO independent causes

| cause | targets | what |
|---|---|---|
| **v0.8.0 version bump, not this feature** | all 11 | `"version": "0.7.0"` → `"0.8.0"` in the annotator field |
| this feature | 3 Pants targets | C143 encoding + C161 grammar |

The first is **debt this PR inherits, not damage it does**. The v0.8.0 release
regenerated the six golden-writing test files but deliberately not the
public-corpus goldens, which must be CI-generated; nobody refreshed them
afterwards, so they carried `0.7.0` until now. Worth naming, because a
reviewer seeing 33 changed files should be able to tell which are this
feature's — the answer is 3 targets, not 11.

## Per-target: non-version diff lines (CycloneDX)

```
go-cobra                     0        pants-example-javascript     0
image-postgres16             0        pants-example-jvm           54
maven-guice                  0        pants-example-python        24
npm-express                  0        python-flask                 0
pants-example-django        70        rust-ripgrep                 0
pants-example-golang         0
```

Every target without Pants content differs **only** by the version string.

## The three Pants targets, classified

| target | change | count |
|---|---|---|
| pants-example-python | C143 `"python-default"` → `["python-default"]` | 11 |
| | C161 `k=v;k=v` → JSON object | 1 |
| pants-example-django | C143 | 34 |
| | C161 | 1 |
| pants-example-jvm | C143 `"default"` → `["default"]` | 27 |

Nothing else. No component appeared or disappeared, no edge changed, no
count moved.

## What did NOT appear, and why that is expected

FR-011b raises edge counts where a component belongs to **several** resolves.
No corpus target has one: each is single-resolve, so each component's
membership is a one-element array and no extra edge can arise. The behaviour
is covered by `pants_resolve_membership.rs` against a purpose-built fixture
instead — which is the right place for it, since no public corpus target
exercises the multi-resolve case at all.

That is also a gap worth naming: **the corpus cannot regress FR-011b**. The
issue author's monorepo can, which is why the before/after numbers on #902
matter more than these goldens do.
