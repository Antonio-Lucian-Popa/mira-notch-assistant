package ro.novabytecode.mira.companion
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.util.UUID

@RunWith(AndroidJUnit4::class)
class CompanionStoreTest {
    @Test fun remindersSurviveRestartAndRepeatedCompletionDoesNotSkipADay() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val name = "mira.test.${UUID.randomUUID()}"
        try {
            val store = MiraStore(context, name)
            val due = System.currentTimeMillis() + 60000
            store.mutate("add", JSONObject().put("title", "Prânz").put("due", due).put("daily", true))
            val restored = MiraStore(context, name).snapshot().getJSONArray("reminders").getJSONObject(0)
            val id = restored.getString("id")
            val payload = JSONObject().put("id", id).put("due", due)
            store.mutate("complete", payload)
            val next = store.snapshot().getJSONArray("reminders").getJSONObject(0).getLong("due")
            store.mutate("complete", payload)
            assertEquals(next, store.snapshot().getJSONArray("reminders").getJSONObject(0).getLong("due"))
            assertEquals(1, store.snapshot().getJSONObject("today").getInt("completed"))
            store.mutate("snooze", JSONObject().put("id", id).put("due", next))
            val snoozed = store.snapshot().getJSONArray("reminders").getJSONObject(0)
            assertTrue(snoozed.getBoolean("daily")); assertEquals(due, snoozed.getLong("anchor"))
            store.mutate("delete", JSONObject().put("id", id))
            assertEquals(0, MiraStore(context, name).snapshot().getJSONArray("reminders").length())
        } finally { context.getSharedPreferences(name, 0).edit().clear().commit() }
    }
    @Test fun focusPauseResumeAndWaterPersistWithoutDemoMetrics() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val name = "mira.test.${UUID.randomUUID()}"
        try {
            val store = MiraStore(context, name)
            assertEquals(0, store.snapshot().getJSONObject("today").optInt("sessions"))
            store.mutate("water", JSONObject())
            store.mutate("focusStart", JSONObject())
            store.mutate("focusPause", JSONObject())
            val restored = MiraStore(context, name).snapshot()
            assertEquals(1, restored.getJSONObject("today").getInt("water"))
            assertTrue(restored.getJSONObject("focus").getBoolean("paused"))
            store.mutate("focusResume", JSONObject())
            assertFalse(store.snapshot().getJSONObject("focus").getBoolean("paused"))
            store.mutate("focusStop", JSONObject())
            assertFalse(store.snapshot().has("focus"))
            assertEquals(0, store.snapshot().getJSONObject("today").optInt("sessions"))
        } finally { context.getSharedPreferences(name, 0).edit().clear().commit() }
    }
}
