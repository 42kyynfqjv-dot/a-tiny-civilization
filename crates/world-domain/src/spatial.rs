use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;

pub const MAX_S2_LEVEL: u8 = 30;
/// Refuse pathological allocations when a caller asks to materialize a large subtree.
pub const MAX_MATERIALIZED_S2_DESCENDANTS: usize = 65_536;

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

    /// Return the four immediately finer S2 children in canonical CellId order.
    pub fn children(self) -> Result<[Self; 4], S2CellIdError> {
        let level = self.level();
        if level >= MAX_S2_LEVEL {
            return Err(S2CellIdError::NoChildrenAtMaximumLevel);
        }
        let parent_lsb = 1_u64 << (2 * u32::from(MAX_S2_LEVEL - level));
        let child_lsb = parent_lsb >> 2;
        let base = self.0 - parent_lsb;
        Ok([
            Self::new(base + child_lsb)?,
            Self::new(base + 3 * child_lsb)?,
            Self::new(base + 5 * child_lsb)?,
            Self::new(base + 7 * child_lsb)?,
        ])
    }

    /// Materialize every descendant at `target_level` in strict CellId order.
    ///
    /// This is intended for bounded causal refinement steps such as L10→L14, not
    /// for enumerating an entire planetary hierarchy. Larger requests fail before
    /// allocating memory, and callers that need broader scans must stream explicitly.
    pub fn descendants_at(self, target_level: u8) -> Result<Vec<Self>, S2CellIdError> {
        let level = self.level();
        if target_level < level {
            return Err(S2CellIdError::CoarserDescendantLevel {
                requested: target_level,
                cell_level: level,
            });
        }
        if target_level > MAX_S2_LEVEL {
            return Err(S2CellIdError::DescendantLevelOutOfRange(target_level));
        }
        let levels_down = u32::from(target_level - level);
        let count = 1_usize.checked_shl(2 * levels_down).ok_or(
            S2CellIdError::DescendantEnumerationTooLarge {
                requested: target_level,
                maximum: MAX_MATERIALIZED_S2_DESCENDANTS,
            },
        )?;
        if count > MAX_MATERIALIZED_S2_DESCENDANTS {
            return Err(S2CellIdError::DescendantEnumerationTooLarge {
                requested: target_level,
                maximum: MAX_MATERIALIZED_S2_DESCENDANTS,
            });
        }

        let mut descendants = vec![self];
        for _ in level..target_level {
            let mut next = Vec::with_capacity(descendants.len().checked_mul(4).ok_or(
                S2CellIdError::DescendantEnumerationTooLarge {
                    requested: target_level,
                    maximum: MAX_MATERIALIZED_S2_DESCENDANTS,
                },
            )?);
            for parent in descendants {
                next.extend(parent.children()?);
            }
            descendants = next;
        }
        Ok(descendants)
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
    #[error("an S2 CellId at level 30 has no children")]
    NoChildrenAtMaximumLevel,
    #[error("S2 ancestor level {requested} is finer than cell level {cell_level}")]
    FinerAncestor { requested: u8, cell_level: u8 },
    #[error("S2 descendant level {requested} is coarser than cell level {cell_level}")]
    CoarserDescendantLevel { requested: u8, cell_level: u8 },
    #[error("S2 descendant level {0} is outside 0 through 30")]
    DescendantLevelOutOfRange(u8),
    #[error("materializing descendants at level {requested} exceeds the maximum of {maximum}")]
    DescendantEnumerationTooLarge { requested: u8, maximum: usize },
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

    #[test]
    fn children_are_ordered_and_round_trip_to_their_parent() {
        let parent = "1000010000000000"
            .parse::<S2CellId>()
            .expect("valid level-10 parent");
        let children = parent.children().expect("parent has children");
        assert!(children.windows(2).all(|pair| pair[0] < pair[1]));
        for child in children {
            assert_eq!(child.level(), 11);
            assert_eq!(child.ancestor(10).expect("parent level"), parent);
        }
        let leaf = S2CellId::new(0x1000_0000_0000_0001).expect("valid level-30 leaf");
        assert!(leaf.children().is_err());
    }

    #[test]
    fn bounded_descendant_enumeration_is_complete_ordered_and_fails_closed() {
        for face in 0_u64..6 {
            let parent =
                S2CellId::new((face << 61) | 0x0000_0100_0000_0000).expect("valid level-10 parent");
            assert_eq!(parent.level(), 10);
            let descendants = parent.descendants_at(14).expect("bounded descendants");
            assert_eq!(descendants.len(), 256);
            assert!(descendants.windows(2).all(|pair| pair[0] < pair[1]));
            assert!(descendants.iter().all(|cell| cell.level() == 14));
            assert!(descendants.iter().all(|cell| parent.contains(*cell)));
        }

        let parent = "1000010000000000"
            .parse::<S2CellId>()
            .expect("valid level-10 parent");
        assert!(matches!(
            parent.descendants_at(9),
            Err(S2CellIdError::CoarserDescendantLevel { .. })
        ));
        assert!(matches!(
            parent.descendants_at(31),
            Err(S2CellIdError::DescendantLevelOutOfRange(31))
        ));
        assert!(matches!(
            parent.descendants_at(19),
            Err(S2CellIdError::DescendantEnumerationTooLarge { .. })
        ));
    }
}
