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
    @Test fun consecutiveTripletsTriggerWithoutACommandCooldown() {
        val d=ShakeDetector()
        assertFalse(peak(d,100));assertFalse(peak(d,350));assertTrue(peak(d,600))
        assertFalse(peak(d,730));assertFalse(peak(d,860));assertTrue(peak(d,990))
        assertFalse(peak(d,1120));assertFalse(peak(d,1250));assertTrue(peak(d,1380))
    }
    @Test fun repeatedSensorEventsWithinOneMovementAreDebounced() {
        val d=ShakeDetector()
        assertFalse(peak(d,100))
        assertFalse(peak(d,120));assertFalse(peak(d,160));assertFalse(peak(d,200))
        assertFalse(peak(d,230));assertTrue(peak(d,360))
        // Only the same per-peak debounce applies at the triplet boundary.
        assertFalse(peak(d,380));assertFalse(peak(d,420))
        assertFalse(peak(d,490));assertFalse(peak(d,620));assertTrue(peak(d,750))
    }
    @Test fun slowMovementsDoNotAccumulateAcrossSequences() {
        val d=ShakeDetector()
        assertFalse(peak(d,100));assertFalse(peak(d,350))
        assertFalse(peak(d,1600));assertFalse(peak(d,1730));assertTrue(peak(d,1860))
    }
    @Test fun sustainedAccelerationDoesNotCountAsRepeatedShakes() {
        val d=ShakeDetector()
        for(t in 0L..2000L step 20)assertFalse(d.sample(3f,t))
    }
}
