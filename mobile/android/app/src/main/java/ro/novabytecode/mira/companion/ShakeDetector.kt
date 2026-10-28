package ro.novabytecode.mira.companion

/** Three distinct strong movements; only individual peaks are debounced. */
internal class ShakeDetector {
    private var high = false
    private var first = 0L
    private var last = -1000L
    private var count = 0
    fun sample(g: Float, now: Long): Boolean {
        if (g < 1.5f) high = false
        if (g < 2.7f || high) return false
        high = true
        if (now - last < 120) return false
        if (now - first > 1200 || count == 0) { first = now; count = 0 }
        last = now
        count++
        if (count < 3) return false
        count = 0
        return true
    }
}
