-- /thumbs looks songs up by art key, fifty keys a request, and the apps ask
-- for every cover in the library on their first start: without an index
-- each request scanned the whole table (docs/plans/036_fast_at_100k.md).
CREATE INDEX songs_art ON songs (art) WHERE art != '';
