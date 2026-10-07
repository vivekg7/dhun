-- Lines between entries in an .m3u8 (comments, #EXTGRP and other
-- directives) belong to the entry they precede, so an edit made through Dhun
-- keeps them with that entry instead of dropping them (plan 005).
ALTER TABLE playlist_items ADD COLUMN lines_before TEXT NOT NULL DEFAULT '';
