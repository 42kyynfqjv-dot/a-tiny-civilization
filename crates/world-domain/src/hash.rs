use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// A lowercase-hex SHA-256 digest used for event and state verification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Digest([u8; 32]);

impl Digest {
    pub const ZERO: Self = Self([0; 32]);

    #[must_use]
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn canonical<T>(value: &T) -> Result<Self, CanonicalHashError>
    where
        T: Serialize,
    {
        // Hash schemas are restricted by ADR 0007 to ordered, non-floating types.
        // A golden vector locks the resulting compact JSON bytes for each schema.
        let bytes = serde_json::to_vec(value).map_err(CanonicalHashError::Serialize)?;
        Ok(Self::sha256(&bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Default for Digest {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&hex::encode(self.0))
    }
}

impl FromStr for Digest {
    type Err = hex::FromHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0; 32];
        hex::decode_to_slice(value, &mut bytes)?;
        Ok(Self(bytes))
    }
}

impl Serialize for Digest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        encoded.parse().map_err(de::Error::custom)
    }
}

#[derive(Debug, Error)]
pub enum CanonicalHashError {
    #[error("canonical JSON serialization failed: {0}")]
    Serialize(serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_round_trips_as_lowercase_hex() {
        let digest = Digest::sha256(b"a tiny civilization");
        let decoded = serde_json::to_string(&digest)
            .and_then(|encoded| serde_json::from_str::<Digest>(&encoded));

        assert!(matches!(decoded, Ok(value) if value == digest));
        assert_eq!(digest.to_string().len(), 64);
    }
}
