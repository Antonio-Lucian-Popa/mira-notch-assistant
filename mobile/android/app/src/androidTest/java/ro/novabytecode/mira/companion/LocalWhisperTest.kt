package ro.novabytecode.mira.companion

import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** Synthesized public test phrases only. No microphone and no mutation of user reminders. */
@RunWith(AndroidJUnit4::class)
class LocalWhisperTest {
    @Test fun transcribesRomanianCommandsWithTheLocalModel() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertTrue("Download the local model first", LocalVoiceModel.ready(context))
        val initialized = CountDownLatch(1)
        var status = -1
        val tts = TextToSpeech(context) { status = it; initialized.countDown() }
        assertTrue(initialized.await(15, TimeUnit.SECONDS))
        assertEquals(TextToSpeech.SUCCESS, status)
        val voice = tts.voices?.firstOrNull { it.locale.language == "ro" && !it.isNetworkConnectionRequired }
        assumeTrue("No offline Romanian TTS voice available for the test fixture", voice != null)
        tts.voice = voice
        tts.setSpeechRate(0.85f)
        val engine = LocalWhisper({}, {}, {}, {})
        var handle = 0L
        val report = JSONArray()
        try {
            handle = engine.load(LocalVoiceModel.file(context).absolutePath)
            assertTrue(handle != 0L)
            for ((phrase, expected) in listOf("Mira, pornește focus." to "focusStart", "Mira, am băut apă." to "water", "Mira, pornește transcrierea." to "dictate", "Mira gata." to "stop", "Mira." to "wake", "Mâine merg la plimbare." to "")) {
                val wav = File(context.cacheDir, "local-voice-test.wav")
                val done = CountDownLatch(1)
                var error = false
                tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                    override fun onStart(id: String?) {}
                    override fun onDone(id: String?) { done.countDown() }
                    @Deprecated("Legacy TTS callback")
                    override fun onError(id: String?) { error = true; done.countDown() }
                })
                assertEquals(TextToSpeech.SUCCESS, tts.synthesizeToFile(phrase, Bundle(), wav, expected))
                assertTrue(done.await(20, TimeUnit.SECONDS)); assertFalse(error)
                val pcm = readWave(wav)
                val began = System.currentTimeMillis()
                val segmenter = SpeechSegmenter()
                val segments = mutableListOf<FloatArray>()
                val padded = pcm + FloatArray(16000)
                for (offset in padded.indices step 320) {
                    val frame = ShortArray(320) { i -> if (offset + i < padded.size) (padded[offset + i] * 32767).toInt().coerceIn(-32768, 32767).toShort() else 0 }
                    segmenter.accept(frame)?.let { segments.add(it) }
                }
                assertTrue("Speech gate discarded the fixture", segments.isNotEmpty())
                val recognized = segments.joinToString(" ") { engine.transcribe(handle, it, true) }
                val command = VoiceRules.wake(recognized)?.let { VoiceRules.command(it) }
                report.put(JSONObject().put("phrase", phrase).put("recognized", recognized).put("command", command)
                    .put("milliseconds", System.currentTimeMillis() - began))
                File(context.getExternalFilesDir(null), "local-voice-test.json").writeText(report.toString(2))
                if (expected == "wake") assertEquals("Recognized: $recognized", "", VoiceRules.wake(recognized))
                else if (expected.isEmpty()) assertNull("Unexpected wake: $recognized", VoiceRules.wake(recognized))
                else assertEquals("Recognized: $recognized", expected, command)
                wav.delete()
            }
            // Silence must never become a wake command through model hallucination.
            val silence = engine.transcribe(handle, FloatArray(16000 * 2))
            assertNull(VoiceRules.wake(silence))
        } finally {
            if (handle != 0L) engine.release(handle)
            tts.shutdown(); File(context.cacheDir, "local-voice-test.wav").delete()
        }
    }
    private fun readWave(file: File): FloatArray {
        val b = ByteBuffer.wrap(file.readBytes()).order(ByteOrder.LITTLE_ENDIAN)
        check(String(b.array(), 0, 4) == "RIFF")
        var rate = 0; var channels = 0; var bits = 0; var format = 0
        var dataOffset = 0; var dataSize = 0
        var offset = 12
        while (offset + 8 <= b.limit()) {
            val name = String(b.array(), offset, 4); val size = b.getInt(offset + 4)
            if (name == "fmt ") { format = b.getShort(offset + 8).toInt(); channels = b.getShort(offset + 10).toInt(); rate = b.getInt(offset + 12); bits = b.getShort(offset + 22).toInt() }
            if (name == "data") { dataOffset = offset + 8; dataSize = size; break }
            offset += 8 + size + (size % 2)
        }
        check(format == 1 && bits == 16 && channels > 0 && rate > 0 && dataOffset > 0)
        val frames = dataSize / (channels * 2)
        val source = FloatArray(frames) { i -> (0 until channels).sumOf { b.getShort(dataOffset + (i * channels + it) * 2).toDouble() }.toFloat() / (channels * 32768f) }
        return FloatArray(frames * 16000 / rate) { i ->
            val x = i.toDouble() * rate / 16000
            val a = x.toInt().coerceAtMost(source.lastIndex)
            val next = (a + 1).coerceAtMost(source.lastIndex)
            (source[a] + (source[next] - source[a]) * (x - a)).toFloat()
        }
    }
}
