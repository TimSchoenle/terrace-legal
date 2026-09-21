//! The locale grammar shared by configuration keys, request parameters and `Accept-Language`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Why a string is not a [`LocaleTag`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseLocaleError {
    /// The input is empty.
    #[error("a locale tag is empty")]
    Empty,
    /// The first subtag is not two or three ASCII letters.
    #[error("the language subtag must be two or three ASCII letters")]
    Language,
    /// A subtag is empty, which happens with `de-` and `de--AT`.
    #[error("a locale tag has an empty subtag")]
    EmptySubtag,
    /// The second subtag is neither a four-letter script nor a region.
    #[error("the second subtag must be a four-letter script or a region")]
    Script,
    /// The last subtag is neither two letters nor three digits.
    #[error("the region subtag must be two ASCII letters or three digits")]
    Region,
    /// More than a language, a script and a region were given.
    #[error("a locale tag has at most three subtags")]
    TooLong,
}

/// A locale in canonical case: a language, an optional script and an optional region.
///
/// The grammar is smaller than BCP 47 on purpose. It takes a two or three letter language, an
/// optional four letter script and an optional two letter or three digit region, so `de`,
/// `zh-Hant` and `de-AT` are tags and `de-CH-1996` is not. `_` is accepted for `-`, because an
/// environment variable name cannot contain a hyphen. Parsing normalises the case, so `de_at`,
/// `DE-at` and `de-AT` are the same value.
///
/// Ordering is by canonical text and is stable across releases, which the negotiation fallback
/// relies on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct LocaleTag {
    text: String,
    language_end: usize,
    script_end: usize,
}

impl LocaleTag {
    /// Returns the lowercase language subtag.
    #[must_use]
    pub fn language(&self) -> &str {
        &self.text[..self.language_end]
    }

    /// Returns the canonical text, for example `de-AT`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Returns the tag with its last subtag removed, or `None` for a bare language.
    ///
    /// `de-Latn-AT` truncates to `de-Latn`, which truncates to `de`.
    #[must_use]
    pub fn truncated(&self) -> Option<Self> {
        let end = if self.script_end < self.text.len() {
            // A region is present: drop it. The script, if any, stays.
            self.script_end
        } else if self.language_end < self.text.len() {
            // Only a script or only a region follows the language.
            self.language_end
        } else {
            return None;
        };
        Some(Self {
            text: self.text[..end].to_owned(),
            language_end: self.language_end,
            script_end: end.min(self.script_end),
        })
    }

    /// Returns whether both tags carry the same language subtag.
    #[must_use]
    pub fn same_language(&self, other: &Self) -> bool {
        self.language() == other.language()
    }
}

impl FromStr for LocaleTag {
    type Err = ParseLocaleError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input.is_empty() {
            return Err(ParseLocaleError::Empty);
        }
        let mut parts = input.split(['-', '_']);
        let language = parts.next().unwrap_or_default();
        if language.is_empty() {
            return Err(ParseLocaleError::EmptySubtag);
        }
        if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_alphabetic())
        {
            return Err(ParseLocaleError::Language);
        }

        let mut text = language.to_ascii_lowercase();
        let language_end = text.len();
        let mut script_end = language_end;
        let mut next = parts.next();

        if let Some(part) = next {
            if part.is_empty() {
                return Err(ParseLocaleError::EmptySubtag);
            }
            if part.len() == 4 {
                if !part.bytes().all(|b| b.is_ascii_alphabetic()) {
                    return Err(ParseLocaleError::Script);
                }
                text.push('-');
                let mut chars = part.chars();
                if let Some(first) = chars.next() {
                    text.push(first.to_ascii_uppercase());
                }
                text.extend(chars.map(|c| c.to_ascii_lowercase()));
                script_end = text.len();
                next = parts.next();
            }
        }

        if let Some(part) = next {
            if part.is_empty() {
                return Err(ParseLocaleError::EmptySubtag);
            }
            let letters = part.len() == 2 && part.bytes().all(|b| b.is_ascii_alphabetic());
            let digits = part.len() == 3 && part.bytes().all(|b| b.is_ascii_digit());
            if !(letters || digits) {
                return Err(if script_end == language_end {
                    ParseLocaleError::Script
                } else {
                    ParseLocaleError::Region
                });
            }
            text.push('-');
            text.push_str(&part.to_ascii_uppercase());
            if parts.next().is_some() {
                return Err(ParseLocaleError::TooLong);
            }
        }

        Ok(Self {
            text,
            language_end,
            script_end,
        })
    }
}

impl TryFrom<String> for LocaleTag {
    type Error = ParseLocaleError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<LocaleTag> for String {
    fn from(tag: LocaleTag) -> Self {
        tag.text
    }
}

impl fmt::Display for LocaleTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl AsRef<str> for LocaleTag {
    fn as_ref(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(input: &str) -> LocaleTag {
        input.parse().expect("a valid tag")
    }

    fn truncations(start: &LocaleTag) -> Vec<String> {
        let mut out = Vec::new();
        let mut current = start.truncated();
        while let Some(next) = current {
            out.push(next.to_string());
            current = next.truncated();
        }
        out
    }

    #[test]
    fn case_is_normalised_and_underscore_is_accepted() {
        for (input, canonical) in [
            ("de", "de"),
            ("DE", "de"),
            ("de_at", "de-AT"),
            ("DE-at", "de-AT"),
            ("de-AT", "de-AT"),
            ("zh-hant", "zh-Hant"),
            ("ZH_HANT_tw", "zh-Hant-TW"),
            ("es-419", "es-419"),
            ("fil", "fil"),
        ] {
            assert_eq!(tag(input).as_str(), canonical, "input {input}");
        }
    }

    #[test]
    fn malformed_input_is_rejected() {
        for input in [
            "",
            "e",
            "english",
            "de-",
            "-de",
            "de--AT",
            "de-A",
            "de-ATT",
            "de-AT-x",
            "12",
            "d3",
            "de-Lat1",
            "de-Latn-",
            "de-Latn-AT-1",
            "de-CH-1996",
            " de",
            "de ",
        ] {
            assert!(input.parse::<LocaleTag>().is_err(), "input {input:?}");
        }
    }

    #[test]
    fn truncation_drops_the_region_then_the_script() {
        assert!(truncations(&tag("de")).is_empty());
        assert_eq!(truncations(&tag("de-Latn-AT")), ["de-Latn", "de"]);
        assert_eq!(truncations(&tag("de-AT")), ["de"]);
        assert_eq!(truncations(&tag("zh-Hant")), ["zh"]);
    }

    #[test]
    fn a_truncated_tag_equals_the_tag_parsed_from_the_same_text() {
        assert_eq!(tag("de-Latn-AT").truncated(), Some(tag("de-Latn")));
        assert_eq!(tag("de-AT").truncated(), Some(tag("de")));
    }

    #[test]
    fn same_language_ignores_script_and_region() {
        assert!(tag("de-AT").same_language(&tag("de")));
        assert!(!tag("de").same_language(&tag("en")));
        assert_eq!(tag("de-AT").language(), "de");
    }

    #[test]
    fn serde_round_trips_through_the_canonical_string() {
        let json = serde_json::to_string(&tag("de_at")).expect("serialises");
        assert_eq!(json, "\"de-AT\"");
        let back: LocaleTag = serde_json::from_str("\"DE-at\"").expect("deserialises");
        assert_eq!(back, tag("de-AT"));
        assert!(serde_json::from_str::<LocaleTag>("\"english\"").is_err());
    }
}
