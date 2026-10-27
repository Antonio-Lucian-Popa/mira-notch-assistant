package ro.novabytecode.mira.companion

import java.text.Normalizer

/** Whitelisted intents: speech can never become executable code or a shell command. */
object VoiceRules {
    fun normalized(text: String) = Normalizer.normalize(text.lowercase(java.util.Locale.ROOT), Normalizer.Form.NFD)
        .replace(Regex("\\p{M}+"), "").replace(Regex("[^a-z0-9 ]"), " ").replace(Regex("\\s+"), " ").trim()
    fun wake(text: String): String? {
        val n = normalized(text)
        return Regex("^(?:hey |hei )?mira(?: (.*))?$").matchEntire(n)?.groupValues?.get(1)
    }
    fun command(text: String): String? = when (normalized(text)) {
        "porneste transcrierea", "incepe transcrierea", "transcriere" -> "dictate"
        "porneste focus", "porneste pomodoro" -> "focusStart"
        "pune pauza", "pauza focus" -> "focusPause"
        "continua focus" -> "focusResume"
        "opreste focus" -> "focusStop"
        "adauga un pahar de apa", "am baut apa", "un pahar de apa" -> "water"
        "gata", "stop", "opreste ascultarea" -> "stop"
        else -> null
    }
}

/** Keep the last three words uncommitted so the stop phrase can span ASR chunks. */
class DictationBuffer {
    private val committed = StringBuilder()
    private var carry = ""
    var stopped = false; private set
    private val stop = Regex("\\b(?:(?:hey|hei)\\s+)?mira[\\s,.:;!?—-]+gat[aă]\\b[.!?]*", RegexOption.IGNORE_CASE)
    fun accept(text: String): Boolean {
        if (stopped) return true
        val combined = listOf(carry, text.trim()).filter { it.isNotEmpty() }.joinToString(" ")
        val match = stop.find(combined)
        if (match != null) {append(combined.substring(0, match.range.first));carry = "";stopped = true;return true}
        val words = combined.split(Regex("\\s+")).filter { it.isNotEmpty() }
        val cut = (words.size - 3).coerceAtLeast(0)
        append(words.take(cut).joinToString(" "));carry = words.drop(cut).joinToString(" ")
        return false
    }
    fun preview() = listOf(committed.toString().trim(), carry).filter { it.isNotEmpty() }.joinToString(" ")
    fun finish(): String {if (!stopped) {append(carry);carry = ""};return committed.toString().trim()}
    fun hasStop(text: String) = stop.containsMatchIn(listOf(carry, text).joinToString(" "))
    private fun append(text: String) {if (text.trim().isNotEmpty()) {if (committed.isNotEmpty()) committed.append('\n');committed.append(text.trim().take((300000-committed.length).coerceAtLeast(0)))}}
}
