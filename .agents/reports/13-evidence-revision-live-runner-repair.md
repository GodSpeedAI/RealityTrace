# evidence-revision-001 revision-002 live runner integration repair

## Verdict

The authorized pilot invocation stopped safely because `_cmd_run` sent `client=None` through the old client-required gate, making `_live_client()` unreachable; a subsequent unconditional `NotImplementedError` also prevented any live matrix loop. No provider call, model call, unit, or output namespace occurred.

The mechanical repair is complete offline. It does not alter science, authorization, release artifacts, or frozen matrix semantics.

## Two-phase gate

1. **Phase A — `validate_static`** verifies owner/stage authority, both fingerprints, release token/marker bindings, owner digest, corpus/evidence/action hashes, matrix/case-input hashes, run ID, namespaces, arm isolation, pair equality, RAW/LEGIBLE equivalence, prompt leak guard, exact K=3, payment <=7, zero structural missingness/singletons, and the pilot-only FULL lock. It accepts `client=None` because it does not inspect or instantiate a client.
2. Only after Phase A passes, `_live_client(model="glm-5.2")` constructs the existing `ZaiCodingPlanClient`. Its constructor only assigns `model` and `timeout`; it reads no credential and makes no request.
3. **Phase B — `validate_client`** checks the interface, frozen model, Z.ai Coding Plan route/endpoint, and local credential shape through `validate_local_configuration`. Local validation reads configuration only and records `network_requests=0`; it never calls `complete` or `urlopen`.
4. **Phase C — `finalize`** requires matching successful phase bindings and returns the sole `FINAL_AUTHORIZED` result. Only then is the run namespace created and the sequential frozen matrix loop entered.

Static failure cannot call `_live_client` or create output. FULL remains refused while `full_execution_authorized=false`.

## Frozen live loop

The new live loop reuses `_run_frozen_unit`, the shared body behind `run_offline_unit`. It preserves the existing v2 UID/checkpoint path, `DurableReplayClient`, raw fsync/hash/ledger before parse, request-bound replay, typed controller/parser/repair, marker-neutral frozen case inputs and observations, exact-K paired policies, result commitment, provider provenance, truncation/output classification, state continuity, prompt guard, and gate-blind versus governed claims.

The loop is intentionally sequential because the checkpoint and ledgers are not multi-writer structures. It visits the exact 60 matrix UIDs in canonical order, verifies each returned UID, resumes verified completed units without transport, and writes an atomic progress summary. It stops at `RESULT_COMMITTED`; it does not invent or invoke a scorer.

## Provider construction

`ZaiCodingPlanClient.__init__` is network-inert. Credential reading, JWT generation, HTTP request construction, `urlopen`, retries, and provider contact remain confined to `complete`, which is unreachable before final authorization. Tests replace credential/network functions with failure spies and prove construction and local validation perform zero network requests.

Future response provenance continues recording requested model/endpoint separately from actual provider-returned model, route, generation ID, finish reason, usage, latency, retries, and cost when returned; absent provider fields remain absent/null by reason rather than fabricated.

## Identity decision

- Design fingerprint unchanged: `3031af33611c9ae8ab0fd0bba22f0abc35147d4fe564e0e3623a174d0dfe28d9`.
- Executable fingerprint unchanged: `12eb26d72f74709a827059c2a6ccb693ef956a6d5aa7ca2bf6f292b645da010e`.

This is required by the actual fingerprint definition. `validity_revision.executable_declarations()` hashes the design identity, case-input/marker/feasibility/leakage bindings, evidence policies, pilot selection/cases/repetitions/namespace, and full namespace. It intentionally contains no runner source, gate sequencing, transport construction, or orchestration implementation hash. Runner/source hashes are separately persisted as run provenance. The repair makes the implementation conform to the already-frozen executable declarations and changes none of their preimage fields.

Therefore the owner authorization at `86f77ad6a7a293de215f9fb7059af67fe3e22366` and the existing release marker remain identity-current; no owner authorization update or marker regeneration is required.

## Offline verification

- Complete suite: **362 collected, 362 passed, 0 failed, 0 skipped**.
- Historical client-none final-gate defect reproduced.
- Static gate succeeds without client but is not final authority.
- Static failures prevent client construction/output.
- Valid fake reaches all 60 real matrix-loop entries.
- Old `NotImplementedError` absent.
- Unauthorized pilot, FULL, marker/fingerprint drift, prompt leak, raw-before-score, resume, and no-network controls pass.
- Runner SHA-256: `96f1fd269a413161818ff37d9923ce132e79bb1f392832523175ca1855501d38`.
- Provider SHA-256: `1f7bfd2ab6d1e86fe344085b38e6359ca0aae43b35ad07cfe76ddd72ff6f9aea`.

## Commits

- `60f599be77c56dd231c47cca41bf9b362bcc0bbb` — two-phase gate, inert local client validation, frozen live loop
- `d0f176dce9d96abc9e80bcf71e96e154efc182d1` — offline live-gate/loop regression suite and authorization-state test corrections

## Authority and calls

- PILOT authorized by owner: true.
- FULL authorized: false.
- Provider calls during repair: 0.
- Model calls during repair: 0.
- Pilot executed during repair: false.
- Existing token contents were not read or displayed and release artifacts were not modified.
