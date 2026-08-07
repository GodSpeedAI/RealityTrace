//! End-to-end against the REAL Slice 0B binary, opt-in: set
//! `SXR_TEST_DOMAINFORGE_BIN=/path/to/domainforge` (e.g. the slice0b worktree
//! build) to exercise the full `resolve` path. The committed fixtures already
//! pin the binary's output contract; this test proves the process boundary
//! against a live binary when available.

use serde_json::json;
use std::path::{Path, PathBuf};
use sxr_core::types::SemanticContext;
use sxr_df::adapter::{CliDomainForgeAdapter, SemanticContextRequest};

fn request(semantic_root: &Path) -> SemanticContextRequest {
    SemanticContextRequest {
        semantic_root: semantic_root.to_path_buf(),
        entry_logical_path: "examples/namespaces/logistics/core.sea".into(),
        registry_path: None,
        default_namespace: None,
        semantic_packs: Vec::new(),
        cep_scope: json!({ "repo_id": "01E2E" }),
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
        .resolve(&request(Path::new(&root)))
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
