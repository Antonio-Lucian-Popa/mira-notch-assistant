package ro.novabytecode.mira.companion

import android.graphics.*

/** The approved camera-to-eyes silhouette, shared by the overlay and visual tests. */
internal object CameraNotchRenderer {
    fun draw(canvas: Canvas, layout: CameraNotchLayout, paint: Paint) {
        val w = layout.width.toFloat(); val h = layout.height.toFloat()
        val wing = layout.wing; val y = layout.shoulderY; val radius = 18f * layout.unit
        val shape = Path().apply {
            moveTo(0f, 0f); lineTo(w, 0f); lineTo(w, y)
            cubicTo(w-wing*.6f,y,w-wing,y+wing*.35f,w-wing,y+wing)
            lineTo(w-wing,h-radius)
            quadTo(w-wing,h,w-wing-radius,h)
            lineTo(wing+radius,h)
            quadTo(wing,h,wing,h-radius)
            lineTo(wing,y+wing)
            cubicTo(wing,y+wing*.35f,wing*.6f,y,0f,y)
            close()
        }
        paint.shader = null; paint.style = Paint.Style.FILL; paint.color = Color.rgb(9,9,11)
        canvas.drawPath(shape, paint)
        val gradient = RadialGradient(0f,0f,1f,
            intArrayOf(Color.argb(217,70,230,240),Color.argb(102,38,150,215),Color.TRANSPARENT,Color.TRANSPARENT),
            floatArrayOf(0f,.4f,.72f,1f), Shader.TileMode.CLAMP)
        gradient.setLocalMatrix(Matrix().apply { setScale(w*1.2f,h*.7f); postTranslate(w/2,h*1.18f) })
        canvas.save();canvas.clipPath(shape);paint.shader=gradient;canvas.drawPaint(paint);canvas.restore();paint.shader=null
    }
}
