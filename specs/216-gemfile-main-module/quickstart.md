# Quickstart: Ruby application main-module emission

**Feature**: 216-gemfile-main-module
**Date**: 2026-07-22

Operator recipe for the new Gemfile-derived application main-module + how it shows up in split-mode scans.

## The 30-second happy path

```bash
# Bundler-managed Ruby application: Gemfile + Gemfile.lock, no .gemspec
waybill sbom scan --path ./my-rails-app --format cyclonedx-json --output my-app.cdx.json

jq '.metadata.component' my-app.cdx.json
```

Before this feature:
```json
{
  "type": "application",
  "name": "my-rails-app",
  "version": "0.0.0",
  "purl": "pkg:generic/my-rails-app@0.0.0"
}
```
… with `waybill:root-selection-heuristic` = `synthetic-placeholder` (confidence 0.3).

After this feature:
```json
{
  "type": "application",
  "name": "my-rails-app",
  "version": "v2.3.1",
  "purl": "pkg:generic/my-rails-app@v2.3.1",
  "properties": [
    { "name": "waybill:component-role", "value": "main-module" },
    { "name": "waybill:package-shape",  "value": "application" }
  ]
}
```
… with `waybill:root-selection-heuristic` = `repo-root-main-module` (confidence 0.95).

Version becomes the `git describe --tags` value (`v2.3.1`) when the repo is git-tagged; falls back to `0.0.0-unknown` when it isn't.

## Split-mode on a polyglot monorepo

Given a monorepo:
```
./
├── services/user-api/           (Go, go.mod)
├── services/analytics/          (Python, pyproject.toml)
├── infra/deploy/                (Ruby app, Gemfile + Gemfile.lock)
└── infra/monitoring/            (Ruby app, Gemfile + Gemfile.lock)
```

Split scan emits one sub-SBOM per subproject including the two Ruby applications:

```bash
waybill sbom scan --path . --split --output-dir ./sboms/

ls ./sboms/
# analytics.pypi.cdx.json
# deploy.generic.cdx.json         ← NEW: Ruby app
# monitoring.generic.cdx.json     ← NEW: Ruby app
# split-manifest.json
# user-api.go.cdx.json
```

Before this feature: only `analytics.pypi.cdx.json` + `user-api.go.cdx.json` + `split-manifest.json` — the two Ruby applications would silently omit.

## Distinguishing Ruby applications from published gems

Every SBOM emitted for a Ruby-application root carries the `waybill:package-shape = "application"` property. Downstream consumers can filter:

```bash
# Every application (across every ecosystem) — any component tagged with the
# waybill:package-shape = "application" annotation.
jq '.metadata.component | select(.properties[]? | select(.name == "waybill:package-shape" and .value == "application"))' *.cdx.json

# Every RUBY application specifically — filter also on pkg:generic/ + a Gemfile-shaped
# heuristic on the components[] list (presence of rubygems.org-sourced components).
jq '.metadata.component | select(
      .purl | startswith("pkg:generic/")
    ) | select(
      .properties[]? | select(.name == "waybill:package-shape" and .value == "application")
    )' *.cdx.json
```

## Inspecting the split manifest for Ruby-app entries

```bash
jq '.entries[] | select(.subproject_id | endswith(".generic"))' ./sboms/split-manifest.json
```

Example output:
```json
{
  "subproject_id": "deploy.generic",
  "root_purl": "pkg:generic/deploy@v1.2.0",
  "source_dir": "infra/deploy",
  "component_count": 47,
  "shared_deps_count": 3,
  "files": { "cyclonedx-json": "deploy.generic.cdx.json" }
}
```

## Application without a Gemfile.lock

```bash
# Directory with just Gemfile, no lock
waybill sbom scan --path ./unlocked-app --format cyclonedx-json --output unlocked.cdx.json

jq '.metadata.properties[] | select(.name | test("graph-completeness"))' unlocked.cdx.json
```

Output:
```json
{ "name": "waybill:graph-completeness", "value": "partial" }
{ "name": "waybill:graph-completeness-reason", "value": "no-lockfile-found: transitive deps unavailable" }
```

The main-module is still emitted (with a `pkg:generic/` PURL); only the transitive-dep tree is degraded — matching how the pip reader behaves on pyproject-without-lock projects.

## Directory containing both Gemfile AND .gemspec

Published-gem shape wins:
```bash
waybill sbom scan --path ./published-gem-with-gemfile --format cyclonedx-json --output pub.cdx.json

jq '.metadata.component.purl' pub.cdx.json
# → "pkg:gem/my-published-gem@1.0.0"     (NOT pkg:generic/)
```

Exactly ONE main-module is emitted per directory — the gemspec-derived one (pre-feature behavior). FR-007 guarantees no double-emission.

## Verification checklist

After running a scan against a Gemfile-only fixture:

```bash
# 1. Main-module PURL uses pkg:generic/
[ "$(jq -r '.metadata.component.purl' out.cdx.json | grep -c '^pkg:generic/')" = "1" ] && echo "OK: pkg:generic/ root"

# 2. waybill:package-shape annotation present with value "application"
jq -e '.metadata.component.properties[]? | select(.name == "waybill:package-shape" and .value == "application")' out.cdx.json > /dev/null && echo "OK: application shape"

# 3. m127 root-selector picked the Ruby-app heuristic (not synthetic-placeholder)
jq -r '.metadata.properties[] | select(.name == "waybill:root-selection-heuristic") | .value' out.cdx.json | jq -r '.value.heuristic' | grep -v synthetic && echo "OK: real heuristic"
```

## Rollback / opt-out

There is NO opt-out flag. The emission is unconditional whenever the walker's predicate matches. Rationale: the emission is additive-only for pre-feature scans (no drift on non-Ruby or gemspec-only fixtures per FR-009/FR-010), so no operator ever needs to turn it off. If a downstream consumer breaks on the new `pkg:generic/` root PURL, the remediation is to fix the consumer's PURL-type expectations (they were probably keying on the m127 synthetic-placeholder pattern which was a bug, not a feature).
