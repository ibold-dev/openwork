use std::fmt;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::DomainError;

/// A positive, integer token amount in base units.
///
/// The on-chain program uses `u64` base units. Floating point never touches an
/// amount at any layer: the API accepts and returns decimal strings, validates
/// them here, and stores them as exact decimal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(u64);

impl Amount {
    /// Largest amount the program can represent.
    pub const MAX: Amount = Amount(u64::MAX);

    pub const fn new(value: u64) -> Result<Self, DomainError> {
        if value == 0 {
            return Err(DomainError::InvalidAmount);
        }
        Ok(Amount(value))
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// Parses a canonical decimal string: digits only, no sign, no leading
    /// zeros, value greater than zero.
    pub fn from_decimal_str(raw: &str) -> Result<Self, DomainError> {
        if raw.is_empty() || raw.len() > 20 {
            return Err(DomainError::InvalidAmount);
        }
        let bytes = raw.as_bytes();
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(DomainError::InvalidAmount);
        }
        // "0", "00", "007" are all rejected: amounts are always canonical so
        // that the same value has exactly one textual representation.
        if bytes[0] == b'0' {
            return Err(DomainError::InvalidAmount);
        }
        let value: u64 = raw.parse().map_err(|_| DomainError::InvalidAmount)?;
        Amount::new(value)
    }

    pub fn as_decimal_string(self) -> String {
        self.0.to_string()
    }

    /// Checked sum over amounts. Returns the total as `u64`.
    pub fn checked_sum<I>(amounts: I) -> Result<u64, DomainError>
    where
        I: IntoIterator<Item = Amount>,
    {
        let mut total: u64 = 0;
        for amount in amounts {
            total = total
                .checked_add(amount.get())
                .ok_or(DomainError::AmountOverflow)?;
        }
        Ok(total)
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

impl FromStr for Amount {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Amount::from_decimal_str(s)
    }
}

impl TryFrom<u64> for Amount {
    type Error = DomainError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Amount::new(value)
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.as_decimal_string())
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Amount::from_decimal_str(&raw).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_canonical_positive_integers() {
        assert_eq!(Amount::from_decimal_str("1").unwrap().get(), 1);
        assert_eq!(
            Amount::from_decimal_str("18446744073709551615")
                .unwrap()
                .get(),
            u64::MAX
        );
        assert_eq!(
            Amount::from_decimal_str("1000000").unwrap().to_string(),
            "1000000"
        );
    }

    #[test]
    fn rejects_zero_and_leading_zeros() {
        for raw in ["0", "00", "007", "0123"] {
            assert_eq!(
                Amount::from_decimal_str(raw),
                Err(DomainError::InvalidAmount),
                "{raw} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_non_decimal_and_out_of_range() {
        for raw in [
            "",
            "-1",
            "+1",
            "1.5",
            " 1",
            "1 ",
            "1_000",
            "0x10",
            "18446744073709551616",
        ] {
            assert_eq!(
                Amount::from_decimal_str(raw),
                Err(DomainError::InvalidAmount),
                "{raw} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_zero_value() {
        assert_eq!(Amount::new(0), Err(DomainError::InvalidAmount));
    }

    #[test]
    fn checked_sum_detects_overflow() {
        let a = Amount::new(u64::MAX).unwrap();
        let b = Amount::new(1).unwrap();
        assert_eq!(
            Amount::checked_sum([a, b]),
            Err(DomainError::AmountOverflow)
        );
        assert_eq!(Amount::checked_sum([a]), Ok(u64::MAX));
        assert_eq!(Amount::checked_sum([]), Ok(0));
    }

    #[test]
    fn serde_uses_decimal_strings() {
        let amount = Amount::new(42).unwrap();
        assert_eq!(serde_json::to_string(&amount).unwrap(), "\"42\"");
        let parsed: Amount = serde_json::from_str("\"42\"").unwrap();
        assert_eq!(parsed, amount);
        assert!(serde_json::from_str::<Amount>("42").is_err());
        assert!(serde_json::from_str::<Amount>("\"042\"").is_err());
    }
}
