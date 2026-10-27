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

class MiraModule(private val context: ReactApplicationContext) : ReactContextBaseJavaModule(context) {
    override fun getName() = "MiraCompanion"
    private fun result(promise: Promise, task: () -> String) { try { promise.resolve(task()) } catch (e: Exception) { promise.reject("MIRA", e.message, e) } }
    @ReactMethod fun snapshot(promise: Promise) = result(promise) {
        MiraStore(context).snapshot().put("overlayGranted", Settings.canDrawOverlays(context))
            .put("overlayRunning", MiraOverlayService.running())
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
            require(context.currentActivity?.hasWindowFocus() == true) { "Pornește capsula din aplicația deschisă." }
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
