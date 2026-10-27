package ro.novabytecode.mira.companion

import android.content.Intent
import android.app.NotificationManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import com.facebook.react.bridge.*
import com.facebook.react.ReactPackage
import com.facebook.react.uimanager.ViewManager
import org.json.JSONObject

class MiraModule(private val context: ReactApplicationContext) : ReactContextBaseJavaModule(context), LifecycleEventListener {
    init { context.addLifecycleEventListener(this) }
    override fun onHostResume() {
        if (context.getSharedPreferences("mira.voice", 0).getBoolean("enabled", false) &&
            !MiraVoiceService.snapshot().optBoolean("active") &&
            context.checkSelfPermission(android.Manifest.permission.RECORD_AUDIO) == android.content.pm.PackageManager.PERMISSION_GRANTED) {
            try { context.startForegroundService(Intent(context, MiraVoiceService::class.java).putExtra("continuous", true)) }
            catch (_: Exception) { /* Android can reject microphone startup; leave the switch available for retry. */ }
        }
    }
    override fun onHostPause() {}
    override fun onHostDestroy() {}
    private val scanner=PairingScanner(context)
    private val io=java.util.concurrent.Executors.newSingleThreadExecutor()
    private fun background(promise:Promise,task:()->String){io.execute{result(promise,task)}}
    override fun getName() = "MiraCompanion"
    private fun result(promise: Promise, task: () -> String) { try { promise.resolve(task()) } catch (e: Exception) { promise.reject("MIRA", e.message, e) } }
    @ReactMethod fun snapshot(promise: Promise) = result(promise) {
        MiraStore(context).snapshot().put("overlayGranted", Settings.canDrawOverlays(context))
            .put("overlayRunning", MiraOverlayService.running())
            .put("shakeEnabled", context.getSharedPreferences("mira.shake", android.content.Context.MODE_PRIVATE).getBoolean("enabled", false))
            .put("voiceModel",LocalVoiceModel.snapshot(context))
            .put("voiceEnabled",context.getSharedPreferences("mira.voice", 0).getBoolean("enabled", false))
            .put("voice",MiraVoiceService.snapshot()).put("transcripts",TranscriptStore(context).list()).put("peer",TranscriptSync.status(context))
            .put("notificationsGranted", context.getSystemService(NotificationManager::class.java).areNotificationsEnabled()).toString()
    }
    @ReactMethod fun action(action: String, json: String, promise: Promise) = result(promise) {
        val payload = JSONObject(json)
        if (action in listOf("complete", "delete", "snooze")) {
            val id = payload.getString("id")
            MiraAlarms.cancel(context, id)
            context.getSystemService(NotificationManager::class.java).cancel(id, 1)
        }
        val s = MiraStore(context).mutate(action, payload)
        MiraAlarms.reconcile(context)
        MiraOverlayService.refresh()
        s.toString()
    }
    @ReactMethod fun scanPairing(promise:Promise){UiThreadUtil.runOnUiThread{scanner.scan(promise)}}
    @ReactMethod fun voiceModelDownload(promise:Promise)=result(promise){LocalVoiceModel.download(context);"{}"}
    @ReactMethod fun voiceAuto(enabled:Boolean,promise:Promise)=result(promise){
        require(context.lifecycleState==com.facebook.react.common.LifecycleState.RESUMED){"Deschide aplicația pentru a activa vocea."}
        if(enabled){
            require(context.checkSelfPermission(android.Manifest.permission.RECORD_AUDIO)==android.content.pm.PackageManager.PERMISSION_GRANTED){"Permite accesul la microfon."}
            require(LocalVoiceModel.ready(context)){"Descarcă mai întâi modelul vocal gratuit din Voce & PC."}
        }
        context.getSharedPreferences("mira.voice",0).edit().putBoolean("enabled",enabled).apply()
        if(enabled) context.startForegroundService(Intent(context,MiraVoiceService::class.java).putExtra("continuous",true))
        else MiraVoiceService.stopNow()
        "{}"
    }
    @ReactMethod fun voiceStart(dictate:Boolean,promise:Promise)=result(promise){
        require(context.lifecycleState==com.facebook.react.common.LifecycleState.RESUMED){"Pornește vocea din aplicația deschisă."}
        require(context.checkSelfPermission(android.Manifest.permission.RECORD_AUDIO)==android.content.pm.PackageManager.PERMISSION_GRANTED){"Permite accesul la microfon."}
        require(LocalVoiceModel.ready(context)){"Descarcă mai întâi modelul vocal gratuit din Voce & PC."}
        val intent=Intent(context,MiraVoiceService::class.java).putExtra("dictate",dictate)
        if(Build.VERSION.SDK_INT>=26)context.startForegroundService(intent)else context.startService(intent)
        "{}"
    }
    @ReactMethod fun voiceStop(promise:Promise)=result(promise){context.getSharedPreferences("mira.voice",0).edit().putBoolean("enabled",false).apply();MiraVoiceService.stopNow();"{}"}
    @ReactMethod fun transcript(id:String,promise:Promise)=background(promise){TranscriptStore(context).get(id).toString()}
    @ReactMethod fun deleteTranscript(id:String,promise:Promise)=background(promise){require(MiraVoiceService.snapshot().optString("transcriptId")!=id||!MiraVoiceService.snapshot().optBoolean("active")){"Oprește mai întâi transcrierea."};TranscriptStore(context).delete(id);"{}"}
    @ReactMethod fun sharePdf(id:String,promise:Promise)=background(promise){
        val file=TranscriptPdf.export(context,TranscriptStore(context).get(id))
        val uri=androidx.core.content.FileProvider.getUriForFile(context,context.packageName+".files",file)
        context.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("application/pdf").putExtra(Intent.EXTRA_STREAM,uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),"Exportă transcrierea").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        "{}"
    }
    @ReactMethod fun pair(code:String,promise:Promise)=background(promise){TranscriptSync.pair(context,code);"{}"}
    @ReactMethod fun unpair(promise:Promise)=background(promise){context.getSharedPreferences("mira.shake", android.content.Context.MODE_PRIVATE).edit().putBoolean("enabled",false).apply();TranscriptSync.disconnect(context);MiraOverlayService.refresh();"{}"}
    @ReactMethod fun syncNow(promise:Promise)=background(promise){check(TranscriptSync.upload(context)){TranscriptSync.status(context).optString("message")};"{}"}
    @ReactMethod fun remoteActions(promise:Promise)=background(promise){TranscriptSync.actions(context).toString()}
    @ReactMethod fun shake(enabled:Boolean,promise:Promise)=result(promise){
        if(enabled){
            require(TranscriptSync.status(context).optBoolean("paired")){"Asociază mai întâi PC-ul."}
            require(MiraOverlayService.running()){"Pornește mai întâi capsula Mira."}
            require(context.getSystemService(android.hardware.SensorManager::class.java).getDefaultSensor(android.hardware.Sensor.TYPE_ACCELEROMETER)!=null){"Telefonul nu are accelerometru disponibil."}
        }
        context.getSharedPreferences("mira.shake", android.content.Context.MODE_PRIVATE).edit().putBoolean("enabled",enabled).apply()
        MiraOverlayService.refresh()
        "{}"
    }
    @ReactMethod fun remoteAction(id:String,promise:Promise)=background(promise){TranscriptSync.trigger(context,id);"{}"}
    @ReactMethod fun overlayPermission(promise: Promise) = result(promise) {
        context.startActivity(Intent(Settings.ACTION_MANAGE_OVERLAY_PERMISSION, Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        "{}"
    }
    @ReactMethod fun notificationSettings(promise: Promise) = result(promise) {
        context.startActivity(Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        "{}"
    }
    @ReactMethod fun overlay(enabled: Boolean, promise: Promise) = result(promise) {
        if (enabled) {
            require(Settings.canDrawOverlays(context)) { "Permite mai întâi afișarea peste alte aplicații." }
            require(context.lifecycleState==com.facebook.react.common.LifecycleState.RESUMED) { "Pornește capsula din aplicația deschisă." }
            val intent = Intent(context, MiraOverlayService::class.java)
            if (Build.VERSION.SDK_INT >= 26) context.startForegroundService(intent) else context.startService(intent)
        } else context.stopService(Intent(context, MiraOverlayService::class.java))
        "{}"
    }
    @ReactMethod fun preview(expression: String, promise: Promise) = result(promise) {
        require(expression in listOf("happy", "wink", "sleepy", "excited"))
        MiraOverlayService.preview(expression)
        "{}"
    }
}
class MiraPackage : ReactPackage {
    override fun createNativeModules(context: ReactApplicationContext): List<NativeModule> = listOf(MiraModule(context))
    override fun createViewManagers(context: ReactApplicationContext): List<ViewManager<*, *>> = emptyList()
}
