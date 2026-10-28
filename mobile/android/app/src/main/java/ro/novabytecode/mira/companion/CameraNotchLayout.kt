package ro.novabytecode.mira.companion

import kotlin.math.ceil
import kotlin.math.max
import kotlin.math.min

/** Screen-pixel geometry: keep the top attached and both eyes below the camera. */
internal data class CameraNotchLayout(
    val left: Int,
    val width: Int,
    val height: Int,
    val shoulderY: Float,
    val wing: Float,
    val eyeY: Float,
    val unit: Float,
) {
    companion object {
        fun calculate(density: Float, scale: Float, screenWidth: Int, cameraCenter: Float,
                      cameraBottom: Float, statusBottom: Float, offsetX: Int, offsetY: Int): CameraNotchLayout {
            val unit = density * scale.coerceIn(.85f, 1.15f)
            val width = min(168f * unit, max(120f * density, screenWidth - 112f * density)).toInt().coerceAtMost(screenWidth)
            val left = (cameraCenter - width / 2f + offsetX * density).toInt().coerceIn(0, max(0, screenWidth - width))
            val shoulder = max(cameraBottom + 4f * density, max(statusBottom, cameraBottom + 4f * density) + offsetY * density)
            val eyes = shoulder + 19f * unit
            return CameraNotchLayout(left, width, ceil(eyes + 23f * unit).toInt(), shoulder, 22f * unit, eyes, unit)
        }
    }
}
