//! End-to-end against the REAL Slice 0B binary, opt-in: set
//! `SXR_TEST_DOMAINFORGE_BIN=/path/to/domainforge` (e.g. the slice0b worktree
//! build) to exercise the full `resolve` path. The committed fixtures already
//! pin the binary's output contract; this test proves the process boundary
//! against a live binary when available — including CAS-carried
//! materialization for a representation > 64 KiB (the regression guard against
//! only ever exercising inline fixtures).

use serde_json::json;
use std::path::{Path, PathBuf};
use sxr_core::types::SemanticContext;
use sxr_df::adapter::{CliDomainForgeAdapter, SemanticContextRequest};

fn request(semantic_root: &Path, cas_root: Option<&Path>) -> SemanticContextRequest {
    SemanticContextRequest {
        semantic_root: semantic_root.to_path_buf(),
        entry_logical_path: "examples/namespaces/logistics/core.sea".into(),
        registry_path: None,
        default_namespace: None,
        semantic_packs: Vec::new(),
        cep_scope: json!({ "repo_id": "01E2E" }),
        cas_root: cas_root.map(Path::to_path_buf),
    }
}

#[test]
#[ignore = "requires SXR_TEST_DOMAINFORGE_BIN pointing at a domainforge binary"]
fn real_domainforge_resolves_the_dogfood_entry() {
    let binary = std::env::var("SXR_TEST_DOMAINFORGE_BIN").expect("SXR_TEST_DOMAINFORGE_BIN set");
    let root = std::env::var("SXR_TEST_DOMAINFORGE_ROOT")
        .expect("SXR_TEST_DOMAINFORGE_ROOT set (slice0b worktree)");
    let adapter = CliDomainForgeAdapter::new(PathBuf::from(&binary));
    let ctx = adapter
        .resolve(&request(Path::new(&root), None))
        .expect("real binary resolves");
    // The dogfood entry is an invalid-model-with-D shape → Resolved + diagnostics.
    let SemanticContext::Resolved {
        content_hash,
        semantic_closure_hash,
        declarations,
        diagnostics,
        ..
    } = &ctx
    else {
        panic!("dogfood entry must resolve with D, got {ctx:?}");
    };
    assert!(content_hash.starts_with("sha256:"));
    assert!(semantic_closure_hash.starts_with("sha256:"));
    assert!(!declarations.is_empty());
    assert!(!diagnostics.is_empty());
}

/// A representation larger than 64 KiB: with the default inline threshold the
/// producer emits a `content_ref` (CAS carriage). The adapter must materialize
/// D through the snapshot boundary, persist it into the CAS store, and return
/// a `Resolved` context byte-identical in semantics to inline carriage.
#[test]
#[ignore = "requires SXR_TEST_DOMAINFORGE_BIN + a .sea entry whose D exceeds 64 KiB"]
fn real_domainforge_materializes_large_cas_carried_representation() {
    let binary = std::env::var("SXR_TEST_DOMAINFORGE_BIN").expect("SXR_TEST_DOMAINFORGE_BIN set");
    let entry_dir = std::env::var("SXR_TEST_LARGE_SEA_DIR")
        .expect("SXR_TEST_LARGE_SEA_DIR set (dir containing interaction-model.sea)");
    let entry = Path::new(&entry_dir).join("interaction-model.sea");
    let adapter = CliDomainForgeAdapter::new(PathBuf::from(&binary));
    let cas = tempfile::tempdir().unwrap();
    let request = SemanticContextRequest {
        semantic_root: entry_dir.clone().into(),
        entry_logical_path: "interaction-model.sea".into(),
        registry_path: None,
        default_namespace: None,
        semantic_packs: Vec::new(),
        cep_scope: json!({}),
        cas_root: Some(cas.path().to_path_buf()),
    };
    let ctx = adapter.resolve(&request).expect("large model resolves");
    let SemanticContext::Resolved {
        content_hash,
        representation_json,
        declarations,
        ..
    } = ctx
    else {
        panic!("large model must resolve with materialized D, got {ctx:?}");
    };
    assert!(
        representation_json.len() > 65_536,
        "test precondition: D must exceed the 64 KiB inline threshold, got {} bytes",
        representation_json.len()
    );
    assert!(!declarations.is_empty());
    // The CAS object must exist, content-addressed by content_hash.
    let hex = content_hash.strip_prefix("sha256:").unwrap();
    let cas_path = cas.path().join(hex);
    assert!(
        cas_path.exists(),
        "materialized D must be persisted into the CAS store at {cas_path:?}"
    );
    // A second resolve must succeed from the CAS store with identical D.
    let again = adapter.resolve(&request).expect("re-resolve from CAS");
    let SemanticContext::Resolved {
        representation_json: again_json,
        ..
    } = again
    else {
        panic!("re-resolve must resolve");
    };
    assert_eq!(
        again_json, representation_json,
        "carriage must not change D"
    );
    let _ = entry;
}
