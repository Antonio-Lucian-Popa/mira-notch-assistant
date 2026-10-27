package ro.novabytecode.mira.companion

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Handler
import android.os.Looper
import java.io.File
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** PCM lives only in a bounded in-memory queue. No provider, cloud API, or audio files. */
class LocalWhisper(private val onText: (String) -> Unit, private val onError: (String) -> Unit,
                   private val onReady: () -> Unit, private val onOverload: () -> Unit, private val commandMode: () -> Boolean = { true }) {
    companion object { init { System.loadLibrary("mira_whisper") } }
    external fun load(path: String): Long
    external fun transcribe(handle: Long, pcm: FloatArray, commandMode: Boolean = true): String
    external fun cancel(handle: Long)
    external fun release(handle: Long)
    private val running = AtomicBoolean(false)
    private val pointerLock = Any()
    private var handle = 0L
    @Volatile private var record: AudioRecord? = null
    private val ui = Handler(Looper.getMainLooper())
    private val decoder = Executors.newSingleThreadExecutor()
    private val microphone = Executors.newSingleThreadExecutor()
    private val queue = ArrayBlockingQueue<FloatArray>(2)
    fun start(model: File) {
        check(running.compareAndSet(false, true))
        decoder.execute {
            try {
                val loaded = load(model.absolutePath)
                synchronized(pointerLock) { handle = loaded }
                if (!running.get()) return@execute
                microphone.execute { capture() }
                ui.post { if (running.get()) onReady() }
                while (running.get()) {
                    val pcm = queue.poll(500, TimeUnit.MILLISECONDS) ?: continue
                    if (!running.get()) break
                    val text = transcribe(handle, pcm, commandMode()).trim()
                    if (text.isNotEmpty()) ui.post { if (running.get()) onText(text) }
                }
            } catch (error: Exception) { fail(error) }
            finally {
                synchronized(pointerLock) { if (handle != 0L) { release(handle); handle = 0 } }
            }
        }
    }
    @SuppressLint("MissingPermission")
    private fun capture() {
        var input: AudioRecord? = null
        try {
            val minimum = AudioRecord.getMinBufferSize(16000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            check(minimum > 0) { "Microfonul nu oferă audio compatibil." }
            input = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, 16000, AudioFormat.CHANNEL_IN_MONO,
                AudioFormat.ENCODING_PCM_16BIT, maxOf(minimum * 2, 6400))
            check(input.state == AudioRecord.STATE_INITIALIZED) { "Nu pot inițializa microfonul." }
            record = input
            if (!running.get()) return
            input.startRecording()
            check(input.recordingState == AudioRecord.RECORDSTATE_RECORDING) { "Microfonul este ocupat sau blocat de Android." }
            val segmenter = SpeechSegmenter()
            val frame = ShortArray(320)
            while (running.get()) {
                val count = input.read(frame, 0, frame.size, AudioRecord.READ_BLOCKING)
                if (count <= 0) { if (running.get()) error("Microfonul a fost întrerupt de Android."); break }
                val segment = segmenter.accept(if (count == frame.size) frame else frame.copyOf(count))
                if (segment != null && !queue.offer(segment)) ui.post { if (running.get()) onOverload() }
            }
        } catch (error: Exception) { fail(error) }
        finally { record = null; try { input?.release() } catch (_: Exception) {} }
    }
    private fun fail(error: Exception) { ui.post { if (running.get()) onError(error.message ?: "Motorul local s-a oprit.") } }
    fun stop() {
        running.set(false)
        synchronized(pointerLock) { if (handle != 0L) cancel(handle) }
        try { record?.stop() } catch (_: Exception) {}
        queue.clear(); microphone.shutdown(); decoder.shutdown()
    }
}
