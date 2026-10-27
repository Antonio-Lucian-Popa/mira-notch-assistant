package ro.novabytecode.mira.companion

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.*
import org.json.JSONObject
import java.lang.ref.WeakReference
import ro.novabytecode.mira.R

/** User-enabled foreground microphone, backed by bundled native Whisper inference. */
class MiraVoiceService : Service() {
    companion object {
        private val lock = Any()
        private var status = JSONObject().put("active", false).put("state", "idle").put("message", "Vocea este oprită.")
        private var active = WeakReference<MiraVoiceService>(null)
        fun snapshot(): JSONObject = synchronized(lock) { JSONObject(status.toString()) }
        fun stopNow() { active.get()?.let { service -> service.ui.post { service.disableAndFinish() } } }
        private fun update(state: String, message: String, partial: String = "", id: String = "") = synchronized(lock) {
            status = JSONObject().put("active", state !in listOf("idle", "error", "saved")).put("state", state)
                .put("message", message).put("partial", partial).put("transcriptId", id).put("engine", "whisper-base-local")
        }
    }
    private val ui = Handler(Looper.getMainLooper())
    private var engine: LocalWhisper? = null
    private var continuous = false
    @Volatile private var dictating = false
    private var finished = false
    private var armedUntil = 0L
    private var draft: JSONObject? = null
    private var buffer = DictationBuffer()
    private val dictationLimit = Runnable {
        try { completeDictation() } catch (error: Exception) { finish(true, error.message.orEmpty()) }
    }
    override fun onBind(intent: Intent?) = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "stop") { disableAndFinish(); return START_NOT_STICKY }
        if (active.get() === this) {
            if (intent?.getBooleanExtra("continuous", false) == true) continuous = true
            if (intent?.getBooleanExtra("dictate", false) == true && !dictating) startDictation()
            return START_NOT_STICKY
        }
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED || !LocalVoiceModel.ready(this)) {
            update("error", if (!LocalVoiceModel.ready(this)) "Descarcă mai întâi modelul vocal gratuit din Voce & PC." else "Permite accesul la microfon.")
            stopSelf(); return START_NOT_STICKY
        }
        continuous = intent?.getBooleanExtra("continuous", false) == true
        try {
            getSystemService(NotificationManager::class.java).createNotificationChannel(
                NotificationChannel("mira.voice", "Microfon Mira", NotificationManager.IMPORTANCE_LOW))
            val stop = PendingIntent.getService(this, 33, Intent(this, MiraVoiceService::class.java).setAction("stop"), PendingIntent.FLAG_IMMUTABLE)
            val notification = Notification.Builder(this, "mira.voice").setSmallIcon(R.drawable.ic_mira)
                .setContentTitle("Mira · microfon local activ")
                .setContentText("Whisper pe telefon. Spune Mira sau oprește microfonul.")
                .setContentIntent(MiraAlarms.launch(this)).setOngoing(true)
                .addAction(Notification.Action.Builder(null, "Stop microfon", stop).build()).build()
            if (Build.VERSION.SDK_INT >= 30) startForeground(33, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
            else startForeground(33, notification)
            active = WeakReference(this)
            update("loading", "Încarc motorul vocal local…")
            engine = LocalWhisper(
                onText = { if (!finished) receive(it) },
                onError = { if (!finished) finish(true, it) },
                onReady = { if (!finished) { if (dictating) publishDictation() else update("wake", "Spune «Mira», apoi comanda. Procesare pe telefon.") } },
                onOverload = { if (!finished) update(if (dictating) "dictating" else "wake", "Motorul local este ocupat. Lasă o scurtă pauză între propoziții.", buffer.preview(), draft?.optString("id").orEmpty()) },
                commandMode = { !dictating }
            )
            if (intent?.getBooleanExtra("dictate", false) == true) startDictation()
            engine!!.start(LocalVoiceModel.file(this))
        } catch (error: Exception) { finish(true, error.message ?: "Nu pot porni vocea locală.") }
          catch (_: LinkageError) { finish(true, "Biblioteca vocală nu este compatibilă cu acest telefon. Instalează APK-ul complet.") }
        return START_NOT_STICKY
    }
    private fun startDictation() {
        draft = TranscriptStore(this).create(); buffer = DictationBuffer(); dictating = true; armedUntil = 0
        ui.removeCallbacks(dictationLimit); ui.postDelayed(dictationLimit, 30 * 60_000L)
        publishDictation()
    }
    private fun publishDictation() = update("dictating", "Transcriu local. Spune «Mira gata» pentru salvare.", buffer.preview(), draft?.optString("id").orEmpty())
    private fun receive(text: String) {
        try {
            if (dictating) {
                val stop = buffer.accept(text)
                draft?.let { it.put("text", buffer.preview()); TranscriptStore(this).save(it) }
                if (stop) completeDictation() else publishDictation()
            } else handleCommand(text)
        } catch (error: Exception) { finish(true, error.message ?: "Nu pot salva transcrierea.") }
    }
    private fun handleCommand(text: String) {
        val wake = VoiceRules.wake(text)
        val commandText = if (wake != null) { armedUntil = SystemClock.elapsedRealtime() + 15_000; wake }
            else if (SystemClock.elapsedRealtime() < armedUntil) VoiceRules.normalized(text) else return
        if (commandText.isBlank()) { update("command", "Te ascult. Spune comanda."); return }
        when (val command = VoiceRules.command(commandText)) {
            "dictate" -> startDictation()
            "stop" -> disableAndFinish()
            "focusStart", "focusPause", "focusResume", "focusStop", "water" -> {
                MiraStore(this).mutate(command, JSONObject()); MiraAlarms.reconcile(this); MiraOverlayService.refresh()
                armedUntil = 0; update("wake", "Gata. Spune «Mira» pentru următoarea comandă.")
            }
            else -> update("command", "Nu am înțeles comanda. Încearcă «pornește focus» sau «pornește transcrierea».")
        }
    }
    private fun completeDictation() {
        ui.removeCallbacks(dictationLimit)
        saveDraft()
        draft = null; dictating = false; buffer = DictationBuffer(); armedUntil = 0
        if (continuous) update("wake", "Transcriere salvată. Spune «Mira» pentru o nouă comandă.")
        else finish(false, "Transcriere salvată pe telefon.")
    }
    private fun saveDraft() {
        draft?.let {
            it.put("text", buffer.finish()).put("complete", true).put("endedAt", System.currentTimeMillis())
            TranscriptStore(this).save(it); TranscriptSync.schedule(this)
        }
    }
    private fun disableAndFinish() {
        getSharedPreferences("mira.voice", 0).edit().putBoolean("enabled", false).apply()
        finish(false)
    }
    private fun finish(error: Boolean, message: String = "") {
        if (finished) return
        finished = true; ui.removeCallbacksAndMessages(null); engine?.stop(); engine = null
        try {
            saveDraft()
            update(if (error) "error" else if (draft != null) "saved" else "idle",
                message.ifBlank { if (draft != null) "Transcriere salvată. Microfon oprit." else "Microfon oprit." }, id = draft?.optString("id").orEmpty())
        } catch (failure: Exception) { update("error", "Nu pot salva textul: ${failure.message}") }
        active.clear(); stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    override fun onDestroy() {
        if (!finished && active.get() === this) finish(false, "Microfon oprit de Android. Textul confirmat a fost păstrat.")
        super.onDestroy()
    }
}
