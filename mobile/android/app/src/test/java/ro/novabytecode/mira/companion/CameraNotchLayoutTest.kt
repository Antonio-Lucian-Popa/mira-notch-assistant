package ro.novabytecode.mira.companion

import org.junit.Assert.*
import org.junit.Test

class CameraNotchLayoutTest {
    @Test fun eyesAreBelowTheCameraAndWingsFitTheStatusBar() {
        for(scale in listOf(.85f,1f,1.15f)) {
            val g=CameraNotchLayout.calculate(3f,scale,1236,618f,96f,108f,0,0)
            assertEquals(618f,g.left+g.width/2f,1f)
            assertTrue(g.shoulderY>=108f)
            assertTrue(g.eyeY-6*g.unit>96f)
            assertTrue(g.eyeY+6*g.unit<g.height)
            assertTrue(g.width<1236-2*100)
            assertTrue(g.height<300)
            assertTrue(g.height-g.shoulderY<=29*g.unit)
            assertTrue(g.width<=129*g.unit)
        }
    }
    @Test fun upwardCalibrationCannotPutEyesOnTheCamera() {
        val g=CameraNotchLayout.calculate(3f,1f,1236,618f,96f,108f,0,-60)
        assertTrue(g.shoulderY>=108f)
        assertTrue(g.eyeY-6*g.unit>96f)
    }
    @Test fun horizontalCalibrationStaysOnScreen() {
        for(x in listOf(-120,0,120)) {
            val g=CameraNotchLayout.calculate(3f,1f,960,480f,80f,100f,x,0)
            assertTrue(g.left>=0)
            assertTrue(g.left+g.width<=960)
        }
    }
}
