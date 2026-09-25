# Ingest caller extensions + release publish fix — 2026-09-25

**Expected state:** a caller (Cognate) can attach structured correlation ids to an
`observation.recorded` record through a documented CLI surface, read them back, and
see them unchanged on duplicate delivery; a release-please release publishes the
crates to crates.io.

**Observed before:**
- `sxr ingest` had no input for the CEP §39 `extensions` section; callers' ids were
  only opaque stdin bytes.
- `sxr_core::envelope::build_envelope` required every entity section to be an
  array, contradicting `schemas/cep-semantic-envelope.schema.json`
  (`extensions: {"type":"object"}`).
- `.github/workflows/release-please.yml` gated `cargo-publish` on the root-only
  `release_created` output. The manifest has no root (`.`) component, so the
  output was never set and v0.2.0 was tagged but never published.

**Changed:**
- `sxr-core`: `extensions` is the one object-valued entity section
  (`EnvelopeError::ExtensionsNotObject` otherwise).
- `sxr-cli`: `sxr ingest --extensions <json-object>`; rejects non-objects and the
  reserved `domainforge` key; carried identically through direct commit, daemon
  (`--async-mode`) and spool (`--spool` → `spool drain`, stored in the spool
  request metadata). Not part of the idempotency key (run + verifier), so a
  duplicate delivery keeps the first committed record.
- Release workflow: gate on `releases_created`; tag/version from the
  `sxr-cli--` path-prefixed outputs.

**Verification:**
- New tests: `slice6::ingest_extensions_{round_trip_and_duplicate_delivery,
  rejects_invalid_input,survive_spool_drain,survive_daemon_queue}`,
  `envelope_build_tests::extensions_section_is_an_object_per_schema`.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --all-targets`: pass (9 consecutive clean runs).
- `cargo publish --workspace --dry-run`: all six crates package and verify.

**Unresolved difference:** one early full-suite run reported 3 failure-pattern lines
that did not reproduce in 9 later runs and were not captured; an intermittent test
may exist in the workspace. Spool drain accepts only cargo-json streams, so
non-cargo payloads cannot use `--spool` (pre-existing, unchanged).
