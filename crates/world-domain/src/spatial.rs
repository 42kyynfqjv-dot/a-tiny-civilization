use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

pub const MAX_S2_LEVEL: u8 = 30;

/// One structurally valid 64-bit S2 CellId.
///
/// JSON uses exactly sixteen lowercase hexadecimal characters so ordering and hashes
/// never depend on a language's integer precision or formatting defaults.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct S2CellId(u64);

impl S2CellId {
    pub fn new(value: u64) -> Result<Self, S2CellIdError> {
        let face = value >> 61;
        let trailing_zeros = value.trailing_zeros();
        if face >= 6 || trailing_zeros > 60 || !trailing_zeros.is_multiple_of(2) {
            return Err(S2CellIdError::InvalidStructure(value));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn face(self) -> u8 {
        (self.0 >> 61) as u8
    }

    #[must_use]
    pub fn level(self) -> u8 {
        let trailing_zeros = self.0.trailing_zeros();
        (30 - trailing_zeros / 2) as u8
    }

    pub fn ancestor(self, level: u8) -> Result<Self, S2CellIdError> {
        let cell_level = self.level();
        if level > cell_level {
            return Err(S2CellIdError::FinerAncestor {
                requested: level,
                cell_level,
            });
        }
        let shift = 2 * u32::from(MAX_S2_LEVEL - level);
        let sentinel = 1_u64 << shift;
        Self::new((self.0 & !(sentinel - 1)) | sentinel)
    }

    #[must_use]
    pub fn contains(self, descendant: Self) -> bool {
        descendant.level() >= self.level()
            && descendant
                .ancestor(self.level())
                .is_ok_and(|parent| parent == self)
    }
}

impl fmt::Debug for S2CellId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "S2CellId({self})")
    }
}

impl fmt::Display for S2CellId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

impl FromStr for S2CellId {
    type Err = S2CellIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 16
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(S2CellIdError::InvalidEncoding(value.to_owned()));
        }
        let raw = u64::from_str_radix(value, 16)
            .map_err(|_| S2CellIdError::InvalidEncoding(value.to_owned()))?;
        Self::new(raw)
    }
}

impl Serialize for S2CellId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for S2CellId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse().map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum S2CellIdError {
    #[error("S2 CellId must be exactly sixteen lowercase hexadecimal characters: {0:?}")]
    InvalidEncoding(String),
    #[error("value {0:#018x} is not a structurally valid S2 CellId")]
    InvalidStructure(u64),
    #[error("S2 ancestor level {requested} is finer than cell level {cell_level}")]
    FinerAncestor { requested: u8, cell_level: u8 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_hex_round_trips_and_rejects_noncanonical_or_malformed_values() {
        let cell = "0000000100000000"
            .parse::<S2CellId>()
            .expect("valid level-14 cell");
        assert_eq!(cell.level(), 14);
        assert_eq!(cell.face(), 0);
        assert_eq!(cell.to_string(), "0000000100000000");
        assert!(matches!(
            serde_json::to_string(&cell).as_deref(),
            Ok("\"0000000100000000\"")
        ));
        assert!(matches!(
            serde_json::from_str::<S2CellId>("\"0000000100000000\""),
            Ok(decoded) if decoded == cell
        ));

        for invalid in [
            "000000010000000",
            "00000001000000000",
            "0000000A00000000",
            "0000000000000000",
            "c000000000000000",
            "0000000200000000",
        ] {
            assert!(invalid.parse::<S2CellId>().is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn ancestor_and_containment_are_exact_on_all_six_faces() {
        for face in 0_u64..6 {
            let descendant = S2CellId::new((face << 61) | 0x0000_0001_0000_0000)
                .expect("valid level-14 descendant");
            let partition = descendant.ancestor(10).expect("valid level-10 ancestor");
            let face_root =
                S2CellId::new((face << 61) | 0x1000_0000_0000_0000).expect("valid face root");

            assert_eq!(partition.level(), 10);
            assert_eq!(partition.face(), u8::try_from(face).expect("face fits u8"));
            assert!(partition.contains(descendant));
            assert!(face_root.contains(partition));
            assert!(!partition.contains(face_root));
        }
    }
}
