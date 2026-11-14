# sxr-observed

WP-16 and WP-17. Library behind `realitytrace observed ingest`, `derive` and `compare`. No I/O except `df_cli`, which spawns the DomainForge binary for `evaluate`.

- `ingest`: classifies a CEP `execution_trace` envelope carrying `extensions["godspeed.interaction_event"]` as accepted or rejected (`INGEST_REJECTED_IDENTITY`, `INGEST_REJECTED_EVIDENCE`, plus `INGEST_REJECTED_SCHEMA`, `INGEST_REJECTED_SIGNATURE`). A signature-check hook is a trait; the S1 default accepts unsigned events and records them as `unsigned`, never as verified.
- `derive`: verifies the ledger hash chain (`sxr_core::chain::chain_verify`), then computes the representation of `contracts/observed-representation.v1.json`. Failure to verify is `CHAIN_VERIFY_FAILED`. Output is canonical JSON, byte-identical for the same ledger and selectors.
- `canon`: guards the I-JSON domain (CEP-0008 section 16.5) and delegates to `sxr_core::canonical_json`.

`schemas/godspeed.interaction_event.v1.json` is vendored from CEP (`schemas/extensions/`); a test pins its SHA-256.

- `compare` (WP-17): the representation against the declared expectations, as `residual.v1` records in separate episode and structural lists; policy expectations through DomainForge `evaluate` over a generated, hashed facts file; see `contracts/README.md` sections 4 and 5.
- `tail`: expected-head check that closes the deletion-of-the-last-record gap of the hash chain.
- `df_cli`: `CliEvaluator`, the `evaluate` port over the CLI binary. A binary without `evaluate` is refused; `validate` is never a fallback.
