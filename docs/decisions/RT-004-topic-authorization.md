# RT-004 — Realtime topic authorization

**Status:** Implemented; fail-closed public, operator, supervisor, and revoke-safe policy verification passed  
**Decision date:** 2026-08-09  
**Depends on:** [RT-001 realtime topic names](RT-001-realtime-topic-names.md), [RT-003 subscribe and unsubscribe contracts](RT-003-subscribe-unsubscribe-contract.md), [ARC-001 modular-monolith boundary](ARC-001-modular-monolith-boundary.md)

## Decision

Topic authorization is an application policy, not a WebSocket handler rule and
not a topic-ID guess. The adapter passes an authenticated principal and
authoritative access facts into the policy:

- `PublicClient` may subscribe only when the match is explicitly
  `public_readable`;
- `MatchDataOperator` may subscribe only when `operator_assigned` is true;
- `MatchDataSupervisor` may subscribe only when `supervisor_authorized` is
  true;
- a denied subscription returns a typed policy error;
- unsubscribe is always allowed because it only removes the requesting
  connection's local subscription and reveals no topic data.

Operator assignment, supervisor scope, and public visibility are facts owned
by future identity, match-assignment, and public-read query boundaries. The
policy never infers them from a UUID or silently treats a missing fact as
allowed. There is no anonymous access to a non-public match topic.

## Implementation

`football-application::realtime_authorization` owns the transport-independent
principal, access-fact, operation, and typed-error vocabulary. WebSocket
session/token extraction and the actual topic lookup remain adapter/auth
integration work; they are not fabricated in this increment.

## Verification

Application unit tests verify public-only access, operator assignment scope,
supervisor authorization scope, fail-closed denial, and safe unsubscribe after
access is revoked.
