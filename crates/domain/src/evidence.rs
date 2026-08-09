use std::fmt;

/// Opaque locator for evidence retained by a quality or verification
/// workflow.
///
/// The domain deliberately does not interpret this value as a URL, file path,
/// or fetch instruction. Source kind, reliability, and any external lookup
/// policy belong to later source increments and their application adapters.
/// Keeping the value opaque prevents a persisted reference from becoming an
/// implicit network or filesystem authority.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceReference(String);

impl EvidenceReference {
    /// Maximum UTF-8 byte length accepted by the domain value object.
    ///
    /// Transport adapters may apply a stricter limit, but they must not
    /// truncate an accepted reference.
    pub const MAX_BYTES: usize = 2_048;

    pub fn try_new(value: impl Into<String>) -> Result<Self, EvidenceReferenceError> {
        let value = value.into();
        let byte_length = value.len();
        if value.trim().is_empty() {
            return Err(EvidenceReferenceError::Empty);
        }
        if byte_length > Self::MAX_BYTES {
            return Err(EvidenceReferenceError::TooLong {
                actual_bytes: byte_length,
                max_bytes: Self::MAX_BYTES,
            });
        }
        if let Some((position, _)) = value
            .char_indices()
            .find(|(_, character)| character.is_control())
        {
            return Err(EvidenceReferenceError::ContainsControlCharacter { position });
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EvidenceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceReferenceError {
    Empty,
    TooLong {
        actual_bytes: usize,
        max_bytes: usize,
    },
    ContainsControlCharacter {
        position: usize,
    },
}

impl fmt::Display for EvidenceReferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("evidence reference must not be empty"),
            Self::TooLong {
                actual_bytes,
                max_bytes,
            } => write!(
                formatter,
                "evidence reference is {actual_bytes} bytes, maximum is {max_bytes}"
            ),
            Self::ContainsControlCharacter { position } => write!(
                formatter,
                "evidence reference contains a control character at byte offset {position}"
            ),
        }
    }
}

impl std::error::Error for EvidenceReferenceError {}

#[cfg(test)]
mod tests {
    use super::{EvidenceReference, EvidenceReferenceError};

    #[test]
    fn accepts_and_preserves_an_opaque_reference() {
        let reference =
            EvidenceReference::try_new("https://pfl.uz/match/2026-001?document=protocol#final")
                .expect("reference should be valid");

        assert_eq!(
            reference.as_str(),
            "https://pfl.uz/match/2026-001?document=protocol#final"
        );
        assert_eq!(reference.to_string(), reference.as_str());
    }

    #[test]
    fn does_not_require_a_url_scheme_or_normalize_source_text() {
        let reference = EvidenceReference::try_new(" PFL protocol ref 2026/001 ")
            .expect("opaque source-native references are valid");

        assert_eq!(reference.as_str(), " PFL protocol ref 2026/001 ");
    }

    #[test]
    fn rejects_only_whitespace() {
        assert_eq!(
            EvidenceReference::try_new(" \t\n"),
            Err(EvidenceReferenceError::Empty)
        );
    }

    #[test]
    fn rejects_control_characters_without_returning_the_reference() {
        assert_eq!(
            EvidenceReference::try_new("pfl-protocol\n2026-001"),
            Err(EvidenceReferenceError::ContainsControlCharacter { position: 12 })
        );
    }

    #[test]
    fn enforces_the_utf8_byte_limit_without_truncation() {
        let reference = "x".repeat(EvidenceReference::MAX_BYTES);
        let accepted = EvidenceReference::try_new(reference.clone())
            .expect("maximum-length reference should be accepted");
        assert_eq!(accepted.as_str(), reference);

        let over_limit = format!("{}x", "x".repeat(EvidenceReference::MAX_BYTES));
        assert_eq!(
            EvidenceReference::try_new(over_limit),
            Err(EvidenceReferenceError::TooLong {
                actual_bytes: EvidenceReference::MAX_BYTES + 1,
                max_bytes: EvidenceReference::MAX_BYTES,
            })
        );
    }

    #[test]
    fn counts_multibyte_source_text_as_utf8_bytes() {
        let reference = "o".repeat(EvidenceReference::MAX_BYTES - 1) + "ʻ";

        assert_eq!(
            EvidenceReference::try_new(reference),
            Err(EvidenceReferenceError::TooLong {
                actual_bytes: EvidenceReference::MAX_BYTES + 1,
                max_bytes: EvidenceReference::MAX_BYTES,
            })
        );
    }
}
