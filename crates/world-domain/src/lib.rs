//! Durable domain primitives shared by the deterministic engine and its adapters.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

uuid_id!(
    WorldId,
    "Stable identifier for one independently replayable world."
);
uuid_id!(EntityId, "Stable identifier for an entity within a world.");
uuid_id!(EventId, "Stable identifier for a durable domain event.");

/// Seed from which independent deterministic random streams are derived.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorldSeed(u64);

impl WorldSeed {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for WorldSeed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Monotonic simulation time. It is intentionally distinct from wall-clock time.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct SimTick(u64);

impl SimTick {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    pub fn checked_next(self) -> Result<Self, TimeOverflow> {
        self.0.checked_add(1).map(Self).ok_or(TimeOverflow)
    }
}

/// Monotonic sequence for committed event batches within a world.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct EventSequence(u64);

impl EventSequence {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Durable lifecycle state for a world.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorldStatus {
    Initializing,
    Running,
    Extinct,
    Archived,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("simulation time exceeded its representable range")]
pub struct TimeOverflow;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_overflow_is_explicit() {
        let result = SimTick::new(u64::MAX).checked_next();
        assert_eq!(result, Err(TimeOverflow));
    }

    #[test]
    fn identifiers_round_trip_through_text() {
        let raw = Uuid::from_u128(0x018f_3f62_d1a8_7b63_8b9d_15ec_0d11_f007);
        let id = WorldId::from_uuid(raw);
        let parsed = id.to_string().parse::<WorldId>();
        assert_eq!(parsed, Ok(id));
    }

    #[test]
    fn world_status_uses_stable_snake_case_json() {
        let encoded = serde_json::to_string(&WorldStatus::Initializing);
        assert!(matches!(encoded.as_deref(), Ok("\"initializing\"")));
    }
}
