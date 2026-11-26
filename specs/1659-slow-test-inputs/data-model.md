# Data model: spec 1659

No runtime data.

- **Guard** (`self_scan_guards.rs`): a name, plus a function from the scanned CycloneDX value to `Result<(), String>`, where the message is the original test's assertion text. The provenance guard can also record a skip.
- **Timing row** (`measurements/runs.tsv`): test, phase (before or after), run number, seconds.
