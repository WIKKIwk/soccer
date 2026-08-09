# COR-002 — Effective canonical event-stream builder

**Status:** Implemented and clean-database replay verified on 2026-08-09  
**Decision date:** 2026-08-09  
**Depends on:** [COR-001 correction reason and immutable void event](COR-001-correction-reason-and-void-event.md), [ARC-002 partial event sourcing](ARC-002-partial-event-sourcing.md), and [ARC-008 command, event, and version semantics](ARC-008-command-event-version-semantics.md)

## Decision

`EffectiveEventStream<T>` is a derived view over a validated
`CanonicalEventStream<T>`. It preserves the original event envelopes and their
canonical aggregate versions. It does not mutate or resequence canonical
evidence. Consequently, a voided event can leave an expected version gap in
the effective view; `canonical_version()` remains the latest version of the
source stream.

The builder currently resolves the envelope-level correction references that
are frozen by the event contract:

- an `EventVoided` marker is excluded from the effective view together with
  its `voids_event_id` target;
- an event with `supersedes_event_id` remains as the replacement and excludes
  its superseded target;
- all referenced targets must exist in the same canonical match stream;
- self-references, malformed void references, and cyclic correction chains are
  rejected;
- original source envelopes remain available through the canonical stream for
  audit and review.

Reference effects are cumulative: excluding a correction marker does not
rewrite or restore the historical event it referenced. A later correction must
append another canonical event. Typed `EventCorrected` replacement payloads,
authorization, persistence codecs, and projection reduction remain separate
increments.

## Domain API

The match engine exports:

- `EffectiveEventStream<T>`;
- `CorrectionReferenceKind`;
- `EffectiveEventStreamError`.

The builder is generic over the payload so it can operate on a raw persisted
envelope before a typed C1 codec is selected. The resulting view is ordered by
the canonical source order and contains no synthetic event or fabricated
payload.

## Verification

Database-free tests cover an untouched stream, a voided source event, a
replacement through `supersedes_event_id`, canonical-version preservation,
missing targets, non-void events carrying `voids_event_id`, self-reference,
conflicting void/supersede metadata, and cyclic correction chains. The typed
projection consumes this effective view, and the mixed persistence fixture
uses it after loading a correction-containing canonical stream.
`cargo test -p football-domain --lib` passes with 109 tests. No database rows
or external state are changed by default verification.

The clean correction fixture loaded the persisted mixed C1/correction stream,
built the effective view, and passed it into the typed projection after commit.
Authorization and transport exposure remain later boundaries.
