package ro.novabytecode.mira.companion

import android.Manifest
import android.app.*
import android.content.*
import android.content.pm.PackageManager
import android.os.Build
import ro.novabytecode.mira.MainActivity
import org.json.JSONObject

object MiraAlarms {
    const val CHANNEL = "mira.reminders"
    fun channels(context: Context) {
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(NotificationChannel(CHANNEL, "Remindere și focus", NotificationManager.IMPORTANCE_HIGH))
        manager.createNotificationChannel(NotificationChannel("mira.overlay", "Capsula Mira", NotificationManager.IMPORTANCE_LOW))
    }
    fun launch(context: Context): PendingIntent = PendingIntent.getActivity(context, 0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    private fun pending(context: Context, id: String, due: Long): PendingIntent = PendingIntent.getBroadcast(context, 0,
        Intent(context, MiraAlarmReceiver::class.java).setAction("ro.novabytecode.mira.ALARM")
            .setData(android.net.Uri.parse("mira://alarm/$id")).putExtra("id", id).putExtra("due", due),
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    fun cancel(context: Context, id: String) { context.getSystemService(AlarmManager::class.java).cancel(pending(context, id, 0)) }
    fun reconcile(context: Context) {
        channels(context)
        val s = MiraStore(context).snapshot()
        val alarms = context.getSystemService(AlarmManager::class.java)
        val items = s.getJSONArray("reminders")
        for (i in 0 until items.length()) {
            val r = items.getJSONObject(i)
            if (!r.optBoolean("delivered")) {
                val due = r.getLong("due")
                // Inexact by design: no exact-alarm or battery-exemption permission required.
                alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, due.coerceAtLeast(System.currentTimeMillis() + 500), pending(context, r.getString("id"), due))
            }
        }
        val focus = s.optJSONObject("focus")
        if (focus != null && !focus.optBoolean("paused")) {
            val due = focus.getLong("due")
            alarms.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, due.coerceAtLeast(System.currentTimeMillis() + 500), pending(context, "focus", due))
        } else cancel(context, "focus")
    }
    fun notify(context: Context, id: String, title: String, text: String, due: Long): Boolean {
        channels(context)
        if (Build.VERSION.SDK_INT >= 33 && context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return false
        val notification = Notification.Builder(context, CHANNEL).setSmallIcon(ro.novabytecode.mira.R.drawable.ic_mira)
            .setContentTitle(title).setContentText(text).setContentIntent(launch(context)).setAutoCancel(true)
            .setCategory(Notification.CATEGORY_REMINDER)
        if (id != "focus") {
            for ((action, label) in listOf("complete" to "Gata", "snooze" to "+5 minute")) {
                val intent = Intent(context, MiraAlarmReceiver::class.java).setAction(action)
                    .setData(android.net.Uri.parse("mira://$action/$id")).putExtra("id", id).putExtra("due", due)
                notification.addAction(Notification.Action.Builder(null, label,
                    PendingIntent.getBroadcast(context, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)).build())
            }
        }
        context.getSystemService(NotificationManager::class.java).notify(id, 1, notification.build())
        return true
    }
}

class MiraAlarmReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val store = MiraStore(context)
        val id = intent.getStringExtra("id") ?: ""
        val due = intent.getLongExtra("due", 0)
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_TIME_CHANGED, Intent.ACTION_TIMEZONE_CHANGED -> MiraAlarms.reconcile(context)
            "complete", "snooze" -> {
                store.mutate(intent.action!!, JSONObject().put("id", id).put("due", due))
                context.getSystemService(NotificationManager::class.java).cancel(id, 1)
                MiraAlarms.reconcile(context)
            }
            else -> {
                val s = store.snapshot()
                if (id == "focus") {
                    val f = s.optJSONObject("focus")
                    if (f != null && !f.optBoolean("paused") && f.getLong("due") == due && due <= System.currentTimeMillis()) {
                        store.mutate("focusDone", JSONObject().put("id", f.getString("id")))
                        MiraAlarms.notify(context, "focus", "Bravo! Ai terminat sesiunea.", "Ia o pauză și întinde-te puțin.", due)
                    }
                } else {
                    val reminders = s.getJSONArray("reminders")
                    for (i in 0 until reminders.length()) {
                        val r = reminders.getJSONObject(i)
                        if (r.getString("id") == id && r.getLong("due") == due && due <= System.currentTimeMillis() && !r.optBoolean("delivered")) {
                            if (MiraAlarms.notify(context, id, r.getString("title"), "Mira îți amintește. E timpul pentru tine.", due))
                                store.mutate("delivered", JSONObject().put("id", id).put("due", due))
                        }
                    }
                }
            }
        }
        MiraOverlayService.refresh()
    }
}
