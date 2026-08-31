-- STD-005 gives every season an independent source revision for accepted
-- verified results. It starts at zero and advances only in the same
-- transaction as the verified-result write and its outbox message.
ALTER TABLE seasons
    ADD COLUMN results_revision BIGINT NOT NULL DEFAULT 0,
    ADD CONSTRAINT seasons_results_revision_nonnegative CHECK (results_revision >= 0);
