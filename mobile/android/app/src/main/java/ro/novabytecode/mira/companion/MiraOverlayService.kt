package ro.novabytecode.mira.companion

import android.app.*
import android.content.*
import android.content.pm.ServiceInfo
import android.graphics.*
import android.os.*
import android.provider.Settings
import android.view.*
import org.json.JSONObject
import ro.novabytecode.mira.R
import java.lang.ref.WeakReference
import kotlin.math.*

/** Native overlay: no React tree/JS runtime needed to keep the capsule visible. */
class MiraOverlayService : Service() {
    companion object {
        private var active = WeakReference<MiraOverlayService>(null)
        fun running() = active.get() != null
        fun refresh() { active.get()?.let { s -> s.uiHandler.post { s.reload() } } }
        fun preview(expression: String) { active.get()?.let { s -> s.uiHandler.post { s.capsule.expression = expression; s.capsule.previewUntil = SystemClock.uptimeMillis() + 3500; s.capsule.animateFrame() } } }
    }
    private val uiHandler = Handler(Looper.getMainLooper())
    private lateinit var manager: WindowManager
    private lateinit var capsule: Capsule
    private lateinit var params: WindowManager.LayoutParams
    private var attached = false
    private var interactive = true
    private var cameraAligned=false
    private var cameraHole=0f
    private val screen = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            interactive = intent.action != Intent.ACTION_SCREEN_OFF
            if (interactive) capsule.animateFrame() else capsule.stop()
        }
    }
    override fun onBind(intent: Intent?) = null
    override fun onCreate() {
        super.onCreate()
        MiraAlarms.channels(this)
        val stop = PendingIntent.getService(this, 1, Intent(this, MiraOverlayService::class.java).setAction("stop"), PendingIntent.FLAG_IMMUTABLE)
        val notification = Notification.Builder(this, "mira.overlay").setSmallIcon(R.drawable.ic_mira)
            .setContentTitle("Mira e aici").setContentText("Capsula este activă. Atinge pentru setări.")
            .setContentIntent(MiraAlarms.launch(this)).setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Oprește capsula", stop).build()).build()
        if (Build.VERSION.SDK_INT >= 34) startForeground(7, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        else startForeground(7, notification)
        if (!Settings.canDrawOverlays(this)) { stopSelf(); return }
        manager = getSystemService(WindowManager::class.java)
        capsule = Capsule()
        params = WindowManager.LayoutParams(1, 1, WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE, PixelFormat.TRANSLUCENT).apply {
            gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
            // Keep system status controls reachable; only this capsule's bounds receive touch.
            if (Build.VERSION.SDK_INT >= 28) layoutInDisplayCutoutMode = WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_NEVER
        }
        try {
            manager.addView(capsule, params)
            attached = true
            active = WeakReference(this)
            val filter = IntentFilter().apply { addAction(Intent.ACTION_SCREEN_ON); addAction(Intent.ACTION_SCREEN_OFF) }
            if (Build.VERSION.SDK_INT >= 33) registerReceiver(screen, filter, RECEIVER_NOT_EXPORTED) else registerReceiver(screen, filter)
            reload()
        } catch (_: SecurityException) { stopSelf() }
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "stop") stopSelf()
        return START_NOT_STICKY
    }
    private fun reload() {
        if (!attached) return
        if (!Settings.canDrawOverlays(this)) { stopSelf(); return }
        capsule.state = MiraStore(this).snapshot()
        resize()
        capsule.animateFrame()
    }
    private fun resize() {
        val scale=capsule.state.optDouble("scale",1.0).toFloat()
        val cutout=if(Build.VERSION.SDK_INT>=30)manager.currentWindowMetrics.windowInsets.displayCutout else if(Build.VERSION.SDK_INT>=28)capsule.rootWindowInsets?.displayCutout else null
        val hole=cutout?.boundingRects?.filter{it.top<resources.displayMetrics.heightPixels/4&&it.width()>0}?.minByOrNull{it.top}
        cameraAligned=capsule.state.optBoolean("cameraMode",true)&&hole!=null&&hole.width()<dp(96f)&&!capsule.expanded
        if(cameraAligned&&hole!=null){
            cameraHole=max(hole.width(),hole.height())/2f+dp(1f)
            val size=(cameraHole*2+dp(22f)*scale).toInt().coerceAtLeast(dp(40f).toInt())
            params.width=size;params.height=size;params.gravity=Gravity.TOP or Gravity.LEFT
            params.x=(hole.centerX()-size/2+dp(capsule.state.optInt("cameraX").toFloat())).toInt()
            params.y=(hole.centerY()-size/2+dp(capsule.state.optInt("cameraY").toFloat())).toInt()
            params.flags=WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
            if(Build.VERSION.SDK_INT>=30)params.layoutInDisplayCutoutMode=WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
            else if(Build.VERSION.SDK_INT>=28)params.layoutInDisplayCutoutMode=WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
        }else{
            params.width=dp(if(capsule.expanded)300f else 146f*scale).toInt();params.height=dp(if(capsule.expanded)136f else 52f*scale).toInt()
            params.gravity=Gravity.TOP or Gravity.CENTER_HORIZONTAL;params.x=0
            params.flags=WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
            if(Build.VERSION.SDK_INT>=28)params.layoutInDisplayCutoutMode=WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_NEVER
            params.y=dp(capsule.state.optInt("offset",8).toFloat()).toInt()
        }
        manager.updateViewLayout(capsule,params)
    }
    override fun onConfigurationChanged(config:android.content.res.Configuration){super.onConfigurationChanged(config);reload()}
    override fun onDestroy() {
        if (attached) {
            capsule.stop()
            unregisterReceiver(screen)
            manager.removeView(capsule)
            attached = false
        }
        active.clear()
        super.onDestroy()
    }
    private fun dp(value: Float) = value * resources.displayMetrics.density
    private fun openApp() { MiraAlarms.launch(this).send() }

    private inner class Capsule : View(this@MiraOverlayService) {
        var state = JSONObject()
        var expanded = false
        var expression = "happy"
        var previewUntil = 0L
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private var lookX = 0f
        private var lookY = 0f
        private var lastTouch = 0L
        private var downX = 0f
        private var downY = 0f
        private var downTime = 0L
        private var blinkStart = SystemClock.uptimeMillis() + 3000
        private val frame = Runnable { animateFrame() }
        private fun nextReminder(): JSONObject? {
            val items = state.optJSONArray("reminders") ?: return null
            return (0 until items.length()).map { items.getJSONObject(it) }.minByOrNull { it.getLong("due") }
        }
        fun stop() { uiHandler.removeCallbacks(frame) }
        fun animateFrame() {
            stop()
            if (!interactive || !attached) return
            invalidate()
            val now = SystemClock.uptimeMillis()
            val next = nextReminder()
            val urgent = next != null && next.getLong("due") - System.currentTimeMillis() <= 60_000
            val moving = state.optBoolean("motion", true) && (urgent || now < previewUntil || now - lastTouch < 600 || now >= blinkStart)
            val untilBlink = (blinkStart - now).coerceAtLeast(16)
            uiHandler.postDelayed(frame, if (moving) 33 else if (expanded || next != null || state.has("focus")) minOf(1000, untilBlink) else untilBlink)
        }
        override fun onDraw(canvas: Canvas) {
            super.onDraw(canvas)
            if(cameraAligned&&!expanded){
                val now=SystemClock.uptimeMillis();val cx=width/2f;val cy=height/2f
                val motion=state.optBoolean("motion",true)
                val blink=if(motion&&now>=blinkStart)(1f-sin(((now-blinkStart).coerceAtMost(180)/180f)*PI).toFloat()*.92f)else 1f
                if(now-blinkStart>180)blinkStart=now+3500
                val color=if(MiraVoiceService.snapshot().optBoolean("active"))Color.rgb(255,137,158)else Color.rgb(130,222,238)
                paint.color=Color.rgb(14,18,24);canvas.drawCircle(cx,cy,width/2f,paint)
                paint.color=color;paint.style=Paint.Style.STROKE;paint.strokeWidth=dp(4f)
                val radius=cameraHole+dp(5f)
                canvas.drawOval(cx-radius,cy-radius*blink,cx+radius,cy+radius*blink,paint)
                paint.style=Paint.Style.FILL
                paint.xfermode=PorterDuffXfermode(PorterDuff.Mode.CLEAR);canvas.drawCircle(cx,cy,cameraHole,paint);paint.xfermode=null
                return
            }
            val now = SystemClock.uptimeMillis()
            val next = nextReminder()
            val remaining = next?.let { it.getLong("due") - System.currentTimeMillis() }
            val urgent = remaining != null && remaining <= 60_000
            val motion = state.optBoolean("motion", true)
            val theme = state.optString("theme", "classic")
            val color = if (urgent) Color.rgb(255, 137, 158) else when(theme) {
                "robot" -> Color.rgb(133, 239, 183); "cat" -> Color.rgb(255, 205, 130)
                "pixel" -> Color.rgb(190, 164, 255); else -> Color.rgb(130, 222, 238)
            }
            val w = width.toFloat(); val h = height.toFloat()
            paint.color = Color.rgb(14, 18, 24)
            canvas.drawRoundRect(0f, 0f, w, h, dp(26f), dp(26f), paint)
            paint.style = Paint.Style.STROKE; paint.strokeWidth = dp(1f); paint.color = Color.argb(100, Color.red(color), Color.green(color), Color.blue(color))
            canvas.drawRoundRect(dp(0.5f), dp(0.5f), w-dp(0.5f), h-dp(0.5f), dp(26f), dp(26f), paint)
            paint.style = Paint.Style.FILL
            val eyeY = if (expanded) dp(27f) else h * 0.45f
            val blink = if (motion && now >= blinkStart) (1f - sin(((now - blinkStart).coerceAtMost(180) / 180f) * PI).toFloat() * 0.94f) else 1f
            if (now - blinkStart > 180) blinkStart = now + 3500
            val activePreview = now < previewUntil
            val jitter = if (motion && urgent) sin(now / 95.0).toFloat() * dp(2f) else 0f
            val dx = if (now - lastTouch < 2000) lookX * dp(4f) else 0f
            val dy = if (now - lastTouch < 2000) lookY * dp(2f) else 0f
            for (i in 0..1) {
                val cx = w / 2 + (if (i == 0) -1 else 1) * dp(20f) + dx + jitter
                val wink = activePreview && expression == "wink" && i == 1
                val sleepy = activePreview && expression == "sleepy"
                val eyeH = dp(if (sleepy) 3f else if (activePreview && expression == "excited") 19f else 14f) * if (wink) 0.13f else blink
                paint.color = color
                val radius = dp(if (theme == "pixel" || theme == "robot") 2f else 8f)
                canvas.drawRoundRect(cx-dp(7f), eyeY+dy-eyeH/2, cx+dp(7f), eyeY+dy+eyeH/2, radius, radius, paint)
                if (theme == "cat" && !wink && blink > .7) {
                    paint.color = Color.rgb(14, 18, 24)
                    canvas.drawRoundRect(cx-dp(1.5f), eyeY-eyeH/2, cx+dp(1.5f), eyeY+eyeH/2, dp(1f), dp(1f), paint)
                }
            }
            if (expanded) {
                text(canvas, if (urgent) "E timpul pentru tine" else "Mira e aici", w/2, dp(58f), 12f, color)
                val label = next?.getString("title") ?: if (state.has("focus")) "Sesiune de focus" else "O zi bună, pas cu pas."
                val clipped = android.text.TextUtils.ellipsize(label, android.text.TextPaint().apply { textSize = dp(14f) }, w-dp(32f), android.text.TextUtils.TruncateAt.END).toString()
                text(canvas, clipped, w/2, dp(82f), 14f, Color.WHITE)
                val subtitle = if (remaining != null) if (remaining <= 0) "Atinge pentru a gestiona reminderul" else "Mai sunt ${formatDuration(remaining)} · Deschide Mira"
                    else "Atinge pentru a deschide Mira"
                text(canvas, subtitle, w/2, dp(111f), 11f, Color.rgb(160, 174, 185))
                text(canvas, "×", w-dp(22f), dp(27f), 20f, Color.GRAY)
            } else {
                val focus = state.optJSONObject("focus")
                val label = if (remaining != null && remaining <= 300_000) { if (remaining <= 0) "E timpul" else formatDuration(remaining) }
                    else if (focus != null) { if (focus.optBoolean("paused")) "Pauză" else formatDuration(focus.getLong("due")-System.currentTimeMillis()) }
                    else "mira"
                text(canvas, label, w/2, h-dp(7f), 9f, color)
            }
        }
        private fun text(canvas: Canvas, value: String, x: Float, y: Float, size: Float, color: Int) {
            paint.color = color; paint.textSize = dp(size); paint.typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL); paint.textAlign = Paint.Align.CENTER
            canvas.drawText(value, x, y, paint)
        }
        override fun onTouchEvent(event: MotionEvent): Boolean {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> { downX = event.x; downY = event.y; downTime = SystemClock.uptimeMillis() }
                MotionEvent.ACTION_MOVE -> { lookX = ((event.x / width - .5f) * 2).coerceIn(-1f, 1f); lookY = ((event.y / height - .5f)*2).coerceIn(-1f,1f); lastTouch = SystemClock.uptimeMillis(); animateFrame() }
                MotionEvent.ACTION_UP -> {
                    if (abs(event.x-downX)+abs(event.y-downY) < dp(18f)) {
                        if (SystemClock.uptimeMillis()-downTime > 550) openApp()
                        else if (expanded && event.x > width-dp(45f) && event.y < dp(45f)) { expanded = false; resize() }
                        else if (expanded) openApp() else { expanded = true; resize() }
                        performClick()
                    }
                    animateFrame()
                }
            }
            return true
        }
        override fun performClick(): Boolean { super.performClick(); return true }
    }
}
internal fun formatDuration(ms: Long): String {
    val total = (ms.coerceAtLeast(0) + 999) / 1000
    return if (total >= 3600) "${total / 3600}h ${(total % 3600) / 60}m" else "%d:%02d".format(total / 60, total % 60)
}
