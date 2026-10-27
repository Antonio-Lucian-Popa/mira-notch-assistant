package ro.novabytecode.mira.companion

/** Three separate strong movements, followed by a 15 second cooldown. */
internal class ShakeDetector {
    private var high = false
    private var first = 0L
    private var last = -1000L
    private var count = 0
    private var nextAllowed = 0L
    fun sample(g: Float, now: Long): Boolean {
        if (g < 1.5f) high = false
        if (g < 2.7f || high || now < nextAllowed) return false
        high = true
        if (now - last < 120) return false
        if (now - first > 1200 || count == 0) { first = now; count = 0 }
        last = now
        count++
        if (count < 3) return false
        count = 0
        nextAllowed = now + 15000
        return true
    }
}
