package ro.novabytecode.mira.companion
import org.junit.Assert.*
import org.junit.Test
import java.util.Calendar
import java.util.TimeZone
class ReminderTimeTest {
    @Test fun recurringRemindersSkipMissedDaysAndKeepLocalClockAcrossDst() {
        val oldZone = TimeZone.getDefault()
        try {
            TimeZone.setDefault(TimeZone.getTimeZone("Europe/Bucharest"))
            val start = Calendar.getInstance().apply { set(2026, Calendar.OCTOBER, 24, 13, 0, 0); set(Calendar.MILLISECOND, 0) }
            val now = Calendar.getInstance().apply { set(2026, Calendar.OCTOBER, 26, 14, 0, 0) }
            val next = Calendar.getInstance().apply { timeInMillis = nextDaily(start.timeInMillis, now.timeInMillis) }
            assertEquals(27, next.get(Calendar.DAY_OF_MONTH)); assertEquals(13, next.get(Calendar.HOUR_OF_DAY))
            assertTrue(next.timeInMillis > now.timeInMillis)
        } finally { TimeZone.setDefault(oldZone) }
    }
    @Test fun countdownNeverShowsNegativeValues() {
        assertEquals("0:00", formatDuration(-100)); assertEquals("0:01", formatDuration(1)); assertEquals("1h 1m", formatDuration(3660000))
    }
}
