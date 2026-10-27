package ro.novabytecode.mira.companion

import org.junit.Assert.*
import org.junit.Test

class ShakeDetectorTest {
    private fun peak(d: ShakeDetector, time: Long): Boolean { d.sample(1f, time-1); return d.sample(3f, time) }
    @Test fun normalMotionAndSingleBumpDoNotTrigger() {
        val d=ShakeDetector()
        for(t in 0L..3000L step 20) assertFalse(d.sample(1.2f,t))
        assertFalse(peak(d,3100)); assertFalse(peak(d,5000)); assertFalse(peak(d,7000))
    }
    @Test fun threeSeparateShakesTriggerOnceAndRespectCooldown() {
        val d=ShakeDetector()
        assertFalse(peak(d,100));assertFalse(peak(d,350));assertTrue(peak(d,600))
        assertFalse(peak(d,900));assertFalse(peak(d,1200));assertFalse(peak(d,1500))
        assertFalse(peak(d,16000));assertFalse(peak(d,16300));assertTrue(peak(d,16600))
    }
    @Test fun sustainedAccelerationDoesNotCountAsRepeatedShakes() {
        val d=ShakeDetector()
        for(t in 0L..2000L step 20)assertFalse(d.sample(3f,t))
    }
}
