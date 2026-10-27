package ro.novabytecode.mira.companion

import org.junit.Assert.*
import org.junit.Test

class SpeechSegmenterTest {
    @Test fun silenceAndSingleClicksDoNotTriggerInference() {
        val s = SpeechSegmenter()
        repeat(300) { assertNull(s.accept(ShortArray(320))) }
        assertNull(s.accept(ShortArray(320) { 8000 }))
        repeat(60) { assertNull(s.accept(ShortArray(320))) }
    }
    @Test fun speechIsEmittedAfterSilenceWithBoundedPreRoll() {
        val s = SpeechSegmenter()
        repeat(30) { s.accept(ShortArray(320)) }
        repeat(20) { assertNull(s.accept(ShortArray(320) { if (it % 2 == 0) 3000 else -3000 })) }
        var chunk: FloatArray? = null
        repeat(35) { s.accept(ShortArray(320))?.let { chunk = it } }
        assertNotNull(chunk)
        assertTrue(chunk!!.size in 8000..18000)
        assertTrue(chunk!!.all { it in -1f..1f })
    }
    @Test fun continuousSpeechNeverGrowsTheBufferWithoutLimit() {
        val s = SpeechSegmenter(); val chunks = mutableListOf<FloatArray>()
        repeat(1000) { s.accept(ShortArray(320) { 3000 })?.let { chunks.add(it) } }
        assertTrue(chunks.size >= 2)
        assertTrue(chunks.all { it.size <= 16000 * 9 })
    }
}
