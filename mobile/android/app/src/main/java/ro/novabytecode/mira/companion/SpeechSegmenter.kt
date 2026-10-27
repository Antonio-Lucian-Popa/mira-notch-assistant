package ro.novabytecode.mira.companion

import java.util.ArrayDeque
import kotlin.math.sqrt

/** Bounded PCM utterances: skip silence and keep a short lead-in so wake words aren't clipped. */
class SpeechSegmenter {
    private val lead = ArrayDeque<ShortArray>()
    private val samples = FloatArray(16000 * 9)
    private var used = 0
    private var voiced = 0
    private var quiet = 0
    private var onset = 0
    private var noise = 0.0015
    fun accept(frame: ShortArray): FloatArray? {
        val rms = sqrt(frame.sumOf { val x = it / 32768.0; x * x } / frame.size)
        val speech = rms > maxOf(0.008, noise * 3.5)
        if (used == 0) {
            lead.addLast(frame.copyOf()); if (lead.size > 15) lead.removeFirst()
            if (!speech) { onset = 0; noise = noise * 0.98 + minOf(rms, 0.006) * 0.02; return null }
            onset++
            if (onset < 2) return null
            lead.forEach { append(it) }; lead.clear(); voiced = onset; quiet = 0
        } else {
            append(frame)
            if (speech) { voiced++; quiet = 0 } else quiet++
        }
        if (quiet >= 30 || used >= 16000 * 8) {
            val end = (used - maxOf(0, quiet - 10) * frame.size).coerceAtLeast(1)
            val result = if (voiced >= 8) samples.copyOf(end) else null
            used = 0; voiced = 0; quiet = 0; onset = 0; lead.clear()
            return result
        }
        return null
    }
    private fun append(frame: ShortArray) {
        for (value in frame) { if (used >= samples.size) break; samples[used++] = value / 32768f }
    }
}
