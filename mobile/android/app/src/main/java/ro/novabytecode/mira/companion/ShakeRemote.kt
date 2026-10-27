package ro.novabytecode.mira.companion

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.widget.Toast
import java.util.concurrent.Executors
import kotlin.math.sqrt

/** Lives only with the explicitly started overlay; never queues offline commands. */
internal class ShakeRemote(private val context: Context) : SensorEventListener {
    private val sensors = context.getSystemService(SensorManager::class.java)
    private val io = Executors.newSingleThreadExecutor()
    private val ui = Handler(Looper.getMainLooper())
    private var detector = ShakeDetector()
    private var listening = false
    @Volatile private var closed = false
    fun update(screenOn: Boolean) {
        val wanted = screenOn && context.getSharedPreferences("mira.shake", Context.MODE_PRIVATE).getBoolean("enabled", false) && TranscriptSync.status(context).optBoolean("paired")
        if (wanted == listening) return
        sensors.unregisterListener(this)
        listening = false
        detector = ShakeDetector()
        if (wanted) sensors.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)?.let {
            listening = sensors.registerListener(this, it, SensorManager.SENSOR_DELAY_GAME)
        }
    }
    override fun onSensorChanged(event: SensorEvent) {
        if (!listening || closed) return
        val a = event.values
        val g = sqrt(a[0]*a[0]+a[1]*a[1]+a[2]*a[2])/SensorManager.GRAVITY_EARTH
        if (!detector.sample(g, SystemClock.elapsedRealtime())) return
        io.execute {
            // Re-check authorization after crossing the worker boundary.
            if (closed || !context.getSharedPreferences("mira.shake", Context.MODE_PRIVATE).getBoolean("enabled", false)) return@execute
            val message = try {
                TranscriptSync.trigger(context, "shake")
                "Scuturare · acțiunea configurată pe PC a fost trimisă."
            } catch (_: Exception) { "Acțiunea nu a fost confirmată. Verifică PC-ul și preferințele de scuturare." }
            ui.post { if (!closed) Toast.makeText(context, message, Toast.LENGTH_LONG).show() }
        }
    }
    override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
    fun close() { closed = true; listening = false; sensors.unregisterListener(this); io.shutdownNow() }
}
