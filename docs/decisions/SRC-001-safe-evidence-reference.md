# SRC-001 — Safe evidence reference

**Status:** Implemented; opaque evidence-reference value object and domain invariants verified
**Decision date:** 2026-08-09
**Depends on:** [P0-001 product contract](P0-001-product-contract.md), [QUA-001 provisional and verified match state](QUA-001-provisional-verified-state.md), and [QUA-004 reasoned match reopen](QUA-004-reasoned-match-reopen.md)

## Decision

The quality boundary now has a domain-owned `EvidenceReference` value object.
It is an opaque locator retained for later verification, audit, and quality
workflows. The domain does not interpret it as a URL, file path, redirect, or
fetch instruction. Persisting a reference therefore does not grant an adapter
permission to make a network or filesystem request.

The value is accepted only when it satisfies all of these invariants:

- it contains at least one non-whitespace character;
- its UTF-8 representation is at most 2,048 bytes;
- it contains no control characters;
- the accepted text is preserved exactly, without trimming, normalization, or
  truncation.

The value object intentionally does not select a source type or reliability
level. The P0 contract's official PFL protocol, result, disciplinary decision,
live broadcast, and on-site observation categories remain product authority
decisions; their typed taxonomy belongs to `SRC-002`. External lookup policy
and transport limits remain application concerns.

## Implementation

`football-domain::EvidenceReference` and
`football-domain::EvidenceReferenceError` live in
`crates/domain/src/evidence.rs` and are re-exported from the domain crate.
No database migration or canonical event payload change is made by this
increment. Later audit and source-linking increments can carry the typed value
without inventing an authority category or changing the frozen QUA-003 and
QUA-004 event schemas.

## Verification

- `cargo fmt --all -- --check` passed after formatting.
- `cargo test -p football-domain --lib` passed: 130 tests, 0 failures.
- The persistent PostgreSQL test database was not modified; no migration was
  required.
