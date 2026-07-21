# m202 License-Splitter Test Fixture

Synthetic ipk built to reproduce #579: a compound License field with one
canonical SPDX operand + one non-canonical operand (`bzip2-1.0.4` — SPDX
has `bzip2-1.0.6` but not `-1.0.4`).

Control:
```
Package: test
Version: 1.0
License: GPL-2.0-only & bzip2-1.0.4
Architecture: all
```

Regenerate:
```bash
# See spec quickstart Reproducer 2.
```
