# Tutorial: End-to-End Verification Quickstart

This hands-on tutorial guides you through the complete SXR lifecycle: enrolling a Git repository, extracting a semantic snapshot, declaring a question and settlement threshold, executing a verification test plan, deriving evidence, registering settlement, and cryptographically verifying the ledger.

---

## Prerequisites

Before starting, ensure you have:
1. **Rust toolchain** installed (`cargo`, `rustc`).
2. **Git** installed and configured on your `PATH`.
3. The `sxr` binary built:
   ```bash
   cargo build -p sxr-cli
   export PATH="$PWD/target/debug:$PATH"
   ```
4. Optional: Set `SXR_DOMAINFORGE_BIN` if you have the DomainForge binary installed. If not, this tutorial works with standard Cargo repositories.

---

## Step 1: Set Up an Isolated Test Environment

To avoid polluting your home directory during this tutorial, set isolated XDG directories in your terminal session:

```bash
export XDG_DATA_HOME=/tmp/sxr-tutorial/data
export XDG_STATE_HOME=/tmp/sxr-tutorial/state
mkdir -p /tmp/sxr-tutorial/data /tmp/sxr-tutorial/state
```

Verify SXR is healthy using `sxr doctor`:

```bash
sxr doctor --json
```

**Expected Output**:
```json
{
  "sxr_output_version": "1",
  "filesystem_class": "posix_compliant",
  "daemon_socket": "inactive",
  "repos_enrolled": 0
}
```

---

## Step 2: Create a Minimal Target Repository

Create a minimal Rust project with a passing test:

```bash
mkdir -p /tmp/sxr-tutorial/my-project/src
cd /tmp/sxr-tutorial/my-project

cat > Cargo.toml <<'EOF'
[package]
name = "my-project"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
EOF

cat > src/lib.rs <<'EOF'
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 2), 4);
    }
}
EOF

git init -q
git add -A
git -c user.email="dev@example.com" -c user.name="Developer" commit -qm "Initial commit"
```

---

## Step 3: Enroll the Repository

Enroll your repository into SXR:

```bash
sxr repo enroll /tmp/sxr-tutorial/my-project --json
```

**Expected Output**:
```json
{
  "repo_id": "01J7ABCDEF1234567890ABCDEF",
  "checkout_id": "01J7ABCDEF1234567890ABCDEG",
  "path": "/tmp/sxr-tutorial/my-project",
  "ledger_position": {
    "ledger_id": "01J7...",
    "append_ordinal": 1
  }
}
```

Save your `repo_id` to an environment variable:
```bash
export REPO_ID=$(sxr repo list --json | python3 -c "import sys, json; print(json.load(sys.stdin)['repos'][0]['repo_id'])")
echo "Enrolled repo: $REPO_ID"
```

---

## Step 4: Declare a Question & Settlement Threshold

Create a question file asking whether our target concept is verified:

```bash
cat > /tmp/question.json <<'EOF'
{
  "question_form": "is_true",
  "question_type": "declared_semantics",
  "target_entities": "concept:addition_logic"
}
EOF

QID=$(sxr question declare --repo "$REPO_ID" --file /tmp/question.json --json | python3 -c "import sys, json; print(json.load(sys.stdin)['question_id'])")
echo "Declared Question: $QID"
```

Create a threshold requiring that all evidence supports the claim:

```bash
cat > /tmp/threshold.json <<'EOF'
{
  "threshold_id": "t1",
  "kind": "AllEvidenceSupports",
  "minimum_reliability": "medium"
}
EOF

TID=$(sxr threshold declare --repo "$REPO_ID" --question "$QID" --file /tmp/threshold.json --json | python3 -c "import sys, json; print(json.load(sys.stdin)['threshold_id'])")
echo "Declared Threshold: $TID"
```

---

## Step 5: Begin a Verification Run

Initiate an active run binding our question and threshold:

```bash
RUN_ID=$(sxr run begin --repo "$REPO_ID" --snapshot "snapshot:genesis" --question "$QID" --threshold "$TID" --json | python3 -c "import sys, json; print(json.load(sys.stdin)['run_id'])")
echo "Active Run: $RUN_ID"
```

Create a verification plan for the `cargo-test` runner:

```bash
PLAN=$(sxr plan create --run "$RUN_ID" --verifier cargo-test --target "concept:addition_logic" --json)
TRANSFORM=$(python3 -c "import json; print(json.loads('''$PLAN''')['transformation_ref'])")
echo "Transformation Reference: $TRANSFORM"
```

---

## 6. Execute the Verifier

Run the verifier test suite:

```bash
VERIFY_OUT=$(sxr verify run --run "$RUN_ID" --json)
echo "$VERIFY_OUT"
```

**Expected Output**:
```json
{
  "run_id": "...",
  "status": "observed",
  "observations": [
    {
      "observation_id": "01J7...",
      "exit_code": 0,
      "parsed_summary": {
        "passed": 1,
        "failed": 0,
        "ignored": 0
      }
    }
  ]
}
```

Extract the `observation_id`:
```bash
OBS_ID=$(echo "$VERIFY_OUT" | python3 -c "import sys, json; print(json.load(sys.stdin)['observations'][0]['observation_id'])")
echo "Observation ID: $OBS_ID"
```

---

## Step 7: Derive Evidence from the Observation

Promote the raw observation into qualified evidence:

```bash
sxr evidence create --run "$RUN_ID" --observation "$OBS_ID" --derive --json
```

**Expected Output**:
```json
{
  "evidence_id": "01J7...",
  "run_id": "...",
  "direction": "supports",
  "relevance": "high",
  "reliability": "high",
  "precedes_observation": true
}
```

---

## Step 8: Register Settlement & Conclude Run

Deterministically evaluate the threshold over the committed evidence:

```bash
sxr settlement register --run "$RUN_ID" --question "$QID" --threshold "$TID" --transformation "$TRANSFORM" --json
```

**Expected Output**:
```json
{
  "settlement_id": "01J7...",
  "settlement_status": "settled",
  "confidence": "high"
}
```

Conclude the run:
```bash
sxr run finish --run "$RUN_ID" --json
```

---

## Step 9: Verify Cryptographic Ledger Integrity

Prove that the entire append-only hash chain is dense, intact, and untampered:

```bash
sxr ledger verify --repo "$REPO_ID" --json
```

**Expected Output**:
```json
{
  "ok": true,
  "rows_verified": 8,
  "head_ordinal": 8,
  "generation_id": "01J7..."
}
```

Finally, verify that SXR adhered to **AC-SECURITY-001** (never touching the target Git repository):
```bash
git -C /tmp/sxr-tutorial/my-project status --porcelain
```
The output should be completely **empty**, confirming that your Git working tree was not contaminated.

---

## Where to Go Next
- Learn why observations are separated from evidence in [Anti-Collapse Epistemology](../explanation/anti-collapse-epistemology.md).
- Learn how to diagnose test failures in [How to Diagnose Failures](../how-to/diagnose-failures.md).
- Explore all CLI flags and subcommands in the [CLI Reference](../reference/cli.md).
