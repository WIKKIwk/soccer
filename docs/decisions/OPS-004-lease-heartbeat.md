# OPS-004: operator lease heartbeat

Status: implemented

## Decision

Heartbeat is a renewal of the existing operator lease. It does not create a
second lease, change the lease ID, change the match assignment, or change the
original `claimed_at` timestamp. The caller supplies a positive lease duration;
the command calculates a new UTC expiry from the heartbeat time.

The development adapter is:

```text
ALLOW_DEVELOPMENT_OPERATIONS_SEED=1 \
DATABASE_URL=<approved-target> \
footballctl match heartbeat \
  --match-id <uuidv7> \
  --operator-id <uuidv7> \
  --lease-id <uuidv7> \
  --lease-seconds <positive-integer>
```

The repository performs the renewal in one transaction:

1. lock and load the match assignment;
2. require the requested operator to be the assigned active operator;
3. lock and load the current lease for the match;
4. require the requested lease ID and operator to own that lease;
5. apply the domain heartbeat invariants;
6. update only `expires_at` and read the row back before commit.

The domain rejects a heartbeat before claim time, at or after the current
expiry, an expiry that is not after the heartbeat time, or an expiry that does
not extend the current lease. Expired-lease takeover is defined by OPS-005 and
the shared expiry boundary in OPS-006; heartbeat never revives an expired
lease.

## Verification

The CLI unit suite passed with the `match heartbeat` parser. The ignored
assignment/claim integration test ran against the disposable
`football_socket_ops004` database and verified a successful renewal, unchanged
lease identity and claim time, extended expiry, and rejection of a heartbeat
for a different lease ID. Domain unit tests cover backwards, expired, invalid,
and non-extending heartbeat timestamps. The disposable database was dropped
after the test.
