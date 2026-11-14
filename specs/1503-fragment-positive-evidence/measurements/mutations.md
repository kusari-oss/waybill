# Mutations (SC-005, T015)

Each mutation edits the one line of `built_by_kaem` in
`waybill-cli/src/scan_fs/package_db/nix/closure/member.rs`:

```rust
        .is_some_and(|program| program == "kaem")
```

Before mutating, the file was copied aside. After each run it was restored with
`cp`, never `git checkout`, and two checks confirmed the restore:
- `git diff --stat` was byte-identical before and after;
- the file was byte-identical (`cmp`) to the saved copy.

Each run was `cargo test --bin waybill s1503`.

| # | mutation | what it models | result | failing tests |
|---|---|---|---|---|
| 1 | `… == "kaem") \|\| true` | the old fall-through: every last-rung member is `fragment` | **3 of 7 fail** | `s1503_a_bash_built_bare_member_is_a_package`, `s1503_no_builder_is_not_evidence`, `s1503_the_builder_test_reads_the_file_name` |
| 2 | `… == "kaem") && false` | the evidence is never accepted | **2 of 7 fail** | `s1503_a_kaem_built_bare_member_is_a_fragment`, `s1503_earlier_rungs_outrank_the_builder` |

**SC-005 holds.** With the evidence check disabled (mutation 1), the bash-built fixture member is `fragment` again, and `s1503_a_bash_built_bare_member_is_a_package` fails.

**Mutation 2 shows the tests can see a check that never fires.** Without it, a check that always returned `false` would look the same as a `package` default that was merely correct.
