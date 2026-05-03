# pip-pyproject-pep621 fixture (milestone 068)

Minimal PEP 621 `pyproject.toml` exercising:

- `[project].name = "my_pkg"` (underscore — PEP 503 normalization
  must produce `pkg:pypi/my-pkg@1.0.0`)
- `[project].version` literal
- `[project].dependencies` with two PEP 508 requirement strings

Used by integration tests in `tests/scan_pip.rs`.
