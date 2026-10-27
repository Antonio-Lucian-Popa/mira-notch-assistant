package ro.novabytecode.mira.companion

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.UUID

/** One authoritative local store, shared by React Native, alarms and the overlay. */
class MiraStore(context: Context, storageName: String = "mira.mobile.v1") {
    private val prefs = context.getSharedPreferences(storageName, Context.MODE_PRIVATE)
    companion object { private val lock = Any() }
    private fun read() = JSONObject(prefs.getString("state", "{}") ?: "{}")
    private fun write(state: JSONObject) {
        check(prefs.edit().putString("state", state.toString()).commit()) { "Nu am putut salva datele." }
    }
    fun snapshot(): JSONObject = synchronized(lock) {
        val s = read()
        if (!s.has("reminders")) s.put("reminders", JSONArray())
        if (!s.has("theme")) s.put("theme", "classic")
        if (!s.has("scale")) s.put("scale", 1.0)
        if (!s.has("offset")) s.put("offset", 8)
        val day = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date())
        s.put("today", s.optJSONObject("days")?.optJSONObject(day) ?: JSONObject())
        s
    }
    private fun day(s: JSONObject): JSONObject {
        val days = s.optJSONObject("days") ?: JSONObject().also { s.put("days", it) }
        val key = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date())
        return days.optJSONObject(key) ?: JSONObject().also { days.put(key, it) }
    }
    fun mutate(action: String, payload: JSONObject): JSONObject = synchronized(lock) {
        val s = read()
        val items = s.optJSONArray("reminders") ?: JSONArray().also { s.put("reminders", it) }
        when (action) {
            "add" -> {
                val title = payload.getString("title").trim()
                val due = payload.getLong("due")
                require(title.isNotEmpty() && title.length <= 160) { "Titlul trebuie să aibă 1–160 de caractere." }
                require(due > System.currentTimeMillis()) { "Alege o oră din viitor." }
                require(items.length() < 200) { "Poți păstra cel mult 200 de remindere." }
                items.put(JSONObject().put("id", UUID.randomUUID().toString()).put("title", title)
                    .put("due", due).put("anchor", due).put("daily", payload.optBoolean("daily")))
            }
            "complete", "delete", "snooze" -> {
                val id = payload.getString("id")
                val index = (0 until items.length()).firstOrNull { items.getJSONObject(it).getString("id") == id }
                if (index != null) {
                    val item = items.getJSONObject(index)
                    // Stale taps cannot complete the next recurring occurrence twice.
                    if (payload.has("due") && payload.getLong("due") != item.getLong("due")) return@synchronized snapshot()
                    if (action == "snooze") {
                        item.put("due", System.currentTimeMillis() + 5 * 60_000).put("delivered", false)
                    } else if (action == "complete" && item.optBoolean("daily")) {
                        val next = nextDaily(item.optLong("anchor", item.getLong("due")), maxOf(System.currentTimeMillis(), item.getLong("due")))
                        item.put("due", next).put("delivered", false)
                    } else items.remove(index)
                    if (action == "complete") { val d = day(s); d.put("completed", d.optInt("completed") + 1) }
                }
            }
            "delivered" -> {
                for (i in 0 until items.length()) {
                    val r = items.getJSONObject(i)
                    if (r.getString("id") == payload.getString("id") && r.getLong("due") == payload.getLong("due")) r.put("delivered", true)
                }
            }
            "water" -> { val d = day(s); d.put("water", (d.optInt("water") + payload.optInt("amount", 1)).coerceIn(0, 99)) }
            "settings" -> {
                if (payload.has("theme")) {
                    require(payload.getString("theme") in listOf("classic", "robot", "cat", "pixel"))
                    s.put("theme", payload.getString("theme"))
                }
                if (payload.has("scale")) s.put("scale", payload.getDouble("scale").coerceIn(0.85, 1.15))
                if (payload.has("offset")) s.put("offset", payload.getInt("offset").coerceIn(0, 120))
                if (payload.has("motion")) s.put("motion", payload.getBoolean("motion"))
            }
            "focusStart" -> {
                require(!s.has("focus")) { "Ai deja o sesiune în curs." }
                s.put("focus", JSONObject().put("id", UUID.randomUUID().toString())
                    .put("due", System.currentTimeMillis() + 25 * 60_000).put("remaining", 25 * 60_000).put("paused", false))
            }
            "focusPause" -> s.optJSONObject("focus")?.let {
                if (!it.optBoolean("paused")) it.put("remaining", (it.getLong("due") - System.currentTimeMillis()).coerceAtLeast(0)).put("paused", true)
            }
            "focusResume" -> s.optJSONObject("focus")?.let {
                if (it.optBoolean("paused")) it.put("due", System.currentTimeMillis() + it.getLong("remaining")).put("paused", false)
            }
            "focusStop" -> s.remove("focus")
            "focusDone" -> s.optJSONObject("focus")?.let {
                if (it.getString("id") == payload.optString("id") && !it.optBoolean("paused") && it.getLong("due") <= System.currentTimeMillis()) {
                    val d = day(s); d.put("sessions", d.optInt("sessions") + 1); s.remove("focus")
                }
            }
            else -> error("Acțiune necunoscută: $action")
        }
        write(s)
        snapshot()
    }
}

internal fun nextDaily(previous: Long, now: Long): Long {
    val c = java.util.Calendar.getInstance().apply { timeInMillis = previous }
    do { c.add(java.util.Calendar.DAY_OF_YEAR, 1) } while (c.timeInMillis <= now)
    return c.timeInMillis
}
