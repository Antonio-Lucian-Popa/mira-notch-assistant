//! A small, pure Pomodoro (focus timer) state machine.
//!
//! The engine holds no clock of its own: the host calls [`PomodoroEngine::tick`]
//! once a second while running. Keeping it pure makes it trivially testable and
//! lets the daemon own the single timer.

use serde::{Deserialize, Serialize};

use crate::model::{Pomodoro, PomodoroPhase};

/// Durations and cadence for the focus cycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PomodoroConfig {
    pub work_secs: i64,
    pub short_break_secs: i64,
    pub long_break_secs: i64,
    /// A long break replaces the short one after this many completed work phases.
    pub cycles_before_long: u32,
}

impl Default for PomodoroConfig {
    fn default() -> Self {
        PomodoroConfig {
            work_secs: 25 * 60,
            short_break_secs: 5 * 60,
            long_break_secs: 15 * 60,
            cycles_before_long: 4,
        }
    }
}

impl PomodoroConfig {
    fn duration_for(&self, phase: PomodoroPhase) -> i64 {
        match phase {
            PomodoroPhase::Work => self.work_secs,
            PomodoroPhase::ShortBreak => self.short_break_secs,
            PomodoroPhase::LongBreak => self.long_break_secs,
        }
    }
}

/// What a tick did, so the host knows when to emit a phase-change alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickResult {
    /// Nothing happened (not running, or just counted down a second).
    Idle,
    /// A normal one-second decrement.
    Counted,
    /// The phase just rolled over to `new`.
    PhaseChanged(PomodoroPhase),
}

#[derive(Clone, Debug)]
pub struct PomodoroEngine {
    config: PomodoroConfig,
    phase: PomodoroPhase,
    remaining_secs: i64,
    running: bool,
    completed_cycles: u32,
}

impl PomodoroEngine {
    pub fn new(config: PomodoroConfig) -> Self {
        PomodoroEngine {
            phase: PomodoroPhase::Work,
            remaining_secs: config.work_secs,
            running: false,
            completed_cycles: 0,
            config,
        }
    }

    pub fn snapshot(&self) -> Pomodoro {
        Pomodoro {
            phase: self.phase,
            remaining_secs: self.remaining_secs,
            running: self.running,
            completed_cycles: self.completed_cycles,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn pause(&mut self) {
        self.running = false;
    }

    pub fn toggle(&mut self) {
        self.running = !self.running;
    }

    /// Reset to a paused, fresh Work phase and clear the cycle count.
    pub fn reset(&mut self) {
        self.phase = PomodoroPhase::Work;
        self.remaining_secs = self.config.work_secs;
        self.running = false;
        self.completed_cycles = 0;
    }

    /// Jump to the next phase immediately (keeps running state).
    pub fn skip(&mut self) -> PomodoroPhase {
        self.advance_phase();
        self.phase
    }

    /// Advance one second. Returns what happened.
    pub fn tick(&mut self) -> TickResult {
        if !self.running {
            return TickResult::Idle;
        }
        self.remaining_secs -= 1;
        if self.remaining_secs <= 0 {
            self.advance_phase();
            TickResult::PhaseChanged(self.phase)
        } else {
            TickResult::Counted
        }
    }

    fn advance_phase(&mut self) {
        self.phase = match self.phase {
            PomodoroPhase::Work => {
                self.completed_cycles += 1;
                if self
                    .completed_cycles
                    .is_multiple_of(self.config.cycles_before_long)
                {
                    PomodoroPhase::LongBreak
                } else {
                    PomodoroPhase::ShortBreak
                }
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => PomodoroPhase::Work,
        };
        self.remaining_secs = self.config.duration_for(self.phase);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fast() -> PomodoroConfig {
        PomodoroConfig {
            work_secs: 2,
            short_break_secs: 1,
            long_break_secs: 3,
            cycles_before_long: 2,
        }
    }

    #[test]
    fn starts_paused_in_work() {
        let e = PomodoroEngine::new(fast());
        let s = e.snapshot();
        assert_eq!(s.phase, PomodoroPhase::Work);
        assert!(!s.running);
        assert_eq!(s.remaining_secs, 2);
    }

    #[test]
    fn tick_is_idle_when_paused() {
        let mut e = PomodoroEngine::new(fast());
        assert_eq!(e.tick(), TickResult::Idle);
        assert_eq!(e.snapshot().remaining_secs, 2);
    }

    #[test]
    fn counts_down_then_changes_phase() {
        let mut e = PomodoroEngine::new(fast());
        e.start();
        assert_eq!(e.tick(), TickResult::Counted); // 2 -> 1
        assert_eq!(
            e.tick(),
            TickResult::PhaseChanged(PomodoroPhase::ShortBreak)
        ); // -> break
        assert_eq!(e.snapshot().phase, PomodoroPhase::ShortBreak);
        assert_eq!(e.snapshot().remaining_secs, 1);
        assert_eq!(e.snapshot().completed_cycles, 1);
    }

    #[test]
    fn long_break_every_n_cycles() {
        let mut e = PomodoroEngine::new(fast()); // cycles_before_long = 2
        e.start();
        // Cycle 1: work -> short break.
        assert_eq!(e.skip(), PomodoroPhase::ShortBreak);
        // break -> work.
        assert_eq!(e.skip(), PomodoroPhase::Work);
        // Cycle 2: work -> long break (completed_cycles = 2).
        assert_eq!(e.skip(), PomodoroPhase::LongBreak);
        assert_eq!(e.snapshot().completed_cycles, 2);
        assert_eq!(e.snapshot().remaining_secs, 3);
    }

    #[test]
    fn reset_clears_everything() {
        let mut e = PomodoroEngine::new(fast());
        e.start();
        e.skip();
        e.reset();
        let s = e.snapshot();
        assert_eq!(s.phase, PomodoroPhase::Work);
        assert!(!s.running);
        assert_eq!(s.completed_cycles, 0);
        assert_eq!(s.remaining_secs, 2);
    }
}
