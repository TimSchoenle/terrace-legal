//! Oracle for `Accept-Language` parsing and the negotiation that consumes it.

use terrace_legal_model::{AcceptLanguage, LocaleTag, MatchedBy, negotiate};

/// Parses arbitrary bytes as a header and checks what a header parse promises.
///
/// # Panics
///
/// When parsing panics, when the output exceeds its bounds or repeats a tag, when a returned range
/// is not a canonical locale, when the text and byte entry points disagree, or when negotiating over
/// the parsed ranges does not pick the client's first choice.
pub fn check(data: &[u8]) {
    let parsed = AcceptLanguage::parse_bytes(data);
    let ranges = parsed.ranges();

    assert!(
        ranges.len() <= AcceptLanguage::MAX_RANGES,
        "too many ranges"
    );
    for (index, tag) in ranges.iter().enumerate() {
        assert!(!ranges[..index].contains(tag), "a tag appears twice: {tag}");
        assert_eq!(
            tag.as_str().parse::<LocaleTag>().as_ref(),
            Ok(tag),
            "a range is not canonical"
        );
    }

    if data.len() <= AcceptLanguage::MAX_HEADER_BYTES
        && let Ok(text) = std::str::from_utf8(data)
    {
        assert_eq!(
            AcceptLanguage::parse(text),
            parsed,
            "the text and byte entry points disagree"
        );
    }

    if let Some(first) = ranges.first() {
        // The client's first choice is available, so it has to win, and by the header.
        let negotiated = negotiate(ranges, None, &parsed, None).expect("something is available");
        assert_eq!(&negotiated.locale, first);
        assert_eq!(negotiated.matched_by, MatchedBy::Header);
    } else {
        assert!(negotiate(&[], None, &parsed, None).is_none());
    }
}
