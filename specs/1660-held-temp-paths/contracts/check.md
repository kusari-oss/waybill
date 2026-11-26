# Contract: `scripts/check-held-temp-paths.py`

```
python3 -I -B scripts/check-held-temp-paths.py              # scan every workspace member's .rs files, excluding target/
python3 -I -B scripts/check-held-temp-paths.py --self-test  # run the built-in controls
```

- **Exit status:** 0 when clean, 1 on violations (each printed as `path:line: <statement>`), 2 on usage error.
- **Flags:** a statement whose temporary-file or temporary-directory constructor result has `.path()` chained onto it, without being bound, persisted or marked.
- **Passes:**
  - a bound guard;
  - `.path()` inside the constructor's arguments;
  - `.keep()` and `.persist(`;
  - a line preceded by `// held-temp-path: <reason>`.
- **`--self-test`:** exits 0 only if every must-flag snippet is flagged and every must-pass snippet passes, and prints each result.
