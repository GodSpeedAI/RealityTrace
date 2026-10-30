# Changelog

## [0.4.0](https://github.com/GodSpeedAI/RealityTrace/compare/realitytrace-v0.3.0...realitytrace-v0.4.0) (2026-10-30)


### Features

* **ingest:** agent-event-v1 ingest, journey_check events, and ledger tail-verify fix ([#13](https://github.com/GodSpeedAI/RealityTrace/issues/13)) ([b616335](https://github.com/GodSpeedAI/RealityTrace/commit/b616335136bf363ab5b4215a6b74e17575b0e66f))
* **packaging:** precompiled native binaries for Linux, macOS, and Windows ([#12](https://github.com/GodSpeedAI/RealityTrace/issues/12)) ([4d154b1](https://github.com/GodSpeedAI/RealityTrace/commit/4d154b12da03243cf813822218110dc1632dc9da))
* **sxr-df:** fail closed on unsupported DomainForge compiler version ([7e7b1ab](https://github.com/GodSpeedAI/RealityTrace/commit/7e7b1ab5bd5927b3a2ef761f4cbab113475685b7))
* **verify:** persist when a verifier ran (started_at, completed_at) ([#10](https://github.com/GodSpeedAI/RealityTrace/issues/10)) ([1234b2b](https://github.com/GodSpeedAI/RealityTrace/commit/1234b2b9b19b05955fc2486f212ae59811b0c3ea))


### Bug Fixes

* **ledger:** read-only ledger commands never create a ledger ([#6](https://github.com/GodSpeedAI/RealityTrace/issues/6)) ([1c9f220](https://github.com/GodSpeedAI/RealityTrace/commit/1c9f2209dc58d31d9f1bc19ce51da6b5efbe235b))
* **verify:** run the verifier the plan names; add command@1 and a target digest ([#9](https://github.com/GodSpeedAI/RealityTrace/issues/9)) ([89c1ea4](https://github.com/GodSpeedAI/RealityTrace/commit/89c1ea448b7bb2aa56a4e5e2449554ec45f96435))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * sxr-core bumped from 0.3.0 to 0.4.0
    * sxr-ledger bumped from 0.2.1 to 0.3.0
    * sxr-git bumped from 0.2.1 to 0.2.2
    * sxr-df bumped from 0.2.1 to 0.3.0
    * sxr-verify bumped from 0.2.1 to 0.3.0

## [0.3.0](https://github.com/GodSpeedAI/RealityTrace/compare/realitytrace-v0.2.0...realitytrace-v0.3.0) (2026-09-25)


### Features

* **ingest:** accept caller CEP extensions via --extensions ([6b9e77f](https://github.com/GodSpeedAI/RealityTrace/commit/6b9e77fd5cd89ee0a3955ff2bf0196d5cc057013))
* **ingest:** accept caller CEP extensions via --extensions ([8c8edbb](https://github.com/GodSpeedAI/RealityTrace/commit/8c8edbb8e549318fb8196deb4bba186148d5aa5c))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * sxr-core bumped from 0.2.0 to 0.3.0
    * sxr-ledger bumped from 0.2.0 to 0.2.1
    * sxr-git bumped from 0.2.0 to 0.2.1
    * sxr-df bumped from 0.2.0 to 0.2.1
    * sxr-verify bumped from 0.2.0 to 0.2.1

## [0.2.0](https://github.com/GodSpeedAI/RealityTrace/compare/realitytrace-v0.1.0...realitytrace-v0.2.0) (2026-09-25)


### Features

* introduce graft CLI tool for codebase navigation and add project-level configuration ([f7fb1c3](https://github.com/GodSpeedAI/RealityTrace/commit/f7fb1c307e1f7ceb15468986330e5ca8c9248d19))
* **release:** configure RealityTrace packaging, crates.io publishing, and Release Please workflow ([bce6378](https://github.com/GodSpeedAI/RealityTrace/commit/bce6378c83b49d03369767f70512c9c92258cc02))


### Bug Fixes

* **release:** specify explicit path and version for workspace dependencies to enable automated release-please bumping ([8e9fce9](https://github.com/GodSpeedAI/RealityTrace/commit/8e9fce97b1b16afe57e607241342b56069d16ec9))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * sxr-core bumped from 0.1.0 to 0.2.0
    * sxr-ledger bumped from 0.1.0 to 0.2.0
    * sxr-git bumped from 0.1.0 to 0.2.0
    * sxr-df bumped from 0.1.0 to 0.2.0
    * sxr-verify bumped from 0.1.0 to 0.2.0
