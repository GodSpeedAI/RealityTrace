# AGENTS.md

## Operating Principle

Work from evidence, not appearance.

Preserve scientific identity, provenance, durability, and recovery semantics. A successful-looking result is not a valid result unless the required evidence chain settles it.

Fail closed when evidence is missing, contradictory, ambiguous, stale, or insufficient.

Do not silently repair, normalize, retry, reinterpret, or discard evidence to make a run appear successful.

---

## `.agents/`

Always keep `.agents/` current.

Use `.agents/` for agent-operational state that should not become permanent product architecture.

Prefer:

- `.agents/current_status.yml` for the current implementation/recovery state and immediate next actions.
- `.agents/plans/` for implementation and recovery plans.
- `.agents/reports/` for generated analysis, verification, audit, and settlement reports.
- `.agents/prompts/` for substantial task-specific agent instructions.
- `.agents/recovery/` for recovery bindings, expected-state contracts, and related control artifacts.

Before substantial work:

1. Read the applicable files under `.agents/`.
2. Determine the current expected state, observed state, and unresolved difference.
3. Update the plan before materially changing implementation when the existing plan no longer matches reality.

After substantial work:

1. Update the relevant `.agents/` state.
2. Record what actually changed.
3. Record verification performed.
4. Record unresolved differences and next actions.
5. Do not describe something as complete merely because implementation exists.

Do not duplicate durable project documentation into `.agents/` unnecessarily.

---

## Expected State → Observed State → Difference

For non-trivial work, reason explicitly in terms of:

`expected state → observed state → difference → next affordable move`

Define the intended outcome before implementation whenever practical.

Acceptance criteria should be computable or directly verifiable.

Do not force a zero difference. If reality differs from the expected state, preserve the difference and report it.

Prefer producing artifacts and gates that allow another agent to independently determine whether the required state has actually been reached.

---

## Evidence and Settlement

Distinguish carefully between:

- an attempted operation;
- an observed response;
- persisted raw evidence;
- a durable commit;
- a parsed/classified result;
- a settled scientific result;
- a coordinator or summary view.

Higher-level summaries do not override lower-level durable evidence.

Raw-before-score semantics must be preserved where applicable.

Never treat filesystem presence alone as proof of settlement when stronger ledger, commit, hash, or checkpoint evidence exists.

Never infer scientific success from shell exit code alone.

A wrapper returning `0` does not establish scientific settlement.

Hash and provenance checks are part of the result, not optional bookkeeping.

---

## Scientific vs Execution Failure

Keep scientific/model outcomes separate from execution/infrastructure failures.

Examples:

- explicit transport ambiguity, provider failure, corrupted persistence, or failed authorization may be execution failures;
- a model that successfully completes but produces an unusable or empty required answer is a scientific/model outcome unless the frozen protocol explicitly says otherwise.

Do not make an undesirable scientific result replaceable by misclassifying it as infrastructure failure.

Replacement eligibility must depend only on the preregistered execution/governance rules, never on whether the scientific result is favorable.

---

## Historical Evidence Is Immutable

Never modify historical run evidence in place.

This includes, where applicable:

- completed or aborted attempts;
- raw responses;
- ledgers;
- checkpoints;
- manifests;
- authority records;
- qualification evidence;
- replacement decisions;
- preserved recovery snapshots;
- historical reports used as provenance.

When a correction is necessary:

1. preserve the old state;
2. create a new revision, attempt, projection, or migration artifact;
3. bind old and new evidence explicitly;
4. retain hashes and lineage.

Never delete failed attempts merely because a later attempt succeeded.

---

## Recovery and Replay

Recovery must preserve causal provenance.

Do not resume or retry an ambiguous scientific call unless the governing protocol explicitly establishes that doing so is safe.

Do not silently retry after ambiguous transport.

A recovery checkpoint is reusable only when its durability, identity, independence, and causal eligibility are proven.

Do not equate computational independence with scheduler independence.

When concurrency or scheduling may affect which external model/payment events occurred, account for execution-admission causality before reusing evidence.

Unknown or ambiguous recovery state is not safe state.

---

## Experiment Identity

Preserve the distinction between:

- scientific/design identity;
- executable identity;
- execution attempt identity;
- settlement identity.

A change to execution semantics may require a new executable revision even when the scientific design remains unchanged.

Do not claim two executions are identical merely because they use the same cases, prompts, model, or matrix.

When identities are frozen, verify them rather than reconstructing them from memory.

---

## Concurrency and Scheduling

Respect preregistered or frozen concurrency.

Do not dynamically increase or decrease scientific concurrency during a confirmatory execution unless the protocol explicitly permits it.

Count scientific workers/shards, not incidental wrapper or child processes.

Do not use broad process matching such as `pgrep -f` as sole proof of scientific liveness.

Prefer exact `/proc` argv/executable inspection or another unambiguous mechanism.

Treat:

- worker liveness;
- scientific progress;
- queue completion;
- scientific settlement

as separate states.

---

## Configuration

Do not hardcode environment-specific values when configuration is appropriate.

Prefer configuration in this order when practical:

1. existing project configuration mechanism;
2. `.toml` or `.yaml`;
3. `.env` / environment variables for secrets and deployment-specific values;
4. CLI flags for explicit one-off overrides.

Do not use Markdown files as runtime configuration.

Do not hardcode:

- secrets;
- credentials;
- machine-specific paths;
- ports;
- provider endpoints;
- model identifiers;
- concurrency settings;
- timeouts;
- environment-specific resource limits

unless they are intentionally frozen protocol constants and their immutability is part of the design.

If a value is scientifically frozen, make that distinction explicit rather than disguising it as ordinary application configuration.

Prefer configuration files over growing collections of command-line flags.

Never commit secrets.

---

## Implementation

Prefer the smallest change that resolves the demonstrated difference.

Do not refactor unrelated code while repairing experimental or evidentiary behavior.

Do not broaden scope merely because nearby code could be improved.

Reuse established project abstractions before introducing new ones.

Avoid parallel implementations of the same concept.

Keep payment-bearing/scientific execution code separate from analysis, audit, recovery, and reporting utilities where practical.

Analysis tools should be read-only by default.

A forensic or verification tool must not accidentally invoke a model/provider.

---

## Commands and Automation

Prefer reproducible scripts and repository commands over undocumented sequences of manual shell operations.

For multi-step or fail-closed operations:

- prefer a checked-in script when the operation is reusable;
- otherwise use a temporary script rather than a fragile long pasted shell expression;
- use `set -euo pipefail`;
- validate scripts with `bash -n` before execution;
- use subshells when failure should not damage the parent interactive shell.

Prefer existing `just`, test, and project commands rather than inventing equivalent command surfaces.

Use the project's pinned/offline execution environment when one exists.

Do not make network access an incidental dependency of verification.

---

## Testing

A change is not complete when the implementation compiles.

Test the behavior that caused the change.

For defects, add regression tests that reproduce the observed failure mode.

Include negative and fail-closed cases.

Where evidence transformation or recovery is involved, test:

- missing evidence;
- partial commits;
- unresolved in-flight state;
- hash mismatch;
- duplicate identity;
- ambiguous mapping;
- deterministic replay/projection;
- immutable-source behavior.

Do not weaken existing tests to make a new implementation pass.

Run the narrowest relevant tests first, then the broader applicable suite.

Report exact test counts and failures.

---

## Determinism

Deterministic operations should produce deterministic outputs.

Separate nondeterministic metadata such as generation timestamps from the deterministic evidentiary projection when necessary.

For merge, migration, inventory, or replay operations:

- define canonical ordering;
- reject missing coordinates;
- reject unexpected coordinates;
- reject duplicates;
- verify hashes;
- verify provenance;
- verify identity bindings.

Do not rely on filesystem traversal order.

---

## Git and Repository Safety

Do not use destructive Git operations to resolve uncertainty.

Avoid `git reset --hard`, destructive checkout, history rewriting, or deleting untracked evidence unless explicitly authorized.

Inspect repository state before modifying it.

Do not overwrite unrelated user changes.

When working from a reviewed historical identity, prefer a dedicated branch/worktree rather than altering the original checkout.

Keep commits narrow and explain the evidentiary or behavioral change they establish.

---

## Agent Behavior

Do not guess when the repository can answer the question.

Inspect the relevant source, tests, logs, manifests, ledgers, checkpoints, and `.agents/` state before making claims.

Do not credit the system with reasoning, recovery, or robustness that was explicitly supplied by operator instructions.

Distinguish:

- what the implementation actually discovered;
- what operator instructions told it to do;
- what tests demonstrate;
- what is inferred but not proven.

When blocked, report the exact missing evidence or unresolved invariant rather than improvising around it.

Do not continue across a failed gate unless the governing plan explicitly defines the next transition.

---

## Completion Standard

Before declaring work complete, answer:

1. What outcome was required?
2. What state was actually produced?
3. What evidence proves it?
4. What tests passed?
5. What identities/hashes matter?
6. What remains different from the expected state?
7. What remains unresolved or ambiguous?
8. What is the next authorized move?

If the material difference is not zero, do not call the task fully settled.

Prefer a smaller proven settlement over a larger unverified claim.