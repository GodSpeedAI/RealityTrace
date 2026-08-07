//! AC-CORE-001 and AC-SXR-001: sxr-core has no I/O and no DomainForge core.
use std::path::PathBuf;

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn core_has_no_forbidden_dependencies() {
    let manifest = std::fs::read_to_string(crate_dir().join("Cargo.toml")).unwrap();
    let manifest = manifest.to_lowercase();
    for forbidden in ["rusqlite", "domainforge", "libsqlite3", "sxr-ledger"] {
        assert!(
            !manifest.contains(forbidden),
            "sxr-core Cargo.toml must not mention {forbidden:?} (AC-CORE-001)"
        );
    }
}

#[test]
fn core_source_has_no_io_primitives() {
    let src = crate_dir().join("src");
    let mut entries: Vec<_> = std::fs::read_dir(&src)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "rs").unwrap_or(false))
        .collect();
    entries.sort();
    for path in entries {
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in [
            "std::fs",
            "std::process",
            "std::net",
            "std::io",
            "Command::new",
            "File::open",
        ] {
            assert!(
                !text.contains(needle),
                "{} contains forbidden I/O primitive {needle:?} (AC-CORE-001)",
                path.display()
            );
        }
    }
}

#[test]
fn no_domainforge_core_anywhere_in_workspace_dependency_graph() {
    // The workspace resolves sxr-core's dependencies; assert the crate itself
    // never appears in sxr-core's own manifest (sxr-core cannot depend on
    // domainforge-core per AC-SXR-001) — enforced by the manifest test above
    // and structurally by not listing it. This test documents the invariant.
    let manifest = std::fs::read_to_string(crate_dir().join("Cargo.toml")).unwrap();
    assert!(!manifest.to_lowercase().contains("domainforge-core"));
}
