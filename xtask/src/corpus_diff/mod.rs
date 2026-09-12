// Feature 840 (issue #763) — review-time normaliser for public-corpus
// golden diffs.
//
// The goldens were last written on 2026-07-21 and roughly 147 merges
// have landed since. The raw diff is therefore enormous and mostly
// benign, which is exactly the condition under which a real regression
// hides. This tool exists so a human can read that diff.
//
// It is NOT part of the gate. `waybill-cli/tests/corpus_harness_195/`
// keeps byte-identity comparison as the authority. Two consequences
// follow, both enforced below and in tests:
//
//   * this never writes to a golden. A normaliser that wrote back would
//     make a reordered-but-equal golden compare EQUAL, silently
//     weakening the very gate it serves (contract C-2.3).
//   * this never re-applies the harness's masking. Goldens are stored
//     already masked (`layer2_golden.rs:51`); masking twice risks
//     diverging from what the lane actually compares (C-2.4).
//
// Masking substitutes values; it cannot reorder. Array ordering is the
// one category masking cannot close, and SPDX 3 wraps its output in a
// `@graph` array whose order is not stable across runs. Left alone, a
// reordered array presents as every element changing and buries real
// drift underneath the volume. Closing that gap is this tool's whole job.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Args;
use serde_json::Value;

#[cfg(test)]
mod tests;

#[derive(Args, Debug)]
pub struct CorpusDiffArgs {
    /// Old golden. Mutually exclusive with `--target`.
    #[arg(long, conflicts_with = "target")]
    pub old: Option<PathBuf>,

    /// New golden. Used with `--old`.
    #[arg(long, requires = "old")]
    pub new: Option<PathBuf>,

    /// Corpus target name; compares its committed goldens against `--old-ref`.
    #[arg(long, conflicts_with = "old")]
    pub target: Option<String>,

    /// Git ref to compare the target's goldens against. Defaults to HEAD.
    #[arg(long, default_value = "HEAD")]
    pub old_ref: String,

    /// Restrict to one format. Default: all three.
    #[arg(long, value_parser = ["cdx", "spdx-2.3", "spdx-3"])]
    pub format: Option<String>,
}

const FORMATS: [&str; 3] = ["cdx", "spdx-2.3", "spdx-3"];

/// Contract C-1.2: exit 0 whether or not differences exist. This is a
/// reading tool, not a gate — a non-zero exit would invite someone to
/// wire it into CI as a second gate, which it must not become (C-4.1).
pub fn run(args: CorpusDiffArgs) -> Result<(), Box<dyn Error>> {
    match (&args.old, &args.target) {
        (Some(old), _) => {
            let new = args
                .new
                .as_ref()
                .ok_or_else(|| -> Box<dyn Error> { "--old requires --new".into() })?;
            let label = args.format.clone().unwrap_or_else(|| "file".to_string());
            report(&label, &read_json(old)?, &read_json(new)?);
            Ok(())
        }
        (None, Some(target)) => diff_target(target, &args.old_ref, args.format.as_deref()),
        (None, None) => Err("pass either --old/--new or --target".into()),
    }
}

fn diff_target(
    target: &str,
    old_ref: &str,
    only: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let formats: Vec<&str> = match only {
        Some(f) => vec![f],
        None => FORMATS.to_vec(),
    };
    for format in formats {
        let path = golden_path(target, format);
        if !path.exists() {
            return Err(format!(
                "no golden at {} — is `{target}` a corpus target?",
                path.display()
            )
            .into());
        }
        let new = read_json(&path)?;
        let old = read_json_at_ref(old_ref, &path)?;
        // C-3.1: identify target and format, so a diff pasted into a PR
        // is attributable without its invocation.
        report(&format!("{target} / {format}"), &old, &new);
    }
    Ok(())
}

fn golden_path(target: &str, format: &str) -> PathBuf {
    PathBuf::from("waybill-cli/tests/fixtures/public_corpus")
        .join(target)
        .join(format!("{format}.json"))
}

fn read_json(path: &Path) -> Result<Value, Box<dyn Error>> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is not valid JSON: {e}", path.display()).into())
}

fn read_json_at_ref(git_ref: &str, path: &Path) -> Result<Value, Box<dyn Error>> {
    let spec = format!("{git_ref}:{}", path.display());
    let out = Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|e| format!("git show {spec}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git show {spec}: {}", err.lines().next().unwrap_or("failed")).into());
    }
    serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("{spec} is not valid JSON: {e}").into())
}

/// Sort every array in the document by a stable key derived from its
/// contents, recursively.
///
/// Contract C-2.5: the key must be total and deterministic. Sorting by
/// the canonical serialisation of each element gives that for free — two
/// elements comparing equal are byte-identical, so ties cannot leave
/// order input-dependent.
///
/// Applied to BOTH sides identically (C-2.2). Asymmetric normalisation
/// manufactures differences.
pub fn normalise(value: &Value) -> Value {
    match value {
        Value::Array(items) => {
            let mut normalised: Vec<Value> = items.iter().map(normalise).collect();
            normalised.sort_by_cached_key(canonical_key);
            Value::Array(normalised)
        }
        Value::Object(map) => {
            // serde_json's Map preserves insertion order unless the
            // `preserve_order` feature is off; normalising the values is
            // enough because key order does not survive canonical
            // serialisation comparison below.
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), normalise(v));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn canonical_key(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// Print a normalised diff. Empty output means the two documents differ
/// only in ways this tool considers non-semantic.
fn report(label: &str, old: &Value, new: &Value) {
    let old_n = normalise(old);
    let new_n = normalise(new);
    if old_n == new_n {
        println!("{label}: no semantic change after normalisation");
        return;
    }
    println!("=== {label} ===");
    let mut lines = Vec::new();
    walk(&old_n, &new_n, String::from("$"), &mut lines);
    // C-3.2: group repeated shapes so a reviewer sees "N components
    // gained X" rather than N separate hunks. C-3.3: anything matching
    // no group is shown individually and never summarised away — those
    // are the FR-007 cases, the entire point of the exercise.
    let (grouped, singles) = group(lines);
    for (shape, count) in grouped {
        println!("  [{count}x] {shape}");
    }
    for line in singles {
        println!("  {line}");
    }
}

fn walk(old: &Value, new: &Value, path: String, out: &mut Vec<String>) {
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (a.get(k), b.get(k)) {
                    (Some(x), Some(y)) => walk(x, y, format!("{path}.{k}"), out),
                    (Some(_), None) => out.push(format!("removed {path}.{k}")),
                    (None, Some(_)) => out.push(format!("added {path}.{k}")),
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                walk(x, y, format!("{path}[{i}]"), out);
            }
        }
        (a, b) if a != b => out.push(format!("changed {path}")),
        _ => {}
    }
}

/// Collapse repeated shapes. A shape is the path with array indices
/// stripped, so `$.components[3].licenses` and `$.components[91].licenses`
/// are one shape.
///
/// A change appearing exactly once is NOT a shape — it is returned
/// individually. Folding singletons into a group is how a regression
/// gets absorbed into a large benign diff (research.md R4).
fn group(lines: Vec<String>) -> (Vec<(String, usize)>, Vec<String>) {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    for line in &lines {
        let shape = strip_indices(line);
        *counts.entry(shape.clone()).or_insert(0) += 1;
        first.entry(shape).or_insert_with(|| line.clone());
    }
    let mut grouped = Vec::new();
    let mut singles = Vec::new();
    for (shape, count) in counts {
        if count > 1 {
            grouped.push((shape, count));
        } else {
            singles.push(first[&shape].clone());
        }
    }
    grouped.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (grouped, singles)
}

fn strip_indices(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_index = false;
    for c in s.chars() {
        match c {
            '[' => {
                in_index = true;
                out.push_str("[]");
            }
            ']' => in_index = false,
            _ if in_index => {}
            _ => out.push(c),
        }
    }
    out
}
