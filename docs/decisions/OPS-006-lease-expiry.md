# OPS-006: operator lease expiry policy

Status: implemented

## Decision

An operator lease expiry is generated from an explicit positive whole-second
duration supplied by the command boundary. The duration is not inferred from
the match, operator identity, wall-clock timezone, or a missing command flag.
The current C1 contract intentionally has no silent default and no
pilot-specific maximum; those values require measured live-operation evidence
and a later decision record.

The expiry rules are:

- `claimed_at`, heartbeat time, and `expires_at` are UTC instants;
- `expires_at` is calculated from the supplied operation time plus the
  positive duration;
- a lease is active only when `expires_at > observed_at`;
- `expires_at == observed_at` is already expired;
- an expired lease cannot be heartbeated back to life;
- claim and takeover use the same active/expired boundary;
- time-range overflow is rejected instead of wrapping or inventing a value.

The policy is owned by
`football_domain::OperatorLeaseExpiryPolicy`. `footballctl match claim`,
`heartbeat`, and `takeover` all use that policy to calculate their expiry.
The repository uses the domain lease boundary for active-lease conflict
checks, so the adapter and persistence paths cannot disagree at the exact
expiry instant.

## Verification

Domain tests cover positive-duration validation, UTC normalization, the exact
expiry boundary, time-range overflow, and the existing heartbeat invariants.
The footballctl parser tests cover positive, zero, and non-numeric durations.
The assignment integration flow was rerun against a disposable
`football_socket_ops006` database and verified claim, heartbeat, release,
active-takeover rejection, expiry-based takeover, and persisted read-back.
The disposable database was dropped after the test; the persistent
`football_socket_test` database was not modified.
