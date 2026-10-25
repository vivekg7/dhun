-- A queue holds at most 1,000 songs (docs/plans/040_queue_size_cap.md): a
-- longer one keeps the 1,000 from its current song on, wrapping round to its
-- start, as the server and the apps now trim a new one. Each owner's data
-- version moves on, so every device pulls the trimmed queue.
UPDATE users SET data_version = data_version + 1
WHERE id IN (SELECT user_id FROM queues WHERE deleted = 0 AND json_array_length(songs) > 1000);

UPDATE queues SET
    version = (SELECT data_version FROM users WHERE users.id = queues.user_id),
    songs = (
        SELECT json_group_array(value ORDER BY place) FROM (
            SELECT value, (key - coalesce((
                SELECT key FROM json_each(queues.songs) WHERE value = queues.current_song LIMIT 1
            ), 0) + json_array_length(queues.songs)) % json_array_length(queues.songs) AS place
            FROM json_each(queues.songs)
        ) WHERE place < 1000
    )
WHERE deleted = 0 AND json_array_length(songs) > 1000;
