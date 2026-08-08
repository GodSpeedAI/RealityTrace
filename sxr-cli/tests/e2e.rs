//! Minimal e2e smoke test: verify the sxr binary can enroll, doctor, and list.
//! The full lifecycle is exercised by the proof script `/tmp/sxr-proof2.sh`.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sxr"))
}

fn run(args: &[&str], xdg_data: &str, xdg_state: &str) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("XDG_DATA_HOME", xdg_data)
        .env("XDG_STATE_HOME", xdg_state);
    cmd.output().expect("spawn sxr")
}

#[test]
fn sxr_binary_is_present() {
    assert!(bin().is_file(), "sxr binary at {:?}", bin());
}

#[test]
fn doctor_returns_valid_json() {
    let d = run(
        &["doctor", "--json"],
        "/tmp/sxr-test-data",
        "/tmp/sxr-test-state",
    );
    assert!(
        d.status.success(),
        "doctor: {:?}",
        String::from_utf8_lossy(&d.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&d.stdout).unwrap();
    assert_eq!(v["sxr_output_version"], "1");
    assert!(v["filesystem_class"].as_str().is_some());
}

#[test]
fn repo_list_after_enroll() {
    let data = format!("/tmp/sxr-test-{}", std::process::id());
    let state = format!("{}-st", data);
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let repo = PathBuf::from(&data).join("target");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"x\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn a() -> u32 { 42 }\n").unwrap();
    let _ = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&repo)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-qm", "init"])
        .current_dir(&repo)
        .status();

    let r = run(
        &["repo", "enroll", repo.to_str().unwrap(), "--json"],
        &data,
        &state,
    );
    assert!(
        r.status.success(),
        "enroll: {:?}",
        String::from_utf8_lossy(&r.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&r.stdout).unwrap();
    assert!(v["repo_id"].as_str().unwrap().len() > 10);

    let l = run(&["repo", "list", "--json"], &data, &state);
    let lv: serde_json::Value = serde_json::from_slice(&l.stdout).unwrap();
    assert_eq!(lv["repos"].as_array().unwrap().len(), 1);
}
