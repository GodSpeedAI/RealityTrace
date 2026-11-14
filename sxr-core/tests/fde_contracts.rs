//! WP-11 conformance for the FDE contracts under `contracts/`: every valid
//! fixture validates, every invalid fixture fails at the pointer and for the
//! reason its manifest names, rules a schema cannot state (order, counts that
//! must agree, coverage status) are checked against their stated codes, and
//! the DIGESTS manifest matches the schemas. Schema validation only.

use jsonschema::JSONSchema;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts")
}

fn read(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

fn schema(contract: &str) -> JSONSchema {
    JSONSchema::compile(&read(&root().join(format!("{contract}.v1.json"))))
        .unwrap_or_else(|e| panic!("{contract}: {e}"))
}

fn errors(s: &JSONSchema, instance: &Value) -> Vec<(String, String)> {
    match s.validate(instance) {
        Ok(()) => vec![],
        Err(it) => it
            .map(|e| (e.instance_path.to_string(), e.to_string()))
            .collect(),
    }
}

fn manifest(contract: &str) -> Value {
    read(&root().join("fixtures").join(contract).join("manifest.json"))
}

fn fixture(contract: &str, sub: &str, file: &str) -> Value {
    read(&root().join("fixtures").join(contract).join(sub).join(file))
}

fn ids(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect()
}

fn strictly_ascending(items: &[&str]) -> bool {
    items.windows(2).all(|w| w[0] < w[1])
}

fn sum(v: &Value) -> u64 {
    v.as_object()
        .unwrap()
        .values()
        .map(|c| c.as_u64().unwrap())
        .sum()
}

fn check_profile(p: &Value) -> Result<(), &'static str> {
    let kinds: Vec<&str> = p["kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    let mut sorted = kinds.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() == kinds.len() {
        Ok(())
    } else {
        Err("PROFILE_DUPLICATE_KIND")
    }
}

fn check_representation(r: &Value) -> Result<(), &'static str> {
    let nodes = r["nodes"].as_array().unwrap();
    let node_ids: Vec<&str> = nodes
        .iter()
        .map(|n| n["node_id"].as_str().unwrap())
        .collect();
    if !strictly_ascending(&node_ids) {
        return Err("REP_NOT_CANONICAL_ORDER");
    }
    for n in nodes {
        if !strictly_ascending(&ids(&n["occurrence_ids"])) {
            return Err("REP_NOT_CANONICAL_ORDER");
        }
        let f = n["frequency"].as_u64().unwrap();
        if f != n["occurrence_ids"].as_array().unwrap().len() as u64 {
            return Err("REP_FREQUENCY_MISMATCH");
        }
        let sums = [
            sum(&n["context_distribution"]["roles"]),
            sum(&n["context_distribution"]["plan_items"]),
            sum(&n["authority_outcomes"]),
            sum(&n["outcome_status"]),
        ];
        if sums.iter().any(|s| *s != f) || n["independent_occurrence_count"].as_u64().unwrap() > f {
            return Err("REP_DISTRIBUTION_MISMATCH");
        }
    }
    let mut last: Option<(&str, &str)> = None;
    for e in r["edges"].as_array().unwrap() {
        let (from, to) = (e["from"].as_str().unwrap(), e["to"].as_str().unwrap());
        if !node_ids.contains(&from) || !node_ids.contains(&to) {
            return Err("REP_EDGE_ENDPOINT_UNKNOWN");
        }
        if last.is_some_and(|l| l >= (from, to)) || !strictly_ascending(&ids(&e["occurrence_ids"]))
        {
            return Err("REP_NOT_CANONICAL_ORDER");
        }
        last = Some((from, to));
        let o = &e["ordering"];
        if o["via_sequence"].as_u64().unwrap() + o["via_causation"].as_u64().unwrap()
            != e["frequency"].as_u64().unwrap()
        {
            return Err("REP_DISTRIBUTION_MISMATCH");
        }
    }
    for c in r["coverage"].as_array().unwrap() {
        let covered = c["observed_count"].as_u64().unwrap() > 0;
        if covered != (c["status"] == "covered") {
            return Err("REP_COVERAGE_INCONSISTENT");
        }
    }
    Ok(())
}

fn semantic(contract: &str, instance: &Value) -> Result<(), &'static str> {
    match contract {
        "comparison-profile" => check_profile(instance),
        "observed-representation" => check_representation(instance),
        _ => Ok(()),
    }
}

fn run_contract(contract: &str, min_valid: usize, min_invalid: usize) {
    let m = manifest(contract);
    let s = schema(contract);

    let valid = m["valid"].as_array().unwrap();
    assert!(
        valid.len() >= min_valid,
        "{contract}: need {min_valid} valid fixtures"
    );
    for e in valid {
        let file = e["file"].as_str().unwrap();
        let inst = fixture(contract, "valid", file);
        assert_eq!(errors(&s, &inst), vec![], "{contract}/valid/{file}");
        assert_eq!(semantic(contract, &inst), Ok(()), "{contract}/valid/{file}");
    }

    let invalid = m["invalid"].as_array().unwrap();
    let semantic_invalid = m["semantic_invalid"].as_array().unwrap();
    assert!(
        invalid.len() + semantic_invalid.len() >= min_invalid,
        "{contract}: need {min_invalid} invalid fixtures"
    );
    for e in invalid {
        let file = e["file"].as_str().unwrap();
        let errs = errors(&s, &fixture(contract, "invalid", file));
        assert!(
            !errs.is_empty(),
            "{contract}/invalid/{file}: schema accepted it"
        );
        let (ptr, needle) = (
            e["pointer"].as_str().unwrap(),
            e["contains"].as_str().unwrap(),
        );
        assert!(
            errs.iter().any(|(p, msg)| p == ptr && msg.contains(needle)),
            "{contract}/invalid/{file}: wanted {ptr} ~ {needle:?}, got {errs:?}"
        );
    }
    for e in semantic_invalid {
        let file = e["file"].as_str().unwrap();
        let inst = fixture(contract, "invalid", file);
        assert_eq!(
            errors(&s, &inst),
            vec![],
            "{contract}/invalid/{file} is meant to pass the schema"
        );
        assert_eq!(
            semantic(contract, &inst),
            Err(e["code"].as_str().unwrap()),
            "{contract}/invalid/{file}"
        );
    }

    let mut on_disk: Vec<String> = ["valid", "invalid"]
        .iter()
        .flat_map(|sub| {
            std::fs::read_dir(root().join("fixtures").join(contract).join(sub))
                .unwrap()
                .map(move |f| format!("{sub}/{}", f.unwrap().file_name().to_string_lossy()))
        })
        .collect();
    let mut listed: Vec<String> = valid
        .iter()
        .map(|e| format!("valid/{}", e["file"].as_str().unwrap()))
        .chain(
            invalid
                .iter()
                .chain(semantic_invalid)
                .map(|e| format!("invalid/{}", e["file"].as_str().unwrap())),
        )
        .collect();
    on_disk.sort();
    listed.sort();
    assert_eq!(on_disk, listed, "{contract}: manifest and files differ");
}

#[test]
fn comparison_profile_fixtures() {
    run_contract("comparison-profile", 2, 3);
}

#[test]
fn observed_representation_fixtures() {
    run_contract("observed-representation", 2, 3);
}

#[test]
fn residual_fixtures() {
    run_contract("residual", 2, 3);
}

#[test]
fn every_class_and_both_comparisons_have_a_valid_residual() {
    let mut classes = std::collections::BTreeSet::new();
    let mut comparisons = std::collections::BTreeSet::new();
    for e in manifest("residual")["valid"].as_array().unwrap() {
        let r = fixture("residual", "valid", e["file"].as_str().unwrap());
        classes.insert(r["class"].as_str().unwrap().to_string());
        comparisons.insert(r["comparison"].as_str().unwrap().to_string());
    }
    assert_eq!(classes.len(), 5, "{classes:?}");
    assert_eq!(comparisons.len(), 2);
}

#[test]
fn every_error_code_is_documented_in_the_readme() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    for c in ["comparison-profile", "observed-representation", "residual"] {
        for e in manifest(c)["semantic_invalid"].as_array().unwrap() {
            let code = e["code"].as_str().unwrap();
            assert!(readme.contains(&format!("`{code}`")), "README lacks {code}");
        }
    }
}

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/check-contract-digests.sh")
}

#[test]
fn digests_manifest_matches_the_schemas() {
    let manifest = std::fs::read_to_string(root().join("DIGESTS")).unwrap();
    let mut listed = 0;
    for line in manifest.lines() {
        let (digest, name) = line.split_once("  ").unwrap();
        let bytes = std::fs::read(root().join(name)).unwrap();
        assert_eq!(hex::encode(Sha256::digest(&bytes)), digest, "{name}");
        listed += 1;
    }
    assert_eq!(listed, 3);
    let out = Command::new("sh").arg(script()).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn digest_check_fails_when_a_schema_drifts() {
    let tmp = tempfile::tempdir().unwrap();
    for f in std::fs::read_dir(root()).unwrap() {
        let f = f.unwrap();
        if f.file_type().unwrap().is_file() {
            std::fs::copy(f.path(), tmp.path().join(f.file_name())).unwrap();
        }
    }
    let target = tmp.path().join("residual.v1.json");
    let mut text = std::fs::read_to_string(&target).unwrap();
    text.push(' ');
    std::fs::write(&target, text).unwrap();
    let out = Command::new("sh")
        .arg(script())
        .env("SXR_CONTRACTS_DIR", tmp.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("DIGEST MISMATCH residual.v1.json"));
}
