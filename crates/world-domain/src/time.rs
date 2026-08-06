use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

macro_rules! decimal_u64 {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let encoded = String::deserialize(deserializer)?;
                encoded.parse::<u64>().map(Self).map_err(de::Error::custom)
            }
        }
    };
}

decimal_u64!(
    WorldSeed,
    "Seed from which independent deterministic random streams are derived."
);
decimal_u64!(
    SimTick,
    "Monotonic simulation time, intentionally distinct from wall-clock time."
);
decimal_u64!(
    EventSequence,
    "Monotonic sequence for committed event batches within a world."
);

impl SimTick {
    pub const ZERO: Self = Self(0);

    pub fn checked_next(self) -> Result<Self, TimeOverflow> {
        self.0.checked_add(1).map(Self).ok_or(TimeOverflow)
    }
}

impl EventSequence {
    pub const ZERO: Self = Self(0);

    pub fn checked_next(self) -> Result<Self, SequenceOverflow> {
        self.0.checked_add(1).map(Self).ok_or(SequenceOverflow)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("simulation time exceeded its representable range")]
pub struct TimeOverflow;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("event sequence exceeded its representable range")]
pub struct SequenceOverflow;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_overflow_is_explicit() {
        let result = SimTick::new(u64::MAX).checked_next();
        assert_eq!(result, Err(TimeOverflow));
    }

    #[test]
    fn large_values_are_json_strings_for_portable_verification() {
        let encoded = serde_json::to_string(&SimTick::new(u64::MAX));
        assert!(matches!(encoded.as_deref(), Ok("\"18446744073709551615\"")));

        let decoded = serde_json::from_str::<SimTick>("\"18446744073709551615\"");
        assert!(matches!(decoded, Ok(value) if value == SimTick::new(u64::MAX)));
    }
}
