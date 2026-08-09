#!/usr/bin/env bash
# Slice 7 corpus scenario library. Each scenario: a deterministic fault
# injected into a real §26 repo's working tree, its verifier invocation, and
# the failure signature that MUST appear for the case to be recorded. The
# true failure locus is a construction record (scenario-keys.json), sealed
# separately; this file never reads keys.
set -euo pipefail

# ---- helpers ---------------------------------------------------------------

record_created() { echo "$1" >> "$2/injected-created.txt"; }

record_chmod() { printf '%s %s\n' "$1" "$2" >> "$3/injected-chmod.txt"; }

# run_verifier <env-assignments...> -- <cmd...> > out 2>&1 ; prints exit code
# Env assignments are applied to the verifier process only.
run_env_verifier() {
  local outfile=$1; shift
  local envs=()
  while [ "$1" != "--" ]; do envs+=("$1"); shift; done
  shift
  env "${envs[@]}" "$@" > "$outfile" 2>&1
  return $?
}

# ---- scenario dispatch: inject <repo> <n> <repo-path> <backup-dir> ---------

scenario_inject() {
  local repo=$1 n=$2 path=$3 bk=$4
  case "$repo-$n" in
  # ══ cep (pytest) ═════════════════════════════════════════════════════════
  cep-1|cep-2|cep-3)
    # verifier: wrong assertion / wrong expectation appended as a test.
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_assertion_bug():
    assert 1 + 1 == 3
EOF
    ;;
  cep-4)
    # plan: step references a test file that no longer exists.
    git -C "$path" mv tests/test_import.py tests/test_import_renamed.py
    ;;
  cep-5)
    git -C "$path" mv tests/test_inventory.py tests/test_inventory_renamed.py
    ;;
  cep-6)
    # implementation: repo-internal module renamed (import fails).
    git -C "$path" mv src/cep/scoring src/cep/scoring_corpus_moved
    find "$path/src/cep" -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_missing_module():
    import cep.scoring  # noqa: F401
EOF
    ;;
  cep-7)
    # implementation: repo-internal symbol renamed (import fails).
    sed -i 's/def iter_reality_gap_case_files/def iter_reality_gap_case_files_corpus_moved/' "$path/src/cep/inventory.py"
    find "$path/src/cep" -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_missing_symbol():
    from cep.inventory import iter_reality_gap_case_files  # noqa: F401
EOF
    ;;
  cep-8)
    # implementation: repo-internal module renamed (import fails).
    git -C "$path" mv src/cep/conformance/anti_collapse.py src/cep/conformance/anti_collapse_moved.py
    find "$path/src/cep" -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_missing_anti_collapse():
    from cep.conformance.anti_collapse import check_fixture  # noqa: F401
EOF
    ;;
  cep-9)
    # environment: temp dir unavailable (TMPDIR points to an unwritable dir).
    mkdir -p "$path/.corpus-tmpfix"
    record_created ".corpus-tmpfix" "$bk"
    record_chmod "$(stat -c %a "$path/.corpus-tmpfix")" ".corpus-tmpfix" "$bk"
    chmod 555 "$path/.corpus-tmpfix"
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_tmpdir_unavailable():
    import os
    td = os.environ.get("TMPDIR") or os.path.dirname(os.path.dirname(__file__))
    assert os.access(td, os.W_OK), "TMPDIR is not writable"
EOF
    ;;
  cep-10)
    # environment: cache dir unavailable.
    : # env applied by verifier
    ;;
  cep-11)
    # authority: fixture unreadable.
    record_chmod "$(stat -c %a "$path/tests/fixtures/sample_settlement_recorded.json")" "tests/fixtures/sample_settlement_recorded.json" "$bk"
    chmod 000 "$path/tests/fixtures/sample_settlement_recorded.json"
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_unreadable_fixture():
    p = "tests/fixtures/sample_settlement_recorded.json"
    with open(p) as f:
        assert f.read()
EOF
    ;;
  cep-12)
    # authority: fixture directory unreadable.
    record_chmod "$(stat -c %a "$path/tests/fixtures")" "tests/fixtures" "$bk"
    chmod 000 "$path/tests/fixtures"
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_unreadable_fixture_dir():
    import os
    assert os.listdir("tests/fixtures")
EOF
    ;;
  cep-13)
    # evidence_collection: test references a missing (stale) fixture.
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_missing_fixture():
    import os
    assert os.path.exists("tests/fixtures/inventory-version-v999.json"), "stale fixture missing"
EOF
    ;;
  cep-14)
    # evidence_collection: fixture content is stale.
    python3 - "$path/tests/fixtures/sample_settlement_recorded.json" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
if isinstance(d, dict) and "settlement_status" in d:
    d["settlement_status"] = "stale"
    json.dump(d, open(p, "w"), indent=2)
else:
    open(p, "a").write("\n# stale\n")
PY
    cat >> "$path/tests/test_import.py" <<'EOF'


def test_corpus_scenario_test_stale_fixture():
    import json
    d = json.load(open("tests/fixtures/sample_settlement_recorded.json"))
    assert d.get("settlement_status") == "settled"
EOF
    ;;
  cep-15)
    # unknown: shared-state ordering dependence (deliberately ambiguous).
    cat >> "$path/tests/test_import.py" <<'EOF'


_CORPUS_ORDER_FLAG = False

def test_corpus_scenario_order_a():
    global _CORPUS_ORDER_FLAG
    _CORPUS_ORDER_FLAG = True

def test_corpus_scenario_order_b():
    assert _CORPUS_ORDER_FLAG is False
EOF
    ;;

  # ══ ast-grep-mcp-rs (cargo test) ═════════════════════════════════════════
  ag-1)
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_assertion_bug() {
    assert_eq!(1 + 1, 3);
}
EOF
    ;;
  ag-2)
    cat >> "$path/tests/integration_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_assertion_bug_integration() {
    assert_eq!(2 + 2, 5);
}
EOF
    ;;
  ag-3)
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_wrong_expectation() {
    assert!("wrong expectation".contains("never"), "wrong expectation");
}
EOF
    ;;
  ag-4)
    git -C "$path" mv tests/unit_tests.rs tests/unit_tests_moved.rs
    ;;
  ag-5)
    git -C "$path" mv tests/integration_tests.rs tests/integration_tests_moved.rs
    ;;
  ag-6|ag-7)
    # implementation: pub fn used by tests is missing/renamed.
    local F=src/format.rs
    sed -i 's/pub fn format_matches_as_text/pub fn format_matches_as_text_corpus_moved/' "$path/$F"
    ;;
  ag-8)
    local F2=src/config.rs
    sed -i 's/pub fn from_cli/pub fn from_cli_corpus_moved/' "$path/$F2"
    ;;
  ag-9)
    # environment: AST_GREP_CONFIG points nowhere (env applied by verifier).
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_config_env() {
    let p = std::env::var("AST_GREP_CONFIG").unwrap_or_default();
    assert!(p.is_empty() || std::path::Path::new(&p).exists(), "AST_GREP_CONFIG points nowhere");
}
EOF
    ;;
  ag-10)
    : # environment: toolchain unavailable (PATH without cargo)
    ;;
  ag-11)
    record_chmod "$(stat -c %a "$path/tests/fixtures/example.py")" "tests/fixtures/example.py" "$bk"
    chmod 000 "$path/tests/fixtures/example.py"
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_unreadable_fixture() {
    std::fs::read("tests/fixtures/example.py").unwrap();
}
EOF
    ;;
  ag-12)
    record_chmod "$(stat -c %a "$path/src/lib.rs")" "src/lib.rs" "$bk"
    chmod 000 "$path/src/lib.rs"
    touch "$path/src/lib.rs"
    ;;
  ag-13)
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_missing_fixture() {
    assert!(std::path::Path::new("tests/fixtures/example-v999.py").exists(), "stale fixture missing");
}
EOF
    ;;
  ag-14)
    python3 - "$path/tests/fixtures/example.py" <<'PY'
import sys
p = sys.argv[1]
open(p, "a").write("\n# stale corpus marker\n")
PY
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_stale_fixture() {
    let s = std::fs::read_to_string("tests/fixtures/example.py").unwrap();
    assert!(!s.contains("stale corpus marker"), "fixture is stale");
}
EOF
    ;;
  ag-15)
    cat >> "$path/tests/unit_tests.rs" <<'EOF'

static CORPUS_ORDER_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn test_corpus_scenario_order_a() {
    CORPUS_ORDER_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn test_corpus_scenario_order_b() {
    assert!(!CORPUS_ORDER_FLAG.load(std::sync::atomic::Ordering::SeqCst));
}
EOF
    ;;

  # ══ sea-rs (cargo test, workspace) ═══════════════════════════════════════
  sr-1)
    cat >> "$path/crates/sea-forge-core/tests/corpus_sr1.rs" <<'EOF'
#[test]
fn test_corpus_scenario_assertion_bug() {
    assert_eq!(1 + 1, 3);
}
EOF
    record_created "crates/sea-forge-core/tests/corpus_sr1.rs" "$bk"
    ;;
  sr-2)
    mkdir -p "$path/crates/sea-forge-domain/tests"
    cat >> "$path/crates/sea-forge-domain/tests/corpus_sr2.rs" <<'EOF'
#[test]
fn test_corpus_scenario_assertion_bug() {
    assert_eq!(2 + 2, 5);
}
EOF
    record_created "crates/sea-forge-domain/tests/corpus_sr2.rs" "$bk"
    ;;
  sr-3)
    # plan: verification step references a test target absent from the tree.
    : # no file injection; the stale target is the fault
    ;;
  sr-4)
    : # plan: stale test target (see sr-3)
    ;;
  sr-5)
    sed -i 's/pub fn run_id/pub fn run_id_corpus_moved/' "$path/crates/sea-forge-core/src/ids.rs"
    ;;
  sr-6)
    sed -i 's/pub fn interpret/pub fn interpret_corpus_moved/' "$path/crates/sea-forge-domain/src/lib.rs"
    mkdir -p "$path/crates/sea-forge-domain/tests"
    cat >> "$path/crates/sea-forge-domain/tests/corpus_sr6.rs" <<'EOF'
#[test]
fn test_corpus_scenario_calls_interpret() {
    let _ = sea_forge_domain::interpret("corpus");
}
EOF
    record_created "crates/sea-forge-domain/tests/corpus_sr6.rs" "$bk"
    ;;
  sr-7)
    : # environment: toolchain unavailable
    ;;
  sr-8)
    record_chmod "$(stat -c %a "$path/crates/sea-forge-core/src/lib.rs")" "crates/sea-forge-core/src/lib.rs" "$bk"
    chmod 000 "$path/crates/sea-forge-core/src/lib.rs"
    touch "$path/crates/sea-forge-core/src/lib.rs"
    ;;
  sr-9)
    cat >> "$path/crates/sea-forge-core/tests/corpus_sr9.rs" <<'EOF'
#[test]
fn test_corpus_scenario_missing_fixture() {
    assert!(std::path::Path::new("data/corpus-v999.json").exists(), "stale fixture missing");
}
EOF
    record_created "crates/sea-forge-core/tests/corpus_sr9.rs" "$bk"
    ;;
  sr-10)
    mkdir -p "$path/crates/sea-forge-domain/tests"
    cat >> "$path/crates/sea-forge-domain/tests/corpus_sr10.rs" <<'EOF'
#[test]
fn test_corpus_scenario_stale_fixture() {
    assert!(std::path::Path::new("data/corpus-v998.json").exists(), "stale fixture missing");
}
EOF
    record_created "crates/sea-forge-domain/tests/corpus_sr10.rs" "$bk"
    ;;
  sr-11)
    sed -i 's/subject: "AgentSponsor"/subject: "NoSuchRoleForCorpus"/' "$path/.sea/interaction/interaction-model.sea"
    ;;
  sr-12)
    sed -i 's/^export Entity "UserIntent"/export Entity "UserIntentCorpusMoved"/' "$path/.sea/interaction/interaction-model.sea"
    ;;
  sr-13|sr-14|sr-15)
    cat >> "$path/crates/sea-forge-core/tests/corpus_sr_order.rs" <<'EOF'
static CORPUS_ORDER_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn test_corpus_scenario_order_a() {
    CORPUS_ORDER_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn test_corpus_scenario_order_b() {
    assert!(!CORPUS_ORDER_FLAG.load(std::sync::atomic::Ordering::SeqCst));
}
EOF
    record_created "crates/sea-forge-core/tests/corpus_sr_order.rs" "$bk"
    ;;

  # ══ domainforge (cargo test + domainforge validate/parse) ═════════════════
  df-1)
    cat >> "$path/domainforge-core/tests/flow_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_assertion_bug() {
    assert_eq!(1 + 1, 3);
}
EOF
    ;;
  df-2)
    cat >> "$path/domainforge-core/tests/entity_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_assertion_bug() {
    assert_eq!(2 + 2, 5);
}
EOF
    ;;
  df-3)
    git -C "$path" mv domainforge-core/tests/flow_tests.rs domainforge-core/tests/flow_tests_moved.rs
    ;;
  df-4)
    sed -i 's/pub fn new_with_namespace/pub fn new_with_namespace_corpus_moved/' "$path/domainforge-core/src/primitives/entity.rs"
    ;;
  df-5)
    sed -i 's/pub fn from_concept/pub fn from_concept_corpus_moved/' "$path/domainforge-core/src/concept_id.rs"
    ;;
  df-6)
    : # environment: toolchain unavailable
    ;;
  df-7)
    record_chmod "$(stat -c %a "$path/domainforge-core/src/lib.rs")" "domainforge-core/src/lib.rs" "$bk"
    chmod 000 "$path/domainforge-core/src/lib.rs"
    ;;
  df-8)
    cat >> "$path/domainforge-core/tests/flow_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_missing_fixture() {
    assert!(std::path::Path::new("data/corpus-v999.json").exists(), "stale fixture missing");
}
EOF
    ;;
  df-9)
    sed -i 's/from "Customer" to "Accounts Receivable"/from "NoSuchCustomerForCorpus" to "Accounts Receivable"/' "$path/examples/namespaces/finance/cashflow.sea"
    ;;
  df-10)
    sed -i 's/Entity "Customer"/Entity "CustomerCorpusMoved"/' "$path/examples/namespaces/finance/cashflow.sea"
    ;;
  df-11)
    python3 - "$path/.sea-registry.toml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace('patterns = ["examples/namespaces/finance/**/*.sea"]', 'patterns = [')
open(p, "w").write(s)
PY
    ;;
  df-12)
    python3 - "$path/.sea-registry.toml" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
s = s.replace('patterns = ["examples/namespaces/finance/**/*.sea"]', 'patterns = []')
open(p, "w").write(s)
PY
    ;;
  df-13)
    cat >> "$path/domainforge-core/tests/aggregation_tests.rs" <<'EOF'

static CORPUS_ORDER_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn test_corpus_scenario_order_a() {
    CORPUS_ORDER_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[test]
fn test_corpus_scenario_order_b() {
    assert!(!CORPUS_ORDER_FLAG.load(std::sync::atomic::Ordering::SeqCst));
}
EOF
    ;;
  df-14)
    cat >> "$path/domainforge-core/tests/aggregation_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_timing_sensitive() {
    let start = std::time::Instant::now();
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(start.elapsed() < std::time::Duration::from_millis(100), "timing assumption violated");
}
EOF
    ;;
  df-15)
    cat >> "$path/domainforge-core/tests/aggregation_tests.rs" <<'EOF'

#[test]
fn test_corpus_scenario_tmpdir_dependent() {
    let d = std::env::temp_dir().join("corpus-marker.txt");
    std::fs::write(&d, "x").unwrap();
    assert!(d.exists(), "tmpdir assumption violated");
}
EOF
    ;;

  *) echo "ERROR: unknown scenario $repo-$n"; exit 1 ;;
  esac
}

# ---- scenario mutation check ------------------------------------------------
# Prove the intended fault is ACTUALLY present in the working tree (or that the
# faulted environment precondition holds) BEFORE the verifier runs. Exit 0 =
# mutation applied. A scenario whose mutation is not present must never be
# recorded — this is the anti-false-positive invariant (a no-op sed, a wrong
# path, or a broken injection cannot produce a valid case).

scenario_mutation_check() {
  local repo=$1 n=$2 path=$3
  case "$repo-$n" in
  cep-1|cep-2|cep-3) grep -q "test_corpus_scenario_test_assertion_bug" "$path/tests/test_import.py" ;;
  cep-4) [ ! -e "$path/tests/test_import.py" ] && [ -e "$path/tests/test_import_renamed.py" ] ;;
  cep-5) [ ! -e "$path/tests/test_inventory.py" ] && [ -e "$path/tests/test_inventory_renamed.py" ] ;;
  cep-6) [ ! -d "$path/src/cep/scoring" ] && [ -d "$path/src/cep/scoring_corpus_moved" ] && grep -q "test_corpus_scenario_test_missing_module" "$path/tests/test_import.py" ;;
  cep-7) grep -q "iter_reality_gap_case_files_corpus_moved" "$path/src/cep/inventory.py" && grep -q "test_corpus_scenario_test_missing_symbol" "$path/tests/test_import.py" ;;
  cep-8) [ ! -e "$path/src/cep/conformance/anti_collapse.py" ] && grep -q "test_corpus_scenario_test_missing_anti_collapse" "$path/tests/test_import.py" ;;
  cep-9) [ -d "$path/.corpus-tmpfix" ] && [ "$(stat -c %a "$path/.corpus-tmpfix")" = "555" ] ;;
  cep-10) ! PATH=/usr/bin:/bin python3 -m pytest --version >/dev/null 2>&1 ;;
  cep-11) [ "$(stat -c %a "$path/tests/fixtures/sample_settlement_recorded.json")" = "000" ] && grep -q "test_corpus_scenario_test_unreadable_fixture" "$path/tests/test_import.py" ;;
  cep-12) [ "$(stat -c %a "$path/tests/fixtures")" = "000" ] && grep -q "test_corpus_scenario_test_unreadable_fixture_dir" "$path/tests/test_import.py" ;;
  cep-13) grep -q "test_corpus_scenario_test_missing_fixture" "$path/tests/test_import.py" ;;
  cep-14) grep -q "test_corpus_scenario_test_stale_fixture" "$path/tests/test_import.py" && grep -q "stale" "$path/tests/fixtures/sample_settlement_recorded.json" ;;
  cep-15) grep -q "test_corpus_scenario_order_b" "$path/tests/test_import.py" ;;
  ag-1|ag-3) grep -q "test_corpus_scenario_assertion_bug\|test_corpus_scenario_wrong_expectation" "$path/tests/unit_tests.rs" ;;
  ag-2) grep -q "test_corpus_scenario_assertion_bug_integration" "$path/tests/integration_tests.rs" ;;
  ag-4) [ ! -e "$path/tests/unit_tests.rs" ] && [ -e "$path/tests/unit_tests_moved.rs" ] ;;
  ag-5) [ ! -e "$path/tests/integration_tests.rs" ] && [ -e "$path/tests/integration_tests_moved.rs" ] ;;
  ag-6|ag-7) grep -q "format_matches_as_text_corpus_moved" "$path/src/format.rs" ;;
  ag-8) grep -q "from_cli_corpus_moved" "$path/src/config.rs" ;;
  ag-9) grep -q "test_corpus_scenario_config_env" "$path/tests/unit_tests.rs" ;;
  ag-10) ! PATH=/usr/bin:/bin bash -c 'command -v cargo' >/dev/null 2>&1 ;;
  ag-11) [ "$(stat -c %a "$path/tests/fixtures/example.py")" = "000" ] && grep -q "test_corpus_scenario_unreadable_fixture" "$path/tests/unit_tests.rs" ;;
  ag-12) [ "$(stat -c %a "$path/src/lib.rs")" = "000" ] ;;
  ag-13) grep -q "test_corpus_scenario_missing_fixture" "$path/tests/unit_tests.rs" ;;
  ag-14) grep -q "test_corpus_scenario_stale_fixture" "$path/tests/unit_tests.rs" && grep -q "stale corpus marker" "$path/tests/fixtures/example.py" ;;
  ag-15) grep -q "test_corpus_scenario_order_b" "$path/tests/unit_tests.rs" ;;
  sr-1) [ -e "$path/crates/sea-forge-core/tests/corpus_sr1.rs" ] && grep -q "test_corpus_scenario_assertion_bug" "$path/crates/sea-forge-core/tests/corpus_sr1.rs" ;;
  sr-2) [ -e "$path/crates/sea-forge-domain/tests/corpus_sr2.rs" ] && grep -q "test_corpus_scenario_assertion_bug" "$path/crates/sea-forge-domain/tests/corpus_sr2.rs" ;;
  sr-3) [ ! -e "$path/crates/sea-forge-core/tests/corpus_sr3_stale.rs" ] ;;
  sr-4) [ ! -e "$path/crates/sea-forge-domain/tests/corpus_sr4_stale.rs" ] ;;
  sr-5) grep -q "run_id_corpus_moved" "$path/crates/sea-forge-core/src/ids.rs" ;;
  sr-6) grep -q "interpret_corpus_moved" "$path/crates/sea-forge-domain/src/lib.rs" && grep -q "test_corpus_scenario_calls_interpret" "$path/crates/sea-forge-domain/tests/corpus_sr6.rs" ;;
  sr-7) ! PATH=/usr/bin:/bin bash -c 'command -v cargo' >/dev/null 2>&1 ;;
  sr-8) [ "$(stat -c %a "$path/crates/sea-forge-core/src/lib.rs")" = "000" ] ;;
  sr-9) grep -q "test_corpus_scenario_missing_fixture" "$path/crates/sea-forge-core/tests/corpus_sr9.rs" ;;
  sr-10) grep -q "test_corpus_scenario_stale_fixture" "$path/crates/sea-forge-domain/tests/corpus_sr10.rs" ;;
  sr-11) grep -q "NoSuchRoleForCorpus" "$path/.sea/interaction/interaction-model.sea" ;;
  sr-12) grep -q "UserIntentCorpusMoved" "$path/.sea/interaction/interaction-model.sea" ;;
  sr-13|sr-14|sr-15) grep -q "test_corpus_scenario_order_b" "$path/crates/sea-forge-core/tests/corpus_sr_order.rs" ;;
  df-1) grep -q "test_corpus_scenario_assertion_bug" "$path/domainforge-core/tests/flow_tests.rs" ;;
  df-2) grep -q "test_corpus_scenario_assertion_bug" "$path/domainforge-core/tests/entity_tests.rs" ;;
  df-3) [ ! -e "$path/domainforge-core/tests/flow_tests.rs" ] && [ -e "$path/domainforge-core/tests/flow_tests_moved.rs" ] ;;
  df-4) grep -q "new_with_namespace_corpus_moved" "$path/domainforge-core/src/primitives/entity.rs" && ! grep -q "pub fn new_with_namespace(" "$path/domainforge-core/src/primitives/entity.rs" ;;
  df-5) grep -q "from_concept_corpus_moved" "$path/domainforge-core/src/concept_id.rs" && ! grep -q "pub fn from_concept(" "$path/domainforge-core/src/concept_id.rs" ;;
  df-6) ! PATH=/usr/bin:/bin bash -c 'command -v cargo' >/dev/null 2>&1 ;;
  df-7) [ "$(stat -c %a "$path/domainforge-core/src/lib.rs")" = "000" ] ;;
  df-8) grep -q "test_corpus_scenario_missing_fixture" "$path/domainforge-core/tests/flow_tests.rs" ;;
  df-9) grep -q "NoSuchCustomerForCorpus" "$path/examples/namespaces/finance/cashflow.sea" ;;
  df-10) grep -q "CustomerCorpusMoved" "$path/examples/namespaces/finance/cashflow.sea" ;;
  df-11) grep -qFx 'patterns = [' "$path/.sea-registry.toml" ;;
  df-12) grep -cF "patterns = []" "$path/.sea-registry.toml" | grep -q "^1$" ;;
  df-13) grep -q "test_corpus_scenario_order_b" "$path/domainforge-core/tests/aggregation_tests.rs" ;;
  df-14) grep -q "test_corpus_scenario_timing_sensitive" "$path/domainforge-core/tests/aggregation_tests.rs" ;;
  df-15) grep -q "test_corpus_scenario_tmpdir_dependent" "$path/domainforge-core/tests/aggregation_tests.rs" ;;
  *) echo "ERROR: no mutation check for $repo-$n"; return 1 ;;
  esac
}

# ---- scenario verifier: prints "envs|cmd|stdin_format|signature" -----------

scenario_verifier() {
  local repo=$1 n=$2 path=$3
  case "$repo-$n" in
  cep-1|cep-2|cep-3|cep-11|cep-12|cep-13|cep-14|cep-15)
    printf '%s\n' "|cd $path && uv run pytest -q tests/test_import.py -k corpus_scenario|pytest-progress|FAILED"
    ;;
  cep-6|cep-7|cep-8)
    printf '%s\n' "|cd $path && uv run pytest -q tests/test_import.py -k corpus_scenario|pytest-progress|Interrupted"
    ;;
  cep-4)
    printf '%s\n' "|cd $path && uv run pytest -q tests/test_import.py|pytest-progress|file or directory not found"
    ;;
  cep-5)
    printf '%s\n' "|cd $path && uv run pytest -q tests/test_inventory.py|pytest-progress|file or directory not found"
    ;;
  cep-9)
    printf '%s\n' "TMPDIR=$path/.corpus-tmpfix|cd $path && uv run pytest -q tests/test_import.py -k corpus_scenario|pytest-progress|FAILED"
    ;;
  cep-10)
    printf '%s\n' "PATH=/usr/bin:/bin|cd $path && python3 -m pytest -q tests/test_import.py -k corpus_scenario|pytest-progress|No module named pytest"
    ;;
  ag-1|ag-3)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario|car-go-json|test result: FAILED"
    ;;
  ag-2)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario_assertion_bug_integration|car-go-json|test result: FAILED"
    ;;
  ag-4)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json --test unit_tests|car-go-json|no test target named"
    ;;
  ag-5)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json --test integration_tests|car-go-json|no test target named"
    ;;
  ag-6|ag-7|ag-8)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario|car-go-json|error\\[E|test result: FAILED"
    ;;
  ag-9)
    printf '%s\n' "AST_GREP_CONFIG=/nonexistent-corpus-config.toml|cd $path && cargo test --offline --message-format json corpus_scenario|car-go-json|test result: FAILED"
    ;;
  ag-10)
    printf '%s\n' "PATH=/usr/bin:/bin|cd $path && cargo test --offline --message-format json corpus_scenario|car-go-json|command not found"
    ;;
  ag-11)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario_unreadable_fixture|car-go-json|test result: FAILED"
    ;;
  ag-12)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario|car-go-json|could not compile"
    ;;
  ag-13)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario_missing_fixture|car-go-json|test result: FAILED"
    ;;
  ag-14)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario_stale_fixture|car-go-json|test result: FAILED"
    ;;
  ag-15)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json corpus_scenario_order -- --test-threads=1|car-go-json|test result: FAILED"
    ;;
  sr-1)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus_scenario_assertion_bug|car-go-json|test result: FAILED"
    ;;
  sr-2)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-domain corpus_scenario_assertion_bug|car-go-json|test result: FAILED"
    ;;
  sr-3)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core --test corpus_sr3_stale|car-go-json|no test target named"
    ;;
  sr-4)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-domain --test corpus_sr4_stale|car-go-json|no test target named"
    ;;
  sr-5)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus_scenario_assertion_bug|car-go-json|cannot find function"
    ;;
  sr-6)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-domain corpus_scenario_calls_interpret|car-go-json|cannot find function"
    ;;
  sr-7)
    printf '%s\n' "PATH=/usr/bin:/bin|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus|car-go-json|command not found"
    ;;
  sr-8)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus|car-go-json|could not compile"
    ;;
  sr-9)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus_scenario_missing_fixture|car-go-json|test result: FAILED"
    ;;
  sr-10)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-domain corpus_scenario_stale_fixture|car-go-json|test result: FAILED"
    ;;
  sr-11)
    printf '%s\n' "|cd $path && $SXR_DOMAINFORGE_BIN validate .sea/interaction/interaction-model.sea|df-validate|Undefined subject role"
    ;;
  sr-12)
    printf '%s\n' "|cd $path && $SXR_DOMAINFORGE_BIN validate .sea/interaction/interaction-model.sea|df-validate|Undefined entity: UserIntent"
    ;;
  sr-13|sr-14|sr-15)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p sea-forge-core corpus_scenario_order -- --test-threads=1|car-go-json|test result: FAILED"
    ;;
  df-1)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core --test flow_tests corpus_scenario_assertion_bug|car-go-json|test result: FAILED"
    ;;
  df-2)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core --test entity_tests corpus_scenario_assertion_bug|car-go-json|test result: FAILED"
    ;;
  df-3)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core --test flow_tests corpus_scenario|car-go-json|no test target named"
    ;;
  df-4)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core corpus|car-go-json|no function or associated item named"
    ;;
  df-5)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core corpus|car-go-json|no function or associated item named"
    ;;
  df-6)
    printf '%s\n' "PATH=/usr/bin:/bin|cd $path && cargo test --offline --message-format json -p domainforge-core corpus|car-go-json|command not found"
    ;;
  df-7)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core corpus|car-go-json|Permission denied"
    ;;
  df-8)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core --test flow_tests corpus_scenario_missing_fixture|car-go-json|test result: FAILED"
    ;;
  df-9|df-10)
    printf '%s\n' "|cd $path && $SXR_DOMAINFORGE_BIN validate examples/namespaces/finance/cashflow.sea|df-validate|Parse failed"
    ;;
  df-11)
    printf '%s\n' "|cd $path && $SXR_DOMAINFORGE_BIN validate examples/namespaces/finance/cashflow.sea|df-validate|Failed to parse registry"
    ;;
  df-12)
    printf '%s\n' "|cd $path && $SXR_DOMAINFORGE_BIN validate examples/namespaces/finance/cashflow.sea|df-validate|must provide at least one pattern"
    ;;
  df-13|df-14)
    printf '%s\n' "|cd $path && cargo test --offline --message-format json -p domainforge-core --test aggregation_tests corpus_scenario -- --test-threads=1|car-go-json|test result: FAILED"
    ;;
  df-15)
    printf '%s\n' "TMPDIR=/nonexistent-corpus-tmpdir|cd $path && cargo test --offline --message-format json -p domainforge-core --test aggregation_tests corpus_scenario_tmpdir_dependent|car-go-json|could not compile"
    ;;
  *) echo "ERROR: unknown scenario $repo-$n"; exit 1 ;;
  esac
}
