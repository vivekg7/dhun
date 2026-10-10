package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.Fold
import io.github.vivekg7.dhun.data.LyricsRow
import io.github.vivekg7.dhun.data.SearchIndex
import io.github.vivekg7.dhun.data.SearchQuery
import io.github.vivekg7.dhun.data.Song
import io.github.vivekg7.dhun.data.searchKept
import org.junit.Assert.assertEquals
import org.junit.Test
import java.io.File

class SearchTest {
    private fun song(
        id: Long,
        title: String,
        artist: String = "",
        album: String = "",
        composer: String = "",
        year: Int = 0,
    ) = Song(id, "a/$id.mp3", title, artist, artist, album, "", composer, "", year, 0, 0, 180_000, "mpeg", 0, 0, 0, 0, false, false, 0, false)

    private val songs =
        listOf(
            song(1, "Tum Hi Ho", "Arijit Singh", "Aashiqui 2", year = 2013),
            song(2, "Live and Let Die", "Wings"),
            song(3, "Love Me Do", "The Beatles"),
            song(4, "दिल से", "A.R. Rahman", "दिल से", composer = "A.R. Rahman", year = 1998),
            song(5, "Highway to Hell", "AC/DC"),
            song(6, "Kabhie Kabhie", "Mukesh", year = 1976),
            song(7, "Hello", "Adele", "Tum Hi Ho Hits"),
        )
    private val index = SearchIndex.songs(songs)

    private fun find(q: String) = index.search(SearchQuery(q)).map { it.first.id }

    // The server's copy (server/internal/library/search.go) reads the same file.
    @Test
    fun foldsAsTheServerDoes() {
        val lines = File("../../api/search-fold.tsv").readLines().filter { it.isNotBlank() && !it.startsWith("#") }
        for (line in lines) {
            val (text, words, keys) = line.split('\t')
            assertEquals(text, words, Fold.text(text))
            assertEquals(text, keys, Fold.words(text).joinToString(" ") { Fold.key(it) })
        }
    }

    @Test
    fun wordsMatchAcrossFieldsInAnyOrder() {
        assertEquals(listOf(1L), find("arijit tum"))
        assertEquals(listOf(1L), find("ho tum arijit"))
    }

    @Test
    fun theTitleRanksAboveTheAlbum() {
        // Song 7 only has it in its album name.
        assertEquals(listOf(1L, 7L), find("tum hi ho"))
    }

    @Test
    fun typosOnlyWhenNothingBetterMatches() {
        assertEquals(listOf(1L), find("arjit"))
        // "love" matches a word, so "live" is not offered as a typo.
        assertEquals(listOf(3L), find("love"))
    }

    @Test
    fun spellingAndScriptsMeet() {
        assertEquals(listOf(4L), find("dil se"))
        assertEquals(listOf(6L), find("kabhi"))
        assertEquals(listOf(5L), find("acdc"))
    }

    @Test
    fun filtersNarrowAWordToAField() {
        assertEquals(listOf(4L), find("composer:rahman"))
        assertEquals(emptyList<Long>(), find("title:arijit"))
        assertEquals(listOf(4L, 6L), find("year:1970-2000").sorted())
        assertEquals(listOf(1L), find("artist:\"arijit singh\" tum"))
    }

    @Test
    fun keptLyricsAreSearchedLineByLine() {
        val rows =
            listOf(
                LyricsRow(1, "[00:01.00]Hum tere bin ab reh nahi sakte\n[00:05.00]Tere bina kya wajood mera", 0),
                LyricsRow(2, "Nothing here\nreh gaye", 0),
            )
        val hits = searchKept(rows, SearchQuery("tere bina"))
        assertEquals(listOf(1L), hits.map { it.song })
        assertEquals("Tere bina kya wajood mera", hits[0].line)
        // Words spread over two lines are not a match.
        assertEquals(emptyList<Long>(), searchKept(rows, SearchQuery("nothing gaye")).map { it.song })
    }
}
