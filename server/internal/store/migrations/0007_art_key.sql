-- Which cover a song shows, as a key the apps keep covers by
-- (docs/plans/019_networking_and_caching.md). Songs that show the same
-- cover share it, so an album's cover is fetched and kept once; a new cover
-- gets a new key. Embedded art: a hash of the image. A folder's cover file:
-- a hash of its path, size and modification time. Empty without art.
ALTER TABLE songs ADD COLUMN art TEXT NOT NULL DEFAULT '';
