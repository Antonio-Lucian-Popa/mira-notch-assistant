package ro.novabytecode.mira.companion

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.*
import android.speech.*
import org.json.JSONObject
import java.lang.ref.WeakReference
import ro.novabytecode.mira.R

/** Explicit, bounded microphone sessions. Not an always-on DSP hotword service. */
class MiraVoiceService : Service() {
    companion object {
        private val lock=Any()
        private var status=JSONObject().put("active",false).put("state","idle").put("message","Vocea este oprită.")
        private var active=WeakReference<MiraVoiceService>(null)
        fun snapshot():JSONObject = synchronized(lock) {JSONObject(status.toString())}
        fun stopNow(){active.get()?.let{s->s.ui.post{s.finish(false)}}}
        private fun update(state:String,message:String,partial:String="",id:String="") = synchronized(lock) {
            status=JSONObject().put("active",state !in listOf("idle","error","saved")).put("state",state)
                .put("message",message).put("partial",partial).put("transcriptId",id)
        }
    }
    private val ui=Handler(Looper.getMainLooper())
    private var recognizer:SpeechRecognizer?=null
    private var dictating=false
    private var finished=false
    private var generation=0
    private var deadline=0L
    private var armedUntil=0L
    private var errors=0
    private var draft:JSONObject?=null
    private var buffer=DictationBuffer()
    private var partial=""
    private val timeout=Runnable{finish(false,"Sesiunea vocală s-a încheiat la limita de timp.")}
    override fun onBind(intent:Intent?)=null
    override fun onStartCommand(intent:Intent?,flags:Int,startId:Int):Int {
        if(intent?.action=="stop"){finish(false);return START_NOT_STICKY}
        if(active.get()!=null) return START_NOT_STICKY
        if(checkSelfPermission(Manifest.permission.RECORD_AUDIO)!=PackageManager.PERMISSION_GRANTED || !SpeechRecognizer.isRecognitionAvailable(this)) {
            update("error","Recunoașterea vocală Android nu este disponibilă sau microfonul nu este permis.");stopSelf();return START_NOT_STICKY
        }
        try {
            MiraAlarms.channels(this)
            val stop=PendingIntent.getService(this,33,Intent(this,MiraVoiceService::class.java).setAction("stop"),PendingIntent.FLAG_IMMUTABLE)
            val n=Notification.Builder(this,"mira.voice").setSmallIcon(R.drawable.ic_mira).setContentTitle("Mira ascultă · microfon activ")
                .setContentText("Sesiune pornită de tine. Poți opri oricând.").setContentIntent(MiraAlarms.launch(this)).setOngoing(true)
                .addAction(Notification.Action.Builder(null,"Stop microfon",stop).build()).build()
            getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel("mira.voice","Microfon Mira",NotificationManager.IMPORTANCE_LOW))
            if(Build.VERSION.SDK_INT>=30)startForeground(33,n,ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE) else startForeground(33,n)
            active=WeakReference(this)
            recognizer=SpeechRecognizer.createSpeechRecognizer(this)
            if(intent?.getBooleanExtra("dictate",false)==true)startDictation() else {
                deadline=SystemClock.elapsedRealtime()+60_000
                update("wake","Spune «Mira» sau «Hey Mira», apoi comanda. Sesiune de 60 secunde.")
            }
            listen()
        }catch(e:Exception){finish(true,"Nu pot porni microfonul: ${e.message}")}
        return START_NOT_STICKY
    }
    private fun startDictation(){
        draft=TranscriptStore(this).create();buffer=DictationBuffer();dictating=true;partial=""
        deadline=SystemClock.elapsedRealtime()+30*60_000
        update("dictating","Transcriu. Spune «Mira gata» sau apasă Stop.",id=draft!!.getString("id"))
    }
    private fun listen(){
        if(finished)return
        if(SystemClock.elapsedRealtime()>=deadline){finish(false);return}
        ui.removeCallbacks(timeout);ui.postDelayed(timeout,(deadline-SystemClock.elapsedRealtime()).coerceAtLeast(1))
        val current=++generation
        recognizer?.setRecognitionListener(object:RecognitionListener {
            private fun valid()=!finished&&current==generation
            override fun onReadyForSpeech(params:Bundle?){}
            override fun onBeginningOfSpeech(){}
            override fun onRmsChanged(rmsdB:Float){}
            override fun onBufferReceived(bytes:ByteArray?){}
            override fun onEndOfSpeech(){}
            override fun onEvent(type:Int,params:Bundle?){}
            override fun onPartialResults(results:Bundle?){
                if(!valid())return
                partial=results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty()
                if(dictating){
                    if(buffer.hasStop(partial)){buffer.accept(partial);partial="";finish(false)}
                    else update("dictating","Transcriu… «Mira gata» pentru oprire.",listOf(buffer.preview(),partial).filter{it.isNotBlank()}.joinToString("\n"),draft?.optString("id").orEmpty())
                }
            }
            override fun onResults(results:Bundle?){
                if(!valid())return
                errors=0
                val text=results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull().orEmpty();partial=""
                try {
                    if(dictating){
                        val stop=buffer.accept(text);draft?.let{it.put("text",buffer.preview());TranscriptStore(this@MiraVoiceService).save(it)}
                        if(stop){finish(false);return}
                        update("dictating","Transcriu. Poți opri oricând.",buffer.preview(),draft?.optString("id").orEmpty())
                    } else handleCommand(text)
                    if(!finished)ui.postDelayed({listen()},350)
                }catch(e:Exception){finish(true,e.message?:"Nu pot salva transcrierea.")}
            }
            override fun onError(error:Int){
                if(!valid())return
                val recoverable=error==SpeechRecognizer.ERROR_SPEECH_TIMEOUT||error==SpeechRecognizer.ERROR_NO_MATCH
                if(recoverable && ++errors<5){ui.postDelayed({listen()},650);return}
                finish(true,when(error){SpeechRecognizer.ERROR_NETWORK,SpeechRecognizer.ERROR_NETWORK_TIMEOUT->"Serviciul vocal nu are conexiune. Textul confirmat a fost păstrat."
                    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS->"Permisiunea pentru microfon a fost retrasă."
                    SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED,SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE->"Instalează/activează limba română în serviciul vocal Android."
                    else->"Recunoașterea vocală s-a oprit (cod $error). Textul confirmat a fost păstrat."})
            }
        })
        recognizer?.startListening(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL,RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
            .putExtra(RecognizerIntent.EXTRA_LANGUAGE,"ro-RO").putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS,true)
            .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS,1))
    }
    private fun handleCommand(text:String){
        val wake=VoiceRules.wake(text)
        val commandText=if(wake!=null){armedUntil=SystemClock.elapsedRealtime()+10_000;wake}else if(SystemClock.elapsedRealtime()<armedUntil)VoiceRules.normalized(text)else return
        if(commandText.isBlank()){update("command","Te ascult. Spune comanda.");return}
        val command=VoiceRules.command(commandText)
        when(command){
            "dictate"->startDictation()
            "stop"->finish(false)
            "focusStart","focusPause","focusResume","focusStop","water"->{MiraStore(this).mutate(command,JSONObject());MiraAlarms.reconcile(this);MiraOverlayService.refresh();update("wake","Gata. Mai poți spune «Mira» și o comandă.");armedUntil=0}
            else->update("command","Comandă necunoscută. Încearcă «pornește transcrierea» sau «pornește focus».")
        }
    }
    private fun finish(error:Boolean,message:String=""){
        if(finished)return
        finished=true;generation++;ui.removeCallbacksAndMessages(null)
        recognizer?.cancel();recognizer?.destroy();recognizer=null
        try {
            draft?.let{
                if(!error&&partial.isNotBlank())buffer.accept(partial)
                it.put("text",buffer.finish()).put("complete",true).put("endedAt",System.currentTimeMillis())
                TranscriptStore(this).save(it)
                TranscriptSync.schedule(this)
            }
            update(if(error)"error" else if(draft!=null)"saved" else "idle",message.ifBlank{if(draft!=null)"Transcriere salvată. O poți exporta sau trimite la PC." else "Microfon oprit."},id=draft?.optString("id").orEmpty())
        }catch(e:Exception){update("error","Nu pot salva: ${e.message}")}
        active.clear();stopForeground(STOP_FOREGROUND_REMOVE);stopSelf()
    }
    override fun onDestroy(){if(!finished)finish(false,"Microfon oprit de Android. Textul confirmat a fost păstrat.");super.onDestroy()}
}
