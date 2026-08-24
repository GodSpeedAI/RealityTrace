# Plan: exec006 — transport model-id drift successor

## Observed difference (2026-08-23)
- exec005 preflight PASS, but fresh synthetic qualification returned FAIL: 20/20 calls
  `REJECTED_RETRYABLE HTTP 500`, zero delivered, zero ambiguous.
  Evidence: `ablation/analysis/post-audit-v2/exec005-fresh-evidence/qualification.json`
  (run_id exec005-qual-20260823T145356Z). Preserved unmodified as drift evidence.
- Manual loopback reproduction: create_session 200; send_message 500 for frozen id
  `deepseek-v4-flash-free`. Server catalog (`GET /config/providers`) lists provider
  `opencode` models `deepseek-v4-flash`, `deepseek-v4-pro` — the `-free` id was retired
  upstream after the exec004/005 freeze (Aug 12).
- Non-free catalog id `deepseek-v4-flash` reaches the upstream gateway but fails with
  upstream `CreditsError` (401 no payment method): assistant message records
  `error.name=APIError`, empty parts. Not usable without billing.
- Owner directive (this session): proceed using the OpenCode free Ox Alpha model.

## Expected state (exec006)
- Execution revision 006 successor of exec005. Scientific design ER002 unchanged
  (fingerprint 05628406…); shard topology 16×4 unchanged; raw-before-score,
  ambiguity freeze, outcome-blind replacement unchanged; endpoint, restricted agent,
  deny-all tools, one-session-per-call unchanged.
- Only transport delta: requested/resolved model id
  `deepseek-v4-flash-free` → `x-preview-f-free` ("Ox Alpha Free (Unlimited)",
  provider `opencode`, cost 0.0, verified present in server catalog).
- New frozen `evidence-revision-002-exec006-*` artifacts, FROZEN_NOT_AUTHORIZED;
  exec005 recorded as immutable lineage with successor reason
  UPSTREAM_MODEL_ID_RETIRED. exec005 files/artifacts byte-untouched.
- Unit IDs disjoint from all prior revisions (executable fingerprint changes).

## Transition steps
1. Implement `eval/v2/evidence_revision_002_exec006.py` (copy of exec005 coordinator;
   own MODEL constant; local client subclass overriding `model`; lineage block bound to
   exec005 hashes incl. its NOT_RUN qualification disposition).
2. Adapt test file `eval/tests/test_v2_evidence_revision_002_exec006.py` from exec005's.
   Targeted suite green; then complete offline ablation suite green.
3. build-artifacts + preflight PASS (exec006 set; exec005 preflight must still PASS).
4. Commit/review (candidate commit = HEAD at authorization time).
5. Live: fresh session watermark → exec006 qualification-bench (20 synthetic calls,
   expect 20/20 DELIVERED) → read-only session provenance audit (20 sessions,
   route/agent matches, tool_parts_total 0) → authorize-full --owner-authorized →
   initialize run + 16 shards @ ≤4 workers (driver scripts exist) → merge/verify →
   scientific projection + settlement report; update .agents/CURRENT_STATUS.md.

## Failure policy
- Any AmbiguousTransport freezes that shard attempt (existing semantics); replacement
  only by incremented attempt. Qualification FAIL blocks authorize-full fail-closed.
- If `x-preview-f-free` is also unavailable at execution time: STOP and re-consult
  owner; do not substitute further models autonomously.
