//! The name a document is published under.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// The longest slug, in bytes.
pub const MAX_SLUG_LEN: usize = 64;

/// Why a string is not a [`Slug`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseSlugError {
    /// The input is empty or longer than [`MAX_SLUG_LEN`].
    #[error("a slug is between 1 and {MAX_SLUG_LEN} characters")]
    Length,
    /// The first character is not a lowercase letter or a digit.
    #[error("a slug starts with a lowercase letter or a digit")]
    Start,
    /// A later character is not a lowercase letter, a digit, `_` or `-`.
    #[error("a slug contains only lowercase letters, digits, `_` and `-`")]
    Character,
}

/// The path segment a document is published under, matching `^[a-z0-9][a-z0-9_-]{0,63}$`.
///
/// A slug is a map key and never a filesystem path. `_` is allowed because an environment
/// variable name cannot contain `-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// Returns the slug text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Slug {
    type Err = ParseSlugError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() || input.len() > MAX_SLUG_LEN {
            return Err(ParseSlugError::Length);
        }
        let mut bytes = input.bytes();
        let first = bytes.next().unwrap_or_default();
        if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
            return Err(ParseSlugError::Start);
        }
        if !bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-') {
            return Err(ParseSlugError::Character);
        }
        Ok(Self(input.to_owned()))
    }
}

impl TryFrom<String> for Slug {
    type Error = ParseSlugError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Slug> for String {
    fn from(slug: Slug) -> Self {
        slug.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slugs_are_accepted() {
        for slug in [
            "terms",
            "a",
            "0",
            "privacy_policy",
            "data-protection",
            "a-_-b",
        ] {
            assert!(slug.parse::<Slug>().is_ok(), "slug {slug}");
        }
        assert!("a".repeat(MAX_SLUG_LEN).parse::<Slug>().is_ok());
    }

    #[test]
    fn invalid_slugs_are_refused() {
        for (slug, error) in [
            ("", ParseSlugError::Length),
            ("Terms", ParseSlugError::Start),
            ("-terms", ParseSlugError::Start),
            ("_terms", ParseSlugError::Start),
            ("te rms", ParseSlugError::Character),
            ("te/rms", ParseSlugError::Character),
            ("..%2F..%2Fetc%2Fpasswd", ParseSlugError::Start),
            ("terms.md", ParseSlugError::Character),
            ("tërms", ParseSlugError::Character),
        ] {
            assert_eq!(slug.parse::<Slug>(), Err(error), "slug {slug:?}");
        }
        assert_eq!(
            "a".repeat(MAX_SLUG_LEN + 1).parse::<Slug>(),
            Err(ParseSlugError::Length)
        );
    }
}
