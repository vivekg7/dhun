-- A song's popularity, 1 to 100, from the curation tool's index in
-- _meta/index/; 0 when it has none (docs/plans/041_search_filters_and_popularity.md).
ALTER TABLE songs ADD COLUMN score INTEGER NOT NULL DEFAULT 0;

-- Genres no longer split on an "&" inside a word: "R&B/Soul" was stored as
-- the genres "R" and "B/Soul". As in 0008, a modification time that matches
-- no file makes the next scan read those songs again, keeping their IDs.
UPDATE songs SET mtime_ns = 0 WHERE genre GLOB '*[^ ]&*' OR genre GLOB '*&[^ ]*';
