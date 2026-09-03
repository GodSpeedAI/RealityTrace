# How-To: Enroll a Repository & Configure Semantics

This guide describes how to enroll an existing Git repository into SXR and configure its declarative semantic model entry point.

---

## Goal
Enroll a local Git working tree into SXR without mutating the repository, and register its entry `.sea` file.

---

## Prerequisites
- SXR binary compiled and available on `PATH` (`sxr`).
- A target Git repository containing code and optional `.sea` model files.

---

## Procedure

### 1. Enroll the Repository
Execute the `repo enroll` command pointing to the root of your Git repository:

```bash
sxr repo enroll /path/to/my-repo --json
```

**Verification**: Check that SXR emitted JSON containing `repo_id` and `checkout_id`:
```json
{
  "repo_id": "01J7Y8...",
  "checkout_id": "01J7Y9...",
  "path": "/path/to/my-repo",
  "ledger_position": { "ledger_id": "01J7...", "append_ordinal": 1 }
}
```

### 2. Configure Semantic Models
If your project defines DomainForge `.sea` models (e.g. at `models/core.sea` or `.sea/model.sea`), configure the logical entry path:

```bash
sxr semantic configure --repo <repo_id> --entry models/core.sea --json
```

**Verification**: Inspect the configuration status:
```bash
sxr semantic status --repo <repo_id> --json
```

### 3. Verify Sandboxing
Confirm that SXR did not contaminate your Git repository:
```bash
git -C /path/to/my-repo status --porcelain
```
The output must be completely empty.

---

## Implementation Locations
- Enrollment logic: [`sxr-cli/src/commands.rs:repo_enroll`](file:///home/sprime01/projects/sxr/sxr-cli/src/commands.rs)
- Git discovery: [`sxr-git/src/discovery.rs:discover`](file:///home/sprime01/projects/sxr/sxr-git/src/discovery.rs)
- Persistence: [`sxr-cli/src/state.rs:write_enrollment`](file:///home/sprime01/projects/sxr/sxr-cli/src/state.rs)

---

## Common Failure Symptoms & Troubleshooting

| Symptom | Cause | Resolution |
| :--- | :--- | :--- |
| `sxr: not a git repository` (Exit 2) | The target path does not contain `.git`. | Ensure you pass the working tree root, not a subdirectory or non-git folder. |
| `sxr: entry file does not exist` (Exit 3) | The path provided to `--entry` does not exist relative to the repo root. | Check the relative path of your `.sea` file. |
| `sxr: repo <id> is not enrolled` (Exit 3) | Invalid or mistyped `repo_id`. | Run `sxr repo list --json` to inspect all enrolled repository IDs. |

---

## Rollback & Un-enrollment
To remove an enrollment without affecting the underlying Git repository:
- Remove the sandboxed directory at `$XDG_DATA_HOME/sxr/repos/<repo_id>/`.
