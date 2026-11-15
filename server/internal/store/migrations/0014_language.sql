-- A song's language as the curation tool names it ("Hindi"), "Instrumental"
-- for an instrumental track whatever language it is filed under, or '' when
-- the tool has none (docs/plans/041_search_filters_and_popularity.md).
ALTER TABLE songs ADD COLUMN language TEXT NOT NULL DEFAULT '';
