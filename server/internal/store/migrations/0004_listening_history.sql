-- Each row of plays is one listen, however short, kept for a future
-- recommender (docs/plans/008_listening_history.md). `at` is when it started;
-- these add how it went. Play counts are a query over the rows that count.
ALTER TABLE plays ADD COLUMN ended_at   TEXT    NOT NULL DEFAULT '';
ALTER TABLE plays ADD COLUMN utc_offset INTEGER;          -- minutes, the device's then
ALTER TABLE plays ADD COLUMN from_ms    INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plays ADD COLUMN to_ms      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE plays ADD COLUMN end_reason TEXT    NOT NULL DEFAULT '';
ALTER TABLE plays ADD COLUMN source     TEXT    NOT NULL DEFAULT '';
ALTER TABLE plays ADD COLUMN queue_id   TEXT    NOT NULL DEFAULT '';
ALTER TABLE plays ADD COLUMN shuffle    INTEGER;
CREATE INDEX plays_user_at ON plays (user_id, at);
