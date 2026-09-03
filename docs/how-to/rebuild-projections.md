# How-To: Rebuild Projections & Verify Replay Determinism

This guide describes how to completely reconstruct SXR's derived relational and graph projection tables from the append-only ledger and verify byte-identical replay determinism (AC-REPLAY-002).

---

## Goal
Wipe and reconstruct derived read-model tables ($G_{declared}$, $G_{observed}$, FTS5), verify that the rebuild produces zero drift, and export an external anti-collapse evaluation corpus.

---

## Prerequisites
- An enrolled repository containing committed ledger rows.

---

## Procedure

### 1. Check Current Projection Status
Inspect the watermarks and row counts across projection tables:

```bash
sxr projections status --repo <repo_id> --json
```

### 2. Export Canonical JSON Pre-Rebuild
Export the complete relational state in canonical JSON format to a temporary file:

```bash
sxr projections export --repo <repo_id> --canonical-json > /tmp/before.json
```

### 3. Rebuild All Projections
Drop and re-apply all projection tables by replaying the ledger from ordinal 1:

```bash
sxr projections rebuild --repo <repo_id> --all --json
```

**Expected Output**:
```json
{
  "rebuilt": true,
  "rows_processed": 14,
  "watermark": {
    "last_ledger_id": "01J7...",
    "last_append_ordinal": 14
  }
}
```

### 4. Verify Replay Determinism (AC-REPLAY-002)
Export the post-rebuild state and verify it is byte-identical:

```bash
sxr projections export --repo <repo_id> --canonical-json > /tmp/after.json
diff -u /tmp/before.json /tmp/after.json && echo "REBUILD IDENTICAL: Zero Drift"
```
The diff must be completely empty.

### 5. Export Anti-Collapse Corpus
To export SXR records for independent third-party CEP conformance evaluation:

```bash
sxr projections export-anti-collapse-corpus \
  --repo <repo_id> \
  --question <question_id> \
  --snapshot <snapshot_id> > /tmp/anti-collapse-corpus.json
```

---

## Implementation Locations
- Rebuild engine: [`sxr-cli/src/projections.rs:projections_rebuild`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs)
- Canonical export: [`sxr-cli/src/projections.rs:projections_export`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs)
- Anti-collapse export: [`sxr-cli/src/projections.rs:export_anti_collapse_corpus`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs)

---

## Common Failure Symptoms & Troubleshooting

| Symptom | Cause | Resolution |
| :--- | :--- | :--- |
| `projections rebuild: --all required` (Exit 2) | Omitted the required `--all` confirmation flag. | Run with `--all`. |
| `diff /tmp/before.json /tmp/after.json` shows divergence | A projection table logic defect introduced non-deterministic ordering or timestamps. | Check `apply_row_with_cas` sorting order. Ledger timestamps must not be overwritten with current wall time during replay. |
