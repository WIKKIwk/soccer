# FND-010 — Secret redaction in configuration debug output

**Status:** Accepted  
**Decision date:** 2026-08-08  
**Depends on:** [FND-008 typed process configuration](FND-008-typed-configuration.md), [FND-009 required environment values](FND-009-required-environment-values.md)

## Decision

`ApplicationConfig` implements `Debug` manually. Its `database_url` field is
always rendered as `<redacted>`; only the non-sensitive admin API bind address
is rendered. The configuration type does not derive `Debug`, so adding a new
secret-bearing field requires an explicit review of this implementation.

The database URL accessor remains available to the infrastructure pool
constructor. This protects accidental structured/debug output; it does not make
it safe to explicitly log the accessor’s returned string.

## Verification

The platform test builds a configuration with a password-bearing URL and
asserts that the full URL and password are absent from `Debug` output while the
bind address and redaction marker remain present.

This increment covers configuration debug output only. Structured logging,
database error mapping, and broader sensitive-payload redaction remain separate
later increments.
