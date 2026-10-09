package io.github.vivekg7.dhun.ui

import androidx.compose.animation.AnimatedContentTransitionScope
import androidx.compose.animation.ContentTransform
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith

/**
 * The app's few motions, so every screen moves alike: a page opened
 * slides in from the side it is going to, a short way and fading, as
 * Android's own screens do; a page closed goes back the way it came.
 */
object Motion {
    const val SHORT = 150
    const val MEDIUM = 280

    /** A move across the whole screen, as a cover changing for the next song. */
    const val LONG = 360

    /** Opening a page ([forward]), or going back from one. */
    fun <S> AnimatedContentTransitionScope<S>.page(forward: Boolean): ContentTransform {
        val sign = if (forward) 1 else -1
        val spec = tween<androidx.compose.ui.unit.IntOffset>(MEDIUM, easing = FastOutSlowInEasing)
        return (slideInHorizontally(spec) { sign * it / 5 } + fadeIn(tween(MEDIUM))) togetherWith
            (slideOutHorizontally(spec) { -sign * it / 5 } + fadeOut(tween(SHORT)))
    }
}
