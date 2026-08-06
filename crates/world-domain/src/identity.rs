use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
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

impl EntityId {
    /// Derives a stable entity identifier from a world and ruleset-owned name.
    #[must_use]
    pub fn deterministic(world_id: WorldId, name: &[u8]) -> Self {
        Self(Uuid::new_v5(&world_id.as_uuid(), name))
    }
}

impl EventId {
    /// Derives a stable identifier from a batch sequence and zero-based event index.
    #[must_use]
    pub fn for_position(world_id: WorldId, sequence: u64, index: u32) -> Self {
        let name = format!("event:{sequence}:{index}");
        Self(Uuid::new_v5(&world_id.as_uuid(), name.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_round_trip_through_text() {
        let raw = Uuid::from_u128(0x018f_3f62_d1a8_7b63_8b9d_15ec_0d11_f007);
        let id = WorldId::from_uuid(raw);
        let parsed = id.to_string().parse::<WorldId>();
        assert_eq!(parsed, Ok(id));
    }

    #[test]
    fn event_identity_is_stable_and_position_scoped() {
        let world = WorldId::from_uuid(Uuid::from_u128(7));
        assert_eq!(
            EventId::for_position(world, 9, 2),
            EventId::for_position(world, 9, 2)
        );
        assert_ne!(
            EventId::for_position(world, 9, 2),
            EventId::for_position(world, 9, 3)
        );
    }
}
