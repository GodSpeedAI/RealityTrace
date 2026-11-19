//! Ensure Chrono does not return through a normal, build, or development edge.
#[test]
fn sxr_core_dependency_tree_has_no_chrono() {
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--offline",
            "-p",
            "sxr-core",
            "-e",
            "normal,build,dev",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .expect("run cargo tree");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).expect("cargo tree output is UTF-8");
    assert!(
        !tree.lines().any(|line| line.starts_with("chrono v")),
        "Chrono is present in sxr-core's dependency tree:\n{tree}"
    );
}
