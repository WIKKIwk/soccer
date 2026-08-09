# FND-009 — Required environment values

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [FND-008 typed process configuration](FND-008-typed-configuration.md)

## Validation contract

`ApplicationConfig::from_env()` reads the process environment at the binary
boundary. `DATABASE_URL` is required and must not be missing or whitespace-only.
`ADMIN_API_BIND_ADDR` is optional, but when present it must parse as a
`std::net::SocketAddr`; absence selects the documented local default.

Environment variables with non-Unicode values fail with a typed configuration
error. No database fallback, hard-coded credential, or silent alternate
environment is selected.

Error messages identify the configuration key and validation failure without
including the database URL value.

## Verification

Pure platform tests cover missing and empty `DATABASE_URL`, invalid bind
addresses, and the optional bind default. The process environment itself is not
mutated by unit tests, avoiding cross-test races.
