//! The configuration types an operator writes.
//!
//! Field documentation here is published: `Describe` renders each `///` into the configuration
//! reference, the example file and the contract, for a reader who never sees this source. Reasoning
//! about a field is therefore in plain `//` comments.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use terrace_config::schema::Describe;

/// The legal documents a deployment publishes.
///
/// This is only the deserialised input. Nothing can be served until
/// [`crate::Catalog::build`] has validated it.
// Closed, while `documents` stays an open map: a leftover key from a configuration that predates
// this one is refused by name instead of silently ignored.
#[derive(Debug, Clone, Default, Deserialize, Serialize, Describe)]
#[serde(default, deny_unknown_fields)]
pub struct LegalConfig {
    /// Locale served when neither the request nor its `Accept-Language` header matches a
    /// published one, for example `en`. Without it, the first published locale in alphabetical
    /// order is served.
    pub default_locale: Option<String>,
    /// The published documents, keyed by the slug their URL uses. A slug is lowercase letters,
    /// digits, `_` and `-`, at most 64 characters, and starts with a letter or a digit.
    #[config(element)]
    pub documents: BTreeMap<String, LegalDocument>,
}

/// One published document.
// `deny_unknown_fields` is load-bearing in two directions. It refuses a misspelt `updatd`, and
// without the `consent` feature it refuses a `consent` key too: an operator who writes a consent
// policy for a build that cannot enforce it hears about it at boot.
#[derive(Debug, Clone, Default, Deserialize, Serialize, Describe)]
#[serde(default, deny_unknown_fields)]
pub struct LegalDocument {
    /// Markdown text per locale, keyed by locale (`en`, `de-AT`). Usually supplied through a
    /// `…__BODY__<LOCALE>_FILE` variable naming a mounted file. Each text is at most 1 MiB.
    /// Exclusive with `url`.
    pub body: BTreeMap<String, String>,
    /// Absolute `http` or `https` URL of a document hosted elsewhere, without credentials.
    /// Exclusive with `body`.
    pub url: Option<String>,
    /// The "last updated" line, displayed verbatim.
    pub updated: Option<String>,
    /// Display title per locale, keyed like `body`.
    pub title: BTreeMap<String, String>,
    /// Position in the index, lowest first. Documents with the same value are ordered by slug.
    pub order: i32,
    /// What a reader has to do with the document.
    #[cfg(feature = "consent")]
    #[config(nested)]
    pub consent: ConsentPolicy,
}

/// The consent policy of one document.
// A defaulted struct rather than an `Option`, so the `Describe` output stays a plain nested
// table. A requirement of `none` is the off state.
#[cfg(feature = "consent")]
#[derive(Debug, Clone, Default, Deserialize, Serialize, Describe)]
#[serde(default, deny_unknown_fields)]
pub struct ConsentPolicy {
    /// `none`, `acknowledge` or `accept`. `acknowledge` shows and records the document without
    /// ever blocking. `accept` has to be accepted at registration, and by existing users once
    /// the grace period ends. Anything but `none` needs `version` and a hosted `body`.
    // The variants belong to a type this crate does not own, so `Describe` cannot derive them.
    // `requirement_spellings_match_the_model` fails if this list and the type disagree.
    #[config(values("none", "acknowledge", "accept"))]
    pub requirement: terrace_legal_model::consent::Requirement,
    /// The operator's revision of the document. Bump it for a material change: readers are asked
    /// again only when it changes, so an editorial fix that keeps the version asks nobody.
    pub version: Option<String>,
    /// The date `version` took effect, as `YYYY-MM-DD`. It starts the grace period.
    pub effective: Option<String>,
    /// Days after `effective` during which existing users may continue without accepting.
    /// Needs `effective` when above zero.
    #[config(range(min = 0, max = 365))]
    pub grace_days: u32,
}

#[cfg(all(test, feature = "consent"))]
mod tests {
    use super::*;

    /// A hand-written list of variants can drift from the type it describes.
    #[test]
    fn requirement_spellings_match_the_model() {
        let spellings: Vec<&str> = terrace_legal_model::consent::Requirement::ALL
            .iter()
            .map(|requirement| requirement.as_str())
            .collect();
        assert_eq!(spellings, ["none", "acknowledge", "accept"]);
        for requirement in terrace_legal_model::consent::Requirement::ALL {
            let json = serde_json::to_string(&requirement).expect("serialises");
            assert_eq!(json, format!("\"{}\"", requirement.as_str()));
        }
        assert_eq!(ConsentPolicy::default().grace_days, 0);
    }
}
