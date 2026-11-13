//! #1360 — `--tier` filters the nix closure tier's components too.
//!
//! The filter ran above the closure merge, so `--tier source-only
//! --nix-closure` still emitted every build-tier closure component. Measured
//! on a nixpkgs `openssh` wrapper flake: 457 build-tier components and 2,455
//! edges under every `--tier` mode. The closure is JSON handed to a stub
//! `nix`, never evaluated by a real one.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};

use common::nix_stub::{NixStub, Rows, Scan, DEFAULT_ATTR};

const CLOSURE: &str = r#"{
  "derivations": {
    "d-root.drv": {
      "name": "waybill-fixture-tierproj-0.1.0",
      "env": {
        "pname": "waybill-fixture-tierproj",
        "version": "0.1.0",
        "buildInputs": "/nix/store/h1-liba /nix/store/h2-libb"
      },
      "inputs": { "drvs": {
        "d-liba.drv": { "outputs": ["out"] },
        "d-libb.drv": { "outputs": ["out"] }
      } },
      "outputs": { "out": { "path": "h0-root" } }
    },
    "d-liba.drv": {
      "name": "waybill-fixture-liba-1.0",
      "env": { "pname": "waybill-fixture-liba", "version": "1.0" },
      "outputs": { "out": { "path": "h1-liba" } }
    },
    "d-libb.drv": {
      "name": "waybill-fixture-libb-2.0",
      "env": { "pname": "waybill-fixture-libb", "version": "2.0" },
      "outputs": { "out": { "path": "h2-libb" } }
    }
  },
  "version": 4
}"#;

fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/closure_role_scope/project")
}

fn scan(extra: &[&str]) -> Scan {
    let mut args = vec!["--nix-closure", "--no-deps-dev", "--no-clearly-defined"];
    args.extend_from_slice(extra);
    NixStub {
        project: project(),
        closure: CLOSURE,
        rows: Rows::Fail,
        attr: DEFAULT_ATTR,
        ..Default::default()
    }
    .scan(&args)
}

fn tiers(s: &Scan) -> Vec<(String, String)> {
    s.cdx["components"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let tier = c["properties"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == "waybill:sbom-tier")
                .and_then(|p| p["value"].as_str())
                .unwrap_or("")
                .to_string();
            (c["name"].as_str().unwrap_or("").to_string(), tier)
        })
        .collect()
}

fn closure_names(t: &[(String, String)]) -> Vec<&str> {
    t.iter()
        .map(|(n, _)| n.as_str())
        .filter(|n| *n == "waybill-fixture-liba" || *n == "waybill-fixture-libb")
        .collect()
}

#[test]
fn m1360_tier_source_only_drops_the_closure_build_tier() {
    // CONTROL: without `--tier`, both closure members are emitted, and as
    // build-tier components. Otherwise the assertion below holds vacuously.
    let all = tiers(&scan(&[]));
    assert_eq!(
        closure_names(&all).len(),
        2,
        "both closure members must be emitted without --tier; got {all:?}"
    );
    assert!(
        all.iter()
            .filter(|(n, _)| n.starts_with("waybill-fixture-lib"))
            .all(|(_, t)| t == "build"),
        "closure members are build-tier; got {all:?}"
    );

    for mode in ["source-only", "design-only", "source-and-binary"] {
        let s = scan(&["--tier", mode]);
        let got = tiers(&s);
        assert!(
            closure_names(&got).is_empty(),
            "--tier {mode} must drop the build-tier closure members; got {got:?}"
        );
        assert!(
            !got.iter().any(|(_, t)| t == "build"),
            "--tier {mode} emitted a build-tier component; got {got:?}"
        );
        // No edge may name a component the filter dropped.
        let refs: Vec<&str> = s.cdx["components"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["bom-ref"].as_str())
            .chain(s.cdx["metadata"]["component"]["bom-ref"].as_str())
            .collect();
        for d in s.cdx["dependencies"].as_array().into_iter().flatten() {
            for to in d["dependsOn"].as_array().into_iter().flatten() {
                let to = to.as_str().unwrap();
                assert!(refs.contains(&to), "--tier {mode}: dangling edge to {to}");
            }
        }
    }
}
