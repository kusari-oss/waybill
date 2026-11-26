# Data model: spec 1661

- **Local rebuild row** (`measurements/local.tsv`): setting, rebuild number, entry count in `target/debug/deps`, seconds.
- **CI row** (`measurements/ci.tsv`): run ID, lane, setting, test-build seconds, `Tests`-step seconds, backtrace test result (pass or fail), and whether a file:line appeared.
