package io.github.vivekg7.dhun

import io.github.vivekg7.dhun.ui.guideBlocks
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** How it works (docs/plans/034_settings_text_defaults_reset.md). */
class GuideTest {
    private val guide = guideBlocks(File("src/main/assets/guide.md").readText())

    @Test
    fun everyInfoButtonOpensAHeadingTheGuideHas() {
        // An ⓘ naming a heading that was renamed would open the guide at the top, silently.
        val asked =
            Regex("""(?:openGuide\(|guide = )"([^"]+)"""")
                .findAll(File("src/main/java/io/github/vivekg7/dhun/ui/Settings.kt").readText())
                .map { it.groupValues[1] }
                .toSet()
        assertTrue(asked.size >= 5)
        val headings = guide.filter { it.first == 3 }.map { it.second }.toSet()
        assertEquals(emptySet<String>(), asked - headings)
    }

    @Test
    fun aWrappedListItemStaysOneItem() {
        val items = guideBlocks("- **Bar:** a strip\n  along the bottom.\n- **None:** nothing.\n\nAfter.")
        assertEquals(listOf(-1 to "**Bar:** a strip along the bottom.", -1 to "**None:** nothing.", 0 to "After."), items)
    }
}
