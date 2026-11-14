//! WP-18 CLI: `observed propose | regression | capability-delta`. Pure
//! JSON-in, JSON-out commands over compare result documents. The semantics
//! are tested over real compare results in `sxr-observed/tests/wp18.rs`; this
//! file checks the command line: files, exit codes, determinism, refusals.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use sxr_core::canonical_json::sha256_canonical;

const V1: &str =
    "world:sop@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const V2: &str =
    "world:sop@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const DIFF: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

fn run(args: &[&str]) -> Output {
    Command::new(PathBuf::from(env!("CARGO_BIN_EXE_realitytrace")))
        .args(args)
        .env_remove("SXR_DAEMON_SOCKET")
        .output()
        .expect("run realitytrace")
}

fn text(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// A compare result with one structural residual per `(expectation, class)`.
fn compare_doc(item: &str, world: &str, elements: &[(&str, &str)]) -> Value {
    let ep = format!("episode:case1:{item}:1@sha256:{}", "1".repeat(64));
    let mut residuals = vec![];
    let mut annotations = serde_json::Map::new();
    for (i, (exp, class)) in elements.iter().enumerate() {
        let rid = format!(
            "residual:{item}-{world_tag}-{i}",
            world_tag = &world[world.len() - 4..]
        );
        residuals.push(json!({"residual_id": rid, "class": class,
            "occurrence_ids": [format!("sea-forge:{item}-r{i}")]}));
        annotations.insert(rid, json!({"expectation": exp}));
    }
    json!({"comparison_id": format!("comparison:{item}{}", &world[world.len() - 4..]),
           "episode_ref": ep, "world_ref": world,
           "profile": {"id": "profile:s1", "sha256": "sha256:p"},
           "silent_evidence_sources": [],
           "residuals": {"episode": [], "structural": residuals},
           "annotations": annotations})
}

fn put(dir: &Path, name: &str, v: &Value) -> String {
    let p = dir.join(name);
    std::fs::write(&p, serde_json::to_vec(v).unwrap()).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn propose_regression_and_delta_run_from_files_with_the_documented_exit_codes() {
    let d = tempfile::tempdir().unwrap();
    let dir = d.path();
    let a = compare_doc(
        "stepA",
        V1,
        &[
            ("flow", "confirmed"),
            ("undeclared:u", "observed_undeclared"),
        ],
    );
    let b = compare_doc(
        "stepB",
        V1,
        &[
            ("flow", "confirmed"),
            ("undeclared:u", "observed_undeclared"),
        ],
    );
    let (fa, fb) = (put(dir, "a.json", &a), put(dir, "b.json", &b));
    let rule = json!({"contract": "heldout-rule.v1", "rule_id": "rule:r", "kind": "exclude_episodes",
                      "held_out_episodes": ["episode:case1:stepB:1"], "statement": "by episode"});
    let frule = put(dir, "rule.json", &rule);
    let digest = sha256_canonical(&rule);
    let res_a = a["residuals"]["structural"][1]["residual_id"]
        .as_str()
        .unwrap();
    let res_b = b["residuals"]["structural"][1]["residual_id"]
        .as_str()
        .unwrap();
    let intended = json!([{"element": "flow-new", "change": "add_declaration", "supersedes": ["undeclared:u"]}]).to_string();
    let propose = |residual: &str, sha: &str, out: &str| {
        run(&[
            "observed",
            "propose",
            "--proposer",
            "actor:p",
            "--scope",
            "case:case1",
            "--old-world",
            V1,
            "--candidate-world",
            V2,
            "--rule",
            &frule,
            "--rule-sha256",
            sha,
            "--compared",
            &fa,
            "--compared",
            &fb,
            "--residual",
            residual,
            "--intended",
            &intended,
            "--semantic-diff-sha256",
            DIFF,
            "--out",
            out,
        ])
    };

    // The held-out episode cannot be cited; a rule that is not the frozen one is refused.
    let out = dir.join("proposal.json").to_string_lossy().into_owned();
    let bad = propose(res_b, &digest, &out);
    assert_eq!(bad.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad.stderr).contains("HELDOUT_CONTAMINATED"));
    let bad = propose(res_a, "sha256:00", &out);
    assert!(String::from_utf8_lossy(&bad.stderr).contains("HELDOUT_RULE_DIGEST_MISMATCH"));
    assert!(
        !Path::new(&out).exists(),
        "a refused proposal writes nothing"
    );

    let ok = propose(res_a, &digest, &out);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let first = std::fs::read_to_string(&out).unwrap();
    propose(res_a, &digest, &out);
    assert_eq!(
        first,
        std::fs::read_to_string(&out).unwrap(),
        "deterministic"
    );

    // Candidate world keeps everything and declares the step.
    let na = compare_doc(
        "stepA",
        V2,
        &[("flow", "confirmed"), ("flow-new", "confirmed")],
    );
    let nb = compare_doc(
        "stepB",
        V2,
        &[("flow", "confirmed"), ("flow-new", "confirmed")],
    );
    let (fna, fnb) = (put(dir, "na.json", &na), put(dir, "nb.json", &nb));
    let pass_out = dir.join("m-pass.json").to_string_lossy().into_owned();
    let m = run(&[
        "observed",
        "regression",
        "--proposal",
        &out,
        "--old",
        &fa,
        "--old",
        &fb,
        "--new",
        &fna,
        "--new",
        &fnb,
        "--out",
        &pass_out,
    ]);
    assert!(
        m.status.success(),
        "{}{}",
        text(&m),
        String::from_utf8_lossy(&m.stderr)
    );
    let matrix: Value = serde_json::from_slice(&std::fs::read(&pass_out).unwrap()).unwrap();
    assert_eq!(matrix["verdict"], json!("pass"));
    assert_eq!(matrix["summary"]["gains"], json!(2));

    // A candidate that breaks a confirmed element: matrix printed, exit 6.
    let ba = compare_doc(
        "stepA",
        V2,
        &[("flow", "contradicted"), ("flow-new", "confirmed")],
    );
    let bb = compare_doc(
        "stepB",
        V2,
        &[("flow", "confirmed"), ("flow-new", "confirmed")],
    );
    let (fba, fbb) = (put(dir, "ba.json", &ba), put(dir, "bb.json", &bb));
    let m = run(&[
        "observed",
        "regression",
        "--proposal",
        &out,
        "--old",
        &fa,
        "--old",
        &fb,
        "--new",
        &fba,
        "--new",
        &fbb,
    ]);
    assert_eq!(m.status.code(), Some(6));
    let printed: Value = serde_json::from_str(text(&m).trim()).unwrap();
    assert_eq!(printed["refusal_code"], json!("REGRESSION_FAILED"));

    // AT-13: GAIN and NO_GAIN both exit 0; missing control or unfrozen definition is refused.
    let frozen = json!({"contract": "capability-frozen.v1", "frozen_id": "frozen:f",
        "episode_key": "episode:case1:stepS:1", "comparison": "structural",
        "old_expectation": "undeclared:u", "new_expectation": "flow-new", "prediction": "GAIN",
        "falsifier": ["old_world_confirmed", "new_world_not_confirmed", "control_shows_gain"]});
    let (ff, fd) = (put(dir, "frozen.json", &frozen), sha256_canonical(&frozen));
    let s_old = put(
        dir,
        "s-old.json",
        &compare_doc("stepS", V1, &[("undeclared:u", "observed_undeclared")]),
    );
    let s_new = put(
        dir,
        "s-new.json",
        &compare_doc("stepS", V2, &[("flow-new", "confirmed")]),
    );
    let c_old = put(
        dir,
        "c-old.json",
        &compare_doc("stepSctl", V1, &[("flow", "confirmed")]),
    );
    let c_new = put(
        dir,
        "c-new.json",
        &compare_doc("stepSctl", V2, &[("flow-new", "insufficient")]),
    );
    let delta = |sha: Option<&str>, control: bool| {
        let mut args = vec![
            "observed",
            "capability-delta",
            "--frozen",
            &ff,
            "--old",
            &s_old,
            "--new",
            &s_new,
        ];
        if let Some(s) = sha {
            args.extend(["--frozen-sha256", s]);
        }
        if control {
            args.extend(["--control-old", &c_old, "--control-new", &c_new]);
        }
        run(&args)
    };
    let g = delta(Some(&fd), true);
    assert!(g.status.success(), "{}", String::from_utf8_lossy(&g.stderr));
    let g: Value = serde_json::from_str(text(&g).trim()).unwrap();
    assert_eq!(g["result"], json!("GAIN"));
    let n = delta(Some(&fd), false);
    assert_eq!(n.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&n.stderr).contains("CONTROL_VARIANT_MISSING"));
    let n = delta(None, true);
    assert!(String::from_utf8_lossy(&n.stderr).contains("FALSIFIER_NOT_FROZEN"));
}
