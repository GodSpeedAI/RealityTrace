# crates.io publish failure: v0.3.0 rate-limited (HTTP 429)

Date: 2026-09-30
Scope: release tooling (`scripts/publish-crates.sh`, `.github/workflows/release-please.yml`)
Status: settlement reached; all six crates published.

## Expected state

Release-please tags six packages and the `Publish to crates.io` job publishes all
six to crates.io: `sxr-core 0.3.0`, `sxr-git 0.2.1`, `sxr-ledger 0.2.1`,
`sxr-df 0.2.1`, `sxr-verify 0.2.1`, `realitytrace 0.3.0`.

## Observed state (before remediation)

Release run 36140411607 (push of `dfadf6a`, 2026-09-25T13:21:12Z) published five
crates and failed on the sixth:

```
error: failed to publish realitytrace v0.3.0 to registry at https://crates.io
  the remote server responded with an error (status 429 Too Many Requests): You have
  published too many new crates in a short period of time. Please try again after
  Fri, 25 Sep 2026 13:24:28 GMT and see https://crates.io/docs/rate-limits for more details.
```

Uploads: sxr-core 13:22:24, sxr-git 13:22:45, sxr-ledger 13:23:05, sxr-df 13:23:23,
sxr-verify 13:23:45, realitytrace 13:24:10 (rejected). Six new crate names in 81
seconds.

## Difference (root cause)

crates.io rate-limits first-time publishes with a per-account token bucket:
`LimitedAction::PublishNew` has `default_rate_seconds = 10 * 60` and
`default_burst = 5` (`rust-lang/crates.io`, `src/rate_limiter.rs`). Every crate in
that run was a brand-new crate name, so the sixth publish was guaranteed to be
rejected. The burst was exhausted; the server advertised the next token at
13:24:28, eighteen seconds after the failed attempt.

Two contributing conditions:

1. `scripts/publish-crates.sh` classified only index-propagation errors as
   retryable, so a 429 fell through to the fatal branch and aborted the job.
2. v0.2.0 never published at all (its `Publish to crates.io` job was skipped because
   the gate read the unprefixed root `release_created` output; fixed by 7a2cd91).
   That made v0.3.0 the first run to publish six new crate names at once.

## Change

- `scripts/publish-crates.sh`: new `is_rate_limited()` classifier and
  `rate_limit_wait_seconds()` helper. A 429 now waits for the retry instant
  advertised by crates.io, clamped to 1..`RATE_LIMIT_MAX_WAIT_SECONDS` (660) with a
  `RATE_LIMIT_FALLBACK_SECONDS` (600) fallback when the hint is missing or
  unparseable, bounded by `RATE_LIMIT_MAX_WAITS` (3, env-overridable).
- `.github/workflows/release-please.yml`: publish job `timeout-minutes` 30 -> 45 to
  cover the longest bounded wait sequence.
- `scripts/tests/rate-limit-wait.test.sh` (new) and a CI step running it.

Commits: 5b3fb4a (`fix(release): retry crates.io publishes rejected with HTTP 429`),
d49bf40 (`test(release): cover the crates.io 429 retry path`), 6999053
(`fix(release): make the crates.io retry instant parsing locale-independent`),
de9a1dc (`fix(release): extract the retry instant as a pure, pinned function`),
1765946 (`docs(release): restore the rate_limit_wait_seconds docstring
placement`). None touches a package path, so release-please attributed no release
to them: Release runs 36798592239, 36798942190 and 36799226235 all skipped the
publish job because no release existed.

## Verification

- `bash -n` and `shellcheck` clean on both scripts.
- `scripts/tests/rate-limit-wait.test.sh`: **16/16 checks passed** locally. It
  drives helpers extracted from the shipped script (failing closed if that layout
  changes) with the exact 429 text observed in production; negative cases (403
  auth failure, index propagation, already published must not classify as rate
  limits); deterministic extraction checks (real text, duplicated GMT token,
  missing instant, dotted localised instant); and wait clamping/fallback cases.
- Registry state: crates.io API max versions `sxr-core 0.3.0`, `sxr-git 0.2.1`,
  `sxr-ledger 0.2.1`, `sxr-df 0.2.1`, `sxr-verify 0.2.1`, `realitytrace 0.3.0`;
  sparse index `realitytrace 0.3.0 yanked=false`.

## Remediation driven by CI

Release run 36798591977 (`workflow_dispatch`, `publish_now=true`): success.
Log evidence: checkout used `ref: refs/heads/main` (patched script, not the
`realitytrace-v0.3.0` tag); `sxr-core|sxr-git|sxr-ledger|sxr-df|sxr-verify is
already published — skipping`; `Uploading realitytrace v0.3.0`;
`Successfully published realitytrace`.

### CI failure (run 36798592121) and its cause

The new `Check release scripts` step failed on push d49bf40: the two cases that
generate a retry instant returned the `600` fallback while the literal crates.io
string passed (clamped to 1s). Cause, established by local reproduction:

- The fixture appended ` GMT` to a value that already ended in ` GMT`, putting a
  second GMT token in the message.
- `try again after [^.]*GMT` matched greedily and captured both tokens: on
  `... after Thu, 01 Oct 2026 01:03:45 GMT GMT and see https://crates.io/...` it
  yields the hint `Thu, 01 Oct 2026 01:03:45 GMT GMT` (verified with `grep -oE`).
- The runner's `date` rejects that hint, so the helper took the fallback. It
  passed locally because this machine's `date` is uutils coreutils 0.8.0
  (`/usr/bin/date -> ../lib/cargo/bin/coreutils/date`), which accepts the
  duplicated zone token (`rc=0`). GNU `date` is not installed here, so the
  runner-side rejection is inferred from the CI evidence rather than reproduced
  directly.

An earlier hypothesis in this report blamed the runner locale. It is
**withdrawn**: the runner log shows `LANG=C.UTF-8` (English), and the actual
fixture defect was the duplicated GMT token.

### Fixes

- 6999053: anchor the extraction to the exact shape crates.io emits, parse the
  hint under `LC_ALL=C`, and format the test fixtures under `LC_ALL=C` using
  epoch arithmetic (locale variance removed as hardening, not as the fix).
- de9a1dc: split the extraction into the pure function `rate_limit_hint()` so it
  is pinned by deterministic checks that do not depend on the host's `date`, and
  make the generated fixtures faithful (no second GMT token).

### CI validation

- CI run 36798942244 (push of 6999053): success, `Check release scripts` step
  passing with `ALL 11 CHECKS PASSED` on the runner.
- CI run 36799226230 (push of de9a1dc): `Check release scripts`, clippy and tests
  all successful, with the suite at 16 checks.

## Residual differences

- `sxr-* 0.2.0` was never published; crates.io has no 0.2.0 for any package. Those
  versions are unrecoverable without republishing, and no current consumer needs
  them.
- The 15s inter-crate sleep for index propagation is still present; it does not
  mitigate the new-crate token bucket (15s refills 0.025 of a token) and is
  unrelated to this failure.

## Next authorized moves

- A release that introduces more than three additional new crate names at once can
  still exceed `RATE_LIMIT_MAX_WAITS`; re-dispatch `publish_now=true` to drain the
  remainder, or split new crate names into separate releases.
- `sxr-cli` is published as `realitytrace`; verify README installation commands
  reference the published name.
