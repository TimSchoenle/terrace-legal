//! Strong entity tags and the `If-None-Match` comparison.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest as _, Sha256};

/// A strong entity tag: 22 URL-safe base64 characters between quotes.
///
/// The tag is the first 128 bits of a SHA-256 over the parts it was built from, so it is stable
/// across builds, platforms and replicas, and two representations with different bytes never
/// share one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ETag(String);

impl ETag {
    /// Builds a tag over `parts`.
    ///
    /// Each part is preceded by its length, so `["ab", "c"]` and `["a", "bc"]` differ.
    #[must_use]
    pub fn of(parts: &[&[u8]]) -> Self {
        let mut hasher = Sha256::new();
        for part in parts {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part);
        }
        let hash: [u8; 32] = hasher.finalize().into();
        let mut text = String::with_capacity(24);
        text.push('"');
        URL_SAFE_NO_PAD.encode_string(&hash[..16], &mut text);
        text.push('"');
        Self(text)
    }

    /// Returns the tag including its quotes, as it is sent in an `ETag` header.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether an `If-None-Match` header value names this tag.
    ///
    /// The comparison is weak, as RFC 9110 section 13.1.2 requires for `If-None-Match`, so a `W/`
    /// prefix on a listed tag is ignored. `*` matches any tag. Anything that is not a list of
    /// entity tags matches nothing.
    #[must_use]
    pub fn matches_if_none_match(&self, header: &str) -> bool {
        let mut rest = header.trim_start();
        if rest.trim_end() == "*" {
            return true;
        }
        loop {
            rest = rest.trim_start_matches([' ', '\t', ',']);
            if rest.is_empty() {
                return false;
            }
            let tag = rest.strip_prefix("W/").unwrap_or(rest);
            let Some(body) = tag.strip_prefix('"') else {
                return false;
            };
            let Some(end) = body.find('"') else {
                return false;
            };
            // `body[..end]` is the text between the quotes, and the tag is compared with them
            // restored so that it matches the stored, quoted form.
            if format!("\"{}\"", &body[..end]) == self.0 {
                return true;
            }
            rest = &body[end + 1..];
        }
    }
}

impl fmt::Display for ETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag() -> ETag {
        ETag::of(&[b"terms", b"en"])
    }

    /// The hash algorithm was unspecified across Rust releases, so replicas built by different
    /// compilers produced different tags for the same text.
    #[test]
    fn a_tag_is_stable_across_builds() {
        assert_eq!(tag().as_str(), "\"-RjOf4S09-Lke8qQa5wXJQ\"");
        assert_eq!(tag(), ETag::of(&[b"terms", b"en"]));
    }

    #[test]
    fn a_tag_is_quoted_and_22_characters() {
        let text = tag().to_string();
        assert!(text.starts_with('"') && text.ends_with('"'));
        assert_eq!(text.len(), 24);
    }

    #[test]
    fn part_boundaries_matter() {
        assert_ne!(ETag::of(&[b"ab", b"c"]), ETag::of(&[b"a", b"bc"]));
        assert_ne!(ETag::of(&[b"a", b""]), ETag::of(&[b"a"]));
    }

    #[test]
    fn if_none_match_accepts_exact_weak_list_and_wildcard() {
        let tag = tag();
        let text = tag.as_str().to_owned();
        for header in [
            text.clone(),
            format!("W/{text}"),
            format!("\"other\", {text}"),
            format!("\"other\",{text}, \"third\""),
            format!("W/\"other\" , W/{text}"),
            format!("  {text}  "),
            "*".to_owned(),
            " * ".to_owned(),
        ] {
            assert!(tag.matches_if_none_match(&header), "header {header:?}");
        }
    }

    #[test]
    fn if_none_match_rejects_everything_else() {
        let tag = tag();
        for header in [
            "",
            " ",
            ",",
            "\"other\"",
            "W/\"other\"",
            "unquoted",
            "\"unterminated",
            "*, \"x\"",
            "**",
            &tag.as_str()[1..],
        ] {
            assert!(!tag.matches_if_none_match(header), "header {header:?}");
        }
    }
}
