//! Process-boundary tests with a scripted fake `domainforge` binary: argv
//! construction, exit-code handling, and stderr/empty-stdout paths. The
//! envelope payloads are the committed fixtures captured from the real
//! Slice 0B binary.

use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use sxr_core::types::SemanticContext;
use sxr_df::adapter::{CliDomainForgeAdapter, SemanticContextRequest};
use tempfile::tempdir;

const FIXTURES: &str = env!("CARGO_MANIFEST_DIR");

fn fixture(name: &str) -> String {
    fs::read_to_string(PathBuf::from(FIXTURES).join("tests/fixtures").join(name))
        .expect("fixture exists")
}

/// Write an executable shell script and return its path.
fn scripted_binary(dir: &std::path::Path, body: &str) -> PathBuf {
    let path = dir.join("fake-domainforge");
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn request(semantic_root: &std::path::Path) -> SemanticContextRequest {
    SemanticContextRequest {
        semantic_root: semantic_root.to_path_buf(),
        entry_logical_path: "fixtures/envelope/valid-basic.sea".into(),
        registry_path: None,
        default_namespace: Some("auth".into()),
        semantic_packs: vec![("logistics".into(), "sha256:aa".into())],
        cep_scope: json!({ "repo_id": "01TEST" }),
        cas_root: None,
    }
}

#[test]
fn argv_construction_and_resolved_passthrough() {
    let dir = tempdir().unwrap();
    let argv_log = dir.path().join("argv.log");
    let body = format!(
        r#"#!/bin/sh
printf '%s\n' "$@" > "{}"
cat "{}"
exit 0
"#,
        argv_log.display(),
        PathBuf::from(FIXTURES)
            .join("tests/fixtures/env-valid-basic.json")
            .display(),
    );
    let binary = scripted_binary(dir.path(), &body);
    let adapter = CliDomainForgeAdapter::new(binary);
    let ctx = adapter.resolve(&request(dir.path())).unwrap();
    assert!(matches!(ctx, SemanticContext::Resolved { .. }));

    let argv = fs::read_to_string(&argv_log).unwrap();
    let args: Vec<&str> = argv.lines().collect();
    assert_eq!(args[0], "envelope");
    assert_eq!(args[1], "--emit");
    assert_eq!(args[2], "both");
    assert_eq!(args[3], "--pack");
    assert_eq!(args[4], "logistics=sha256:aa");
    assert_eq!(args[5], "--default-namespace");
    assert_eq!(args[6], "auth");
    assert_eq!(args[7], "--scope");
    let scope: serde_json::Value = serde_json::from_str(args[8]).unwrap();
    assert_eq!(scope["repo_id"], "01TEST");
    assert_eq!(args[9], "fixtures/envelope/valid-basic.sea");
}

#[test]
fn exit_one_with_failure_envelope_maps_to_validation_failed() {
    let dir = tempdir().unwrap();
    let body = format!(
        r#"#!/bin/sh
cat "{}"
exit 1
"#,
        PathBuf::from(FIXTURES)
            .join("tests/fixtures/env-invalid-no-d.json")
            .display(),
    );
    let binary = scripted_binary(dir.path(), &body);
    let adapter = CliDomainForgeAdapter::new(binary);
    let ctx = adapter.resolve(&request(dir.path())).unwrap();
    assert!(matches!(ctx, SemanticContext::ValidationFailed { .. }));
}

#[test]
fn exit_one_with_d_constructed_still_resolves() {
    let dir = tempdir().unwrap();
    let body = format!(
        r#"#!/bin/sh
cat "{}"
exit 1
"#,
        PathBuf::from(FIXTURES)
            .join("tests/fixtures/env-invalid-with-d.json")
            .display(),
    );
    let binary = scripted_binary(dir.path(), &body);
    let adapter = CliDomainForgeAdapter::new(binary);
    let ctx = adapter.resolve(&request(dir.path())).unwrap();
    assert!(matches!(ctx, SemanticContext::Resolved { .. }));
}

#[test]
fn unexpected_exit_code_is_an_error() {
    let dir = tempdir().unwrap();
    let body = format!(
        r#"#!/bin/sh
cat "{}"
echo "boom" >&2
exit 2
"#,
        PathBuf::from(FIXTURES)
            .join("tests/fixtures/env-valid-basic.json")
            .display(),
    );
    let binary = scripted_binary(dir.path(), &body);
    let adapter = CliDomainForgeAdapter::new(binary);
    let err = adapter.resolve(&request(dir.path())).unwrap_err();
    assert!(
        err.to_string().contains("unexpected code Some(2)"),
        "got {err}"
    );
    assert!(err.to_string().contains("boom"), "got {err}");
}

#[test]
fn empty_stdout_is_malformed_even_on_exit_one() {
    let dir = tempdir().unwrap();
    let binary = scripted_binary(
        dir.path(),
        "#!/bin/sh\nexit 1\n".replace("\\n", "\n").as_str(),
    );
    let adapter = CliDomainForgeAdapter::new(binary);
    let err = adapter.resolve(&request(dir.path())).unwrap_err();
    assert!(err.to_string().contains("empty stdout"), "got {err}");
}

#[test]
fn schema_violation_through_the_process_boundary() {
    let dir = tempdir().unwrap();
    let mut envelope: serde_json::Value =
        serde_json::from_str(&fixture("env-valid-basic.json")).unwrap();
    envelope.as_object_mut().unwrap().remove("boundary_record");
    let output_path = dir.path().join("bad-envelope.json");
    fs::write(&output_path, serde_json::to_string(&envelope).unwrap()).unwrap();
    let body = format!(
        r#"#!/bin/sh
cat "{}"
exit 0
"#,
        output_path.display()
    );
    let binary = scripted_binary(dir.path(), &body);
    let adapter = CliDomainForgeAdapter::new(binary);
    let err = adapter.resolve(&request(dir.path())).unwrap_err();
    assert!(err.to_string().contains("schema"), "got {err}");
}

#[test]
fn missing_binary_is_reported() {
    let dir = tempdir().unwrap();
    let adapter = CliDomainForgeAdapter::new("/definitely/not/a/real/binary");
    let err = adapter.resolve(&request(dir.path())).unwrap_err();
    assert!(err.to_string().contains("unavailable"), "got {err}");
}

fn versioned_binary(dir: &std::path::Path, reported: &str) -> (PathBuf, PathBuf) {
    let ran_envelope = dir.join("envelope-ran");
    let body = format!(
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "domainforge {reported}"; exit 0; fi
touch "{}"
cat "{}"
exit 0
"#,
        ran_envelope.display(),
        PathBuf::from(FIXTURES)
            .join("tests/fixtures/env-valid-basic.json")
            .display(),
    );
    (scripted_binary(dir, &body), ran_envelope)
}

#[test]
fn required_version_mismatch_fails_closed_before_envelope_runs() {
    let dir = tempdir().unwrap();
    let (binary, ran_envelope) = versioned_binary(dir.path(), "0.18.1");
    let adapter = CliDomainForgeAdapter::new(binary).with_required_version("0.18.2");
    let err = adapter.resolve(&request(dir.path())).unwrap_err();
    match err {
        sxr_df::SemanticError::UnsupportedVersion { expected, found } => {
            assert_eq!(expected, "0.18.2");
            assert_eq!(found, "0.18.1");
        }
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    }
    assert!(
        !ran_envelope.exists(),
        "envelope must not run on a version mismatch"
    );
}

#[test]
fn required_version_match_resolves() {
    let dir = tempdir().unwrap();
    let (binary, ran_envelope) = versioned_binary(dir.path(), "0.18.2");
    let adapter = CliDomainForgeAdapter::new(binary).with_required_version("0.18.2");
    adapter.resolve(&request(dir.path())).unwrap();
    assert!(ran_envelope.exists());
}

#[test]
fn unparseable_version_output_fails_closed() {
    let dir = tempdir().unwrap();
    let body = "#!/bin/sh\necho 'who knows'\nexit 0\n";
    let binary = scripted_binary(dir.path(), body);
    let adapter = CliDomainForgeAdapter::new(binary).with_required_version("0.18.2");
    assert!(matches!(
        adapter.resolve(&request(dir.path())).unwrap_err(),
        sxr_df::SemanticError::UnsupportedVersion { .. }
    ));
}
