# CLI-001 — `footballctl` configuration and authorization adapter

**Status:** Implemented; typed configuration, development authorization, and failure-boundary tests verified
**Decision date:** 2026-08-14
**Depends on:** [FND-008 typed configuration](FND-008-typed-configuration.md), [FND-009 required environment values](FND-009-required-environment-values.md), and the existing CLI persistence decisions

## Decision

`footballctl` keeps the platform `ApplicationConfig` as the source of truth
for environment parsing and validation. The CLI process boundary wraps it in
`CliRuntimeConfig`, which owns the shared database-pool configuration and
connection path used by persistence commands.

Development-only persistence commands use one typed `AuthorizationScope`
adapter. Each scope preserves its existing environment variable and requires
the exact confirmation value `1`. This is a local development write guard,
not a substitute for application or API authentication.

Existing command names, authorization messages, configuration errors, and
database error mapping remain unchanged. The adapter does not run migrations
implicitly and does not open a database connection when authorization or
configuration validation has already failed.

## Implementation

- `apps/footballctl/src/config.rs` centralizes `ApplicationConfig` loading and
  `DatabasePoolConfig` connection setup.
- `apps/footballctl/src/auth.rs` defines the five existing development
  authorization scopes and their exact environment names.
- `apps/footballctl/src/main.rs` routes every persistence command through those
  adapters while preserving the existing `CliError` surface.

## Verification

`cargo test --locked -p footballctl` passed with 24 unit tests and 4 focused
CLI adapter subprocess tests; 7 database-backed integration tests remained
ignored because they require an explicitly approved clean target. The focused
tests cover every authorization boundary, missing and empty `DATABASE_URL`,
invalid optional bind configuration, and exact legacy error messages.
