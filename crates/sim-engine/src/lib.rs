//! Pure deterministic state transitions for simulation time and world state.

use serde::{Deserialize, Serialize};
use world_domain::{SimTick, TimeOverflow};

/// Version pinned to each world so old histories are never silently reinterpreted.
pub const RULESET_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineState {
    tick: SimTick,
    ruleset_version: u32,
}

impl EngineState {
    #[must_use]
    pub const fn genesis() -> Self {
        Self {
            tick: SimTick::ZERO,
            ruleset_version: RULESET_VERSION,
        }
    }

    #[must_use]
    pub const fn tick(&self) -> SimTick {
        self.tick
    }

    #[must_use]
    pub const fn ruleset_version(&self) -> u32 {
        self.ruleset_version
    }

    pub fn advance_clock(&mut self) -> Result<SimTick, TimeOverflow> {
        self.tick = self.tick.checked_next()?;
        Ok(self.tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_advances_one_fixed_tick_at_a_time() {
        let mut state = EngineState::genesis();
        assert_eq!(state.advance_clock(), Ok(SimTick::new(1)));
        assert_eq!(state.advance_clock(), Ok(SimTick::new(2)));
        assert_eq!(state.ruleset_version(), RULESET_VERSION);
    }

    #[test]
    fn independent_states_are_reproducible() {
        let mut first = EngineState::genesis();
        let mut second = EngineState::genesis();

        for _ in 0..100 {
            assert_eq!(first.advance_clock(), second.advance_clock());
        }
        assert_eq!(first, second);
    }
}
