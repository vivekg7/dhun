package library

import (
	"slices"
	"testing"
)

// books groups items as audiobooks and returns each book's title and its
// files' paths in order.
func books(kind string, items []*groupItem) map[string][]string {
	group(kind, items)
	slices.SortFunc(items, func(a, b *groupItem) int { return a.newIndex - b.newIndex })
	out := map[string][]string{}
	keys := map[string]string{}
	for _, it := range items {
		if k, ok := keys[it.newTitle]; ok && k != it.newKey {
			out[it.newTitle+" (another)"] = append(out[it.newTitle+" (another)"], it.path)
			continue
		}
		keys[it.newTitle] = it.newKey
		out[it.newTitle] = append(out[it.newTitle], it.path)
	}
	return out
}

// The shapes of the owner's Audiobooks folder, 2026-10-10: books side by side
// in one folder, a book in parts tagged "pt N", and an untagged book whose
// parts sit in "(N of 3)" folders.
func TestBooksFromTheShapesOnTheNAS(t *testing.T) {
	got := books(KindAudiobook, []*groupItem{
		{id: 1, path: "Dune/Dune 2 - Dune Messiah.mp3", album: "Dune Messiah: Dune, Book 2"},
		{id: 2, path: "Dune/Dune 1 - Dune.mp3", album: "Dune: Dune, Book 1"},
		{id: 3, path: "Mistborn/Book 2/b.mp3", album: "Mistborn 02 - The Well of Ascension  pt 1", track: 1},
		{id: 4, path: "Mistborn/Book 2/a.mp3", album: "Mistborn 02 - The Well of Ascension  pt 2", track: 1},
		{id: 5, path: "Mistborn/Book 1/Mistborn 1 (2 of 3)/MISTBORN0102P01.mp3"},
		{id: 6, path: "Mistborn/Book 1/Mistborn 1 (1 of 3)/MISTBORN0101P10.mp3"},
		{id: 7, path: "Mistborn/Book 1/Mistborn 1 (1 of 3)/MISTBORN0101P2.mp3"},
		{id: 8, path: "Amish/The Secret of the Nagas.mp3", album: "Secret of the Nagas"},
		{id: 9, path: "Amish/The Immortals of Meluha.mp3", album: "Shiva Trilogy"},
	})
	want := map[string][]string{
		"Dune: Dune, Book 1":                  {"Dune/Dune 1 - Dune.mp3"},
		"Dune Messiah: Dune, Book 2":          {"Dune/Dune 2 - Dune Messiah.mp3"},
		"Mistborn 02 - The Well of Ascension": {"Mistborn/Book 2/b.mp3", "Mistborn/Book 2/a.mp3"},
		"Book 1":                              {"Mistborn/Book 1/Mistborn 1 (1 of 3)/MISTBORN0101P2.mp3", "Mistborn/Book 1/Mistborn 1 (1 of 3)/MISTBORN0101P10.mp3", "Mistborn/Book 1/Mistborn 1 (2 of 3)/MISTBORN0102P01.mp3"},
		"Secret of the Nagas":                 {"Amish/The Secret of the Nagas.mp3"},
		"Shiva Trilogy":                       {"Amish/The Immortals of Meluha.mp3"},
	}
	for title, paths := range want {
		if !slices.Equal(got[title], paths) {
			t.Errorf("%s: got %q, want %q", title, got[title], paths)
		}
	}
	if len(got) != len(want) {
		t.Errorf("got %d books, want %d: %v", len(got), len(want), got)
	}
}

// Episodes the owner's tool has not tagged yet must stay in the same show as
// the tagged ones, not become a second show named after the folder.
func TestShowIsTheFolderNamedByItsAlbumTag(t *testing.T) {
	items := []*groupItem{
		{id: 1, path: "Lex Fridman/405_Jeff Bezos_ Amazon.opus"},
		{id: 2, path: "Lex Fridman/002 - Christof Koch - Consciousness.opus", album: "Lex Fridman Podcast", date: "2018-05-01"},
		{id: 3, path: "Lex Fridman/001 - Max Tegmark - Life 3.0.opus", album: "Lex Fridman Podcast", date: "2018-04-19"},
	}
	got := books(KindPodcast, items)
	want := []string{"Lex Fridman/405_Jeff Bezos_ Amazon.opus", "Lex Fridman/001 - Max Tegmark - Life 3.0.opus", "Lex Fridman/002 - Christof Koch - Consciousness.opus"}
	if !slices.Equal(got["Lex Fridman Podcast"], want) || len(got) != 1 {
		t.Errorf("got %v, want one show \"Lex Fridman Podcast\" ordered %q", got, want)
	}
}
