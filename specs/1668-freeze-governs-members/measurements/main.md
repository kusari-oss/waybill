# `main` 68dc440c fixture baseline (T001, T003)

Built with `fixture.sh`; raw rows are in `main/summary.tsv`, and main modules in `main/mains.tsv`.

| Fixture | `waybill-fixture-frozen` | `@4.0.0` user | `@4.0.0` C45 | `-unused@1.0.0` C45 |
|---|---|---|---|---|
| (a) `packages: app` | versionless, plus `@4.0.0` | none | `edgeless-lock-pin` | `edgeless-lock-pin` |
| (b) `libs/*` | versionless (from y), plus `@4.0.0` | the synthetic root `pkg:generic/waybill-fixture-tree@0.0.0` | none | none (also attached to the synthetic root) |
| (c) `./app/` | as (a) | none | `edgeless-lock-pin` | `edgeless-lock-pin` |
| (d) `app/…cabal` | as (a) | none | `edgeless-lock-pin` | `edgeless-lock-pin` |
| (e) hpack member | as (a) | none | `edgeless-lock-pin` | `edgeless-lock-pin` |
| (f) control `packages: .` | `@4.0.0` only | app | none | `edgeless-lock-pin` |
| (g) unlisted `other/` and `../outside` | as (a) | none | `edgeless-lock-pin` | `edgeless-lock-pin` |

Findings:
- **#1744 reproduces** in (a) and (c)–(e). Research R1 holds, and the hpack member (e) behaves exactly like the `.cabal` member.
- **(b) hides the defect differently.** With two main modules, waybill synthesises a root, and the unreachable pins attach to it rather than staying orphans. The versionless twin is still there.
- **(g):** #1032's membership filter drops `other/`, and `../outside` is outside the scan root, so neither is emitted. FR-007 has nothing to change there.
- **`waybill-fixture-loose`** is versionless design-tier and reached from the member in every fixture. FR-011 requires that to stay the same.

## Branch (T009 preview)

- **(a)–(e) and (g):** one `waybill-fixture-frozen@4.0.0` component, reached from the member, with no C45 and with `cabal.project.freeze` as its source file. The versionless twin is gone.
- **`waybill-fixture-unused@1.0.0`:** unchanged; it is `edgeless-lock-pin` in all except (b), where it attaches to the synthetic root as it did on `main`.
- **`waybill-fixture-loose`:** unchanged in every fixture.
- **Not predicted:** each governed member's main module goes from `waybill:sbom-tier` `design` to `source`. Control (f) already has `source` on `main`, because a governing lock makes a package's resolution source tier. FR-011 was amended to say this. No other main-module annotation changes, and (b)'s synthetic root is unchanged.
