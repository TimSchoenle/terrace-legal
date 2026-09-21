//! Locale negotiation for a document body or a title.

use crate::accept::AcceptLanguage;
use crate::locale::LocaleTag;

/// Which input decided the negotiated locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchedBy {
    /// The explicit `lang` request parameter.
    Request,
    /// A range of the `Accept-Language` header.
    Header,
    /// The operator's default locale.
    Default,
    /// Nothing matched, so the first available locale in canonical order was used.
    FirstAvailable,
}

/// The outcome of [`negotiate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    /// The chosen locale, always one of the available tags.
    pub locale: LocaleTag,
    /// The input that chose it. A client shows "shown in English" only when this is not
    /// [`MatchedBy::Request`].
    pub matched_by: MatchedBy,
}

/// Chooses one of `available` for a reader.
///
/// The inputs are tried in order, and the first that yields a tag wins:
///
/// 1. `requested`, by lookup.
/// 2. Each `accept` range in order, by lookup.
/// 3. `default`, by lookup.
/// 4. The first tag of `available` in canonical order.
///
/// Lookup follows RFC 4647 section 3.4: the tag is tried whole, then with its last subtag
/// removed, and so on down to the bare language. If none of those is available, the first
/// available tag with the same language wins, so a request for `de` finds `de-AT`. "First" is in
/// canonical order, so the answer does not depend on the order of `available`.
///
/// Returns `None` only when `available` is empty, and otherwise always a member of `available`.
/// One function serves the body and the title, so the two cannot disagree.
#[must_use]
pub fn negotiate(
    available: &[LocaleTag],
    requested: Option<&LocaleTag>,
    accept: &AcceptLanguage,
    default: Option<&LocaleTag>,
) -> Option<Negotiated> {
    let found = |locale: &LocaleTag, matched_by| {
        Some(Negotiated {
            locale: lookup(available, locale)?.clone(),
            matched_by,
        })
    };

    if let Some(negotiated) = requested.and_then(|tag| found(tag, MatchedBy::Request)) {
        return Some(negotiated);
    }
    if let Some(negotiated) = accept
        .ranges()
        .iter()
        .find_map(|tag| found(tag, MatchedBy::Header))
    {
        return Some(negotiated);
    }
    if let Some(negotiated) = default.and_then(|tag| found(tag, MatchedBy::Default)) {
        return Some(negotiated);
    }
    available.iter().min().map(|first| Negotiated {
        locale: first.clone(),
        matched_by: MatchedBy::FirstAvailable,
    })
}

/// Finds the available tag that best answers `wanted`, or `None`.
fn lookup<'a>(available: &'a [LocaleTag], wanted: &LocaleTag) -> Option<&'a LocaleTag> {
    let mut candidate = Some(wanted.clone());
    while let Some(tag) = candidate {
        if let Some(hit) = available.iter().find(|have| **have == tag) {
            return Some(hit);
        }
        candidate = tag.truncated();
    }
    available
        .iter()
        .filter(|have| have.same_language(wanted))
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(text: &str) -> LocaleTag {
        text.parse().expect("a valid tag")
    }

    fn tags(texts: &[&str]) -> Vec<LocaleTag> {
        texts.iter().map(|text| tag(text)).collect()
    }

    fn pick(
        available: &[&str],
        requested: Option<&str>,
        header: &str,
        default: Option<&str>,
    ) -> Option<(String, MatchedBy)> {
        let requested = requested.map(tag);
        let default = default.map(tag);
        negotiate(
            &tags(available),
            requested.as_ref(),
            &AcceptLanguage::parse(header),
            default.as_ref(),
        )
        .map(|n| (n.locale.to_string(), n.matched_by))
    }

    #[test]
    fn the_request_beats_the_header_which_beats_the_default() {
        let available = ["de", "en", "fr"];
        assert_eq!(
            pick(&available, Some("fr"), "de", Some("en")),
            Some(("fr".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&available, None, "de", Some("en")),
            Some(("de".into(), MatchedBy::Header))
        );
        assert_eq!(
            pick(&available, None, "ja", Some("en")),
            Some(("en".into(), MatchedBy::Default))
        );
        assert_eq!(
            pick(&available, Some("ja"), "", None),
            Some(("de".into(), MatchedBy::FirstAvailable))
        );
    }

    /// The last fallback was the first configured locale in code order, so `de` sorted before
    /// `en` and a French reader was served German with no operator control over it.
    #[test]
    fn the_default_locale_beats_the_first_available_one() {
        assert_eq!(
            pick(&["de", "en"], None, "fr", Some("en")),
            Some(("en".into(), MatchedBy::Default))
        );
        assert_eq!(
            pick(&["de", "en"], None, "fr", None),
            Some(("de".into(), MatchedBy::FirstAvailable))
        );
    }

    #[test]
    fn a_default_that_is_not_available_is_skipped() {
        assert_eq!(
            pick(&["de", "en"], None, "", Some("fr")),
            Some(("de".into(), MatchedBy::FirstAvailable))
        );
    }

    #[test]
    fn lookup_truncates_subtag_by_subtag() {
        assert_eq!(
            pick(&["de", "en"], Some("de-Latn-AT"), "", None),
            Some(("de".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&["de-Latn", "de"], Some("de-Latn-AT"), "", None),
            Some(("de-Latn".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&["de", "de-AT"], Some("de-AT"), "", None),
            Some(("de-AT".into(), MatchedBy::Request))
        );
    }

    /// A regional tag matched only its own primary subtag, so `de-CH` never found `de-AT`.
    #[test]
    fn a_regional_tag_matches_its_primary_subtag() {
        assert_eq!(
            pick(&["de", "en"], Some("de-AT"), "", None),
            Some(("de".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&["de-AT", "en"], Some("de"), "", None),
            Some(("de-AT".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&["de-AT", "en"], Some("de-CH"), "", None),
            Some(("de-AT".into(), MatchedBy::Request))
        );
    }

    #[test]
    fn primary_language_fallback_takes_the_first_in_canonical_order() {
        assert_eq!(
            pick(&["de-CH", "de-AT"], Some("de"), "", None),
            Some(("de-AT".into(), MatchedBy::Request))
        );
        assert_eq!(
            pick(&["de-AT", "de-CH"], Some("de"), "", None),
            Some(("de-AT".into(), MatchedBy::Request))
        );
    }

    #[test]
    fn header_ranges_are_tried_in_preference_order() {
        assert_eq!(
            pick(&["de", "en"], None, "fr, en;q=0.8, de;q=0.5", None),
            Some(("en".into(), MatchedBy::Header))
        );
    }

    #[test]
    fn nothing_available_negotiates_nothing() {
        assert_eq!(pick(&[], Some("de"), "de", Some("de")), None);
    }

    #[test]
    fn the_first_available_locale_does_not_depend_on_input_order() {
        assert_eq!(
            pick(&["fr", "de", "en"], None, "", None),
            pick(&["en", "fr", "de"], None, "", None)
        );
    }
}
