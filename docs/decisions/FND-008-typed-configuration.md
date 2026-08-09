# FND-008 — Typed process configuration

**Status:** Accepted  
**Decision date:** 2026-08-08

## Decision

Process configuration belongs to `football-platform::config`. Binaries consume
one `ApplicationConfig` value instead of reading environment variables in
handlers, repositories, or domain code.

The initial typed configuration contains only values already required by the
first skeleton:

- required `DATABASE_URL` as an owned string exposed through an accessor;
- optional `ADMIN_API_BIND_ADDR` parsed as `SocketAddr`;
- default bind address `127.0.0.1:3000` when the optional value is absent; and
- optional `RUST_LOG` filter with the default `info` value used by the
  observability boundary.

`TEST_DATABASE_URL` remains an integration-test-only input and is not part of
runtime application configuration. Pool sizing and migration execution remain
owned by their infrastructure operations and are not duplicated here.

The configuration type uses a manual `Debug` implementation that keeps the
database URL redacted while exposing non-sensitive bind-address information.
Broader log and error redaction remains subject to the later observability and
error increments.

## Verification

The platform unit tests build the typed configuration without mutating the
process environment and cover the default bind address and invalid raw values.
Required-environment behavior is recorded separately under FND-009.
