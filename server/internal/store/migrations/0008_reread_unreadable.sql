-- Read once more every song the tag reader could not read: WebM (Matroska)
-- saved under an .opus name, which taglib has no parser for, was indexed
-- with no format and a duration of 0 (docs/plans/005_storage_and_library_model.md).
-- The scanner skips a file whose size and mtime are unchanged, so those rows
-- would never be read again on their own. A modification time that matches
-- no file makes the next scan re-read them, keeping their IDs; the apps get
-- the fixed rows through the usual library delta.
UPDATE songs SET mtime_ns = 0 WHERE format = '';
