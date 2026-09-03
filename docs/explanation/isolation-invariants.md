# Explanation: Repository Isolation & Identity Invariants

This document explains why SXR enforces total filesystem isolation from enrolled repositories and decouples repository identity from Git commit hashes.

---

## 1. The Repository Non-Mutation Invariant (AC-SECURITY-001)

### The Principle
> **Invariant**: An audit tool must not contaminate the object of its audit.

If an audit or telemetry runtime writes files, logs, or databases inside the target project checkout:
1. `git status` becomes dirty, polluting developer working trees.
2. Build tools (e.g. `cargo`, `pytest`, `npm`) may inadvertently package audit databases into production release artifacts.
3. Multiple agents or branches operating on the same checkout will experience lock collisions or git merge conflicts inside `.audit/` directories.

### SXR Implementation
SXR treats the enrolled repository as strictly **read-only**:
- All databases (`ledger.db`), configuration files, locks, CAS blobs, and logs reside under standard XDG paths (`$XDG_DATA_HOME/sxr` and `$XDG_STATE_HOME/sxr`).
- Proof scripts enforce this invariant via `git status --porcelain | wc -l | grep -qx 0`. If even a single byte is created in the repository checkout, the gate fails.

---

## 2. Decoupling Identity from Git (AC-IDENTITY-001)

Most developer tools identify repositories using:
- The remote URL (`git remote get-url origin`)
- The root commit hash (`git rev-list --max-parents=0 HEAD`)
- The current commit hash (`git rev-parse HEAD`)

### Why Git-Derived Identities Fail
1. **Local Scratch Repositories**: Repositories created locally for spikes or private experiments often have no remote URL configured.
2. **Rebased / Filtered History**: If an engineer runs `git commit --amend`, `git rebase`, or squash-merges, the commit hash changes completely, breaking all historical bindings.
3. **Worktrees & Clones**: An engineer often maintains two clones of the same repository (e.g., `main` vs `feature-x`). Under Git-derived naming, both checkouts collide and write to the same database.

### The Minted ULID Solution
When `sxr repo enroll <path>` is executed:
- SXR mints a fresh, random **128-bit ULID** for `repo_id`.
- SXR mints a separate **ULID** for `checkout_id`.
- These identifiers are stored in `$XDG_DATA_HOME/sxr/repos/<repo_id>/registry.json` alongside an `IdentityEvidence` snapshot recording the Git state at enrollment.
- If the repository is rebased, rewritten, or renamed on disk, its SXR `repo_id` remains immutable and stable.

---

## 3. Multi-Tenant Graph Partitioning (AC-IDENTITY-002)

What happens if an organization has two separate repositories that define the same domain concept (e.g., concept `Order` with ID `d842502d-...`)?

In traditional global knowledge graphs, identical IDs cause catastrophic node collisions.

SXR prevents this through strict **scoped partitioning**:
- In the projection schema, declaration nodes in $G_{declared}$ are keyed by:
  ```sql
  PRIMARY KEY (snapshot_id, decl_key)
  ```
- Because each snapshot belongs to a single `repo_id`, queries scoped to one repository never see or merge nodes from another repository.
- Proven in `proof/slice3.sh`: the identical declaration identity enrolled in two separate repositories remains completely distinct.

---

## 4. Source Trail

- [`sxr-cli/src/state.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/state.rs) — XDG directory layout and repository sandbox isolation.
- [`sxr-git/src/identity.rs`](file:///home/sprime01/projects/sxr/sxr-git/src/identity.rs) — ULID identity minting and identity evidence capture.
- [`sxr-cli/src/projections.rs`](file:///home/sprime01/projects/sxr/sxr-cli/src/projections.rs) — Partitioned primary keys on `sxr_decl_node`.
- [`proof/slice3.sh`](file:///home/sprime01/projects/sxr/proof/slice3.sh) — Test verifying AC-IDENTITY-002 partition isolation.
