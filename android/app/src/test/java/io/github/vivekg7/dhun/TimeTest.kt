package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.data.parseTime
import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.OffsetDateTime

class TimeTest {
    // The server's two forms (store.Now and sync.go's opTime) take the fast path; anything else the library.
    @Test
    fun serverTimesParseAsTheLibraryWould() {
        val times =
            listOf(
                "2026-10-20T05:23:36Z",
                "2026-10-20T05:23:36.250Z",
                "2028-02-29T00:00:00Z",
                "2026-10-20T05:23:36.123456789Z",
                "2026-10-20T10:53:36+05:30",
            )
        for (s in times) assertEquals(s, OffsetDateTime.parse(s).toInstant().toEpochMilli(), parseTime(s))
        assertEquals(0, parseTime(""))
        assertEquals(0, parseTime("yesterday"))
        // 2026 is not a leap year: no time, as the library would say.
        assertEquals(0, parseTime("2026-02-29T00:00:00Z"))
    }
}
