//! Run `spdx3-validate` without the network (#1417).
//!
//! The validator fetches the SPDX 3.0.1 JSON-LD context, JSON schema and
//! SHACL model from spdx.org on every invocation, so every conformance gate
//! failed whenever spdx.org was unreachable. `scripts/spdx3-validate-offline/run.py`
//! serves those three from vendored, checksummed copies and refuses any other
//! URL, then runs the installed validator unchanged.
//!
//! Callers keep checking for the `spdx3-validate` console script, which is
//! what proves the pinned package is installed; this only changes how it runs.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The offline wrapper, by absolute path so the test's working directory
/// does not matter.
pub fn wrapper() -> PathBuf {
    super::workspace_root().join("scripts/spdx3-validate-offline/run.py")
}

/// A `Command` equivalent to `Command::new(validator)`, with the remote
/// resources served locally. `validator` is the venv's `bin/spdx3-validate`;
/// the wrapper runs under that venv's own interpreter, isolated (`-I`) so
/// neither `PYTHON*` variables nor the working directory can change which
/// modules load.
pub fn command(validator: &Path) -> Command {
    let mut cmd = Command::new(validator.with_file_name("python"));
    cmd.arg("-I").arg(wrapper());
    cmd
}
