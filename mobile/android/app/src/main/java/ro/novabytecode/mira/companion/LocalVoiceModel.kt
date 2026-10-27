package ro.novabytecode.mira.companion

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** Only model weights are downloaded. Microphone data never enters this class. */
object LocalVoiceModel {
    const val SIZE = 147951465L
    const val SHA256 = "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe"
    const val URL_BASE = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"
    private val downloading = AtomicBoolean(false)
    private val io = Executors.newSingleThreadExecutor()
    @Volatile private var progress = 0
    @Volatile private var message = "Descarcă modelul vocal gratuit o singură dată."
    fun file(context: Context) = File(context.filesDir, "voice-models/ggml-base.bin")
    fun ready(context: Context) = file(context).let { it.isFile && it.length() == SIZE }
    fun snapshot(context: Context) = JSONObject().put("ready", ready(context)).put("downloading", downloading.get())
        .put("progress", progress).put("bytes", SIZE).put("message", if (ready(context)) "Whisper local · română · fără cost per utilizare" else message)
    fun download(context: Context) {
        if (ready(context) || !downloading.compareAndSet(false, true)) return
        val app = context.applicationContext
        progress = 0; message = "Descarc modelul vocal local…"
        io.execute {
            val target = file(app)
            val temporary = File(target.parentFile, "ggml-base.bin.part")
            var connection: HttpURLConnection? = null
            try {
                target.parentFile!!.mkdirs()
                check(target.parentFile!!.usableSpace > SIZE + 20_000_000) { "Spațiu insuficient pe telefon." }
                connection = URL(URL_BASE).openConnection() as HttpURLConnection
                connection.connectTimeout = 20_000; connection.readTimeout = 30_000
                connection.instanceFollowRedirects = true
                connection.setRequestProperty("User-Agent", "Mira-local-voice")
                check(connection.responseCode == 200) { "Descărcarea nu este disponibilă. Reîncearcă." }
                val digest = MessageDigest.getInstance("SHA-256")
                var bytes = 0L
                connection.inputStream.use { input -> temporary.outputStream().use { output ->
                    val buffer = ByteArray(65536)
                    while (true) {
                        val count = input.read(buffer); if (count < 0) break
                        bytes += count
                        check(bytes <= SIZE) { "Modelul primit are o dimensiune neașteptată." }
                        output.write(buffer, 0, count); digest.update(buffer, 0, count)
                        progress = (bytes * 100 / SIZE).toInt()
                    }
                } }
                val hash = digest.digest().joinToString("") { "%02x".format(it) }
                check(bytes == SIZE && hash == SHA256) { "Verificarea modelului a eșuat. Reîncearcă descărcarea." }
                check(temporary.renameTo(target)) { "Nu pot salva modelul pe telefon." }
                message = "Model vocal instalat. Poți activa Mira."
            } catch (error: Exception) {
                temporary.delete(); progress = 0
                message = error.message ?: "Nu pot descărca modelul. Verifică internetul și reîncearcă."
            } finally { connection?.disconnect(); downloading.set(false) }
        }
    }
}
