# Check controls (T005, T006)

## `--self-test`

```
ok   must flag: tests' exact shape
ok   must flag: unwrap, unqualified
ok   must flag: TempDir joined
ok   must flag: tempdir() joined
ok   must flag: Builder
ok   must flag: ? operator
ok   must flag: as an argument
ok   must pass: bound then used
ok   must pass: held TempDir
ok   must pass: path in the constructor's arguments
ok   must pass: deliberate keep
ok   must pass: into_temp_path bound
ok   must pass: marker
ok   must pass: in a comment
self-test passed
```

## Positive control on the unfixed tree

The check exits 1 and lists 23 sites (`check-before.txt`), exactly research R1's list. `policy_layout.rs:255` (`new_in(tmp.path())`, bound) is correctly not flagged.
