//! Parsing of the `Accept-Language` request header.

use crate::locale::LocaleTag;

/// The language ranges of an `Accept-Language` header, most preferred first.
///
/// The default value expresses no preference, as a missing header does.
///
/// Parsing is bounded and never fails. At most [`Self::MAX_HEADER_BYTES`] bytes are read and at
/// most [`Self::MAX_RANGES`] ranges are considered. A range that is not a [`LocaleTag`], a range
/// with a malformed `q`, a `*` wildcard and a range with `q=0` (which RFC 9110 defines as "not
/// acceptable") are skipped. Equal weights keep the order the client sent them in, and a tag that
/// appears twice keeps its most preferred position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcceptLanguage {
    ranges: Vec<LocaleTag>,
}

/// A weight in thousandths, as RFC 9110 allows at most three decimals.
type Millis = u16;

impl AcceptLanguage {
    /// The most header bytes that are read; anything beyond is ignored.
    pub const MAX_HEADER_BYTES: usize = 4096;

    /// The most comma-separated ranges that are considered.
    pub const MAX_RANGES: usize = 32;

    /// Parses header bytes, which are not required to be UTF-8.
    #[must_use]
    pub fn parse_bytes(header: &[u8]) -> Self {
        let slice = &header[..header.len().min(Self::MAX_HEADER_BYTES)];
        let text = String::from_utf8_lossy(slice);
        if header.len() > Self::MAX_HEADER_BYTES {
            Self::parse_cut(&text)
        } else {
            Self::parse(&text)
        }
    }

    /// Parses a header that is already text.
    #[must_use]
    pub fn parse(header: &str) -> Self {
        if header.len() <= Self::MAX_HEADER_BYTES {
            return Self::parse_ranges(header);
        }
        let end = header.floor_char_boundary(Self::MAX_HEADER_BYTES);
        Self::parse_cut(&header[..end])
    }

    /// Parses text that was cut at the byte bound, so its last range may be incomplete.
    fn parse_cut(text: &str) -> Self {
        // `de` cut from `de-AT` would be a valid but wrong range, so everything after the last
        // complete comma is dropped.
        Self::parse_ranges(text.rfind(',').map_or("", |comma| &text[..comma]))
    }

    fn parse_ranges(header: &str) -> Self {
        let mut weighted: Vec<(Millis, LocaleTag)> = header
            .split(',')
            .take(Self::MAX_RANGES)
            .filter_map(parse_range)
            .filter(|(weight, _)| *weight > 0)
            .collect();
        // Stable, so equal weights keep the client's order.
        weighted.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));

        let mut ranges: Vec<LocaleTag> = Vec::with_capacity(weighted.len());
        for (_, tag) in weighted {
            if !ranges.contains(&tag) {
                ranges.push(tag);
            }
        }
        Self { ranges }
    }

    /// Returns the ranges, most preferred first.
    #[must_use]
    pub fn ranges(&self) -> &[LocaleTag] {
        &self.ranges
    }

    /// Returns whether the header expressed no usable preference.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

fn parse_range(segment: &str) -> Option<(Millis, LocaleTag)> {
    let mut pieces = segment.split(';');
    // An empty range and the `*` wildcard are not locales, so parsing skips both.
    let tag: LocaleTag = pieces.next()?.trim().parse().ok()?;

    let mut weight: Millis = 1000;
    for parameter in pieces {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("q") {
            // A malformed weight skips the whole range.
            weight = parse_qvalue(value.trim())?;
            break;
        }
    }
    Some((weight, tag))
}

/// Parses `0`, `0.5`, `0.123`, `1`, `1.0` and `1.000`, and nothing else.
fn parse_qvalue(value: &str) -> Option<Millis> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut thousandths: Millis = 0;
    let mut place: Millis = 100;
    for digit in fraction.bytes() {
        thousandths += Millis::from(digit - b'0') * place;
        place /= 10;
    }
    match whole {
        "0" => Some(thousandths),
        "1" if thousandths == 0 => Some(1000),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(header: &str) -> Vec<String> {
        AcceptLanguage::parse(header)
            .ranges()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    /// The header was ordered by position instead of by weight, so `en;q=0.5, de` served English.
    #[test]
    fn accept_language_is_ordered_by_quality_not_by_position() {
        assert_eq!(ranges("en;q=0.5, de, fr;q=0.8"), ["de", "fr", "en"]);
        assert_eq!(ranges("en;q=0.5,de;q=0.9"), ["de", "en"]);
    }

    /// A request with no header, or only `*`, expresses no preference and must not select
    /// anything.
    #[test]
    fn a_missing_or_wildcard_header_expresses_no_preference() {
        for header in ["", "  ", "*", "*;q=0.5", ",,,", "*, *"] {
            assert!(
                AcceptLanguage::parse(header).is_empty(),
                "header {header:?}"
            );
        }
        assert!(AcceptLanguage::default().is_empty());
    }

    /// `q=0` means "not acceptable" in RFC 9110, but it was kept as the last fallback, so a
    /// reader who refused German was served German.
    #[test]
    fn a_range_with_zero_quality_is_dropped() {
        assert_eq!(ranges("de;q=0, en"), ["en"]);
        assert_eq!(ranges("de;q=0.000, en;q=0.001"), ["en"]);
        assert!(ranges("de;q=0").is_empty());
    }

    #[test]
    fn a_malformed_quality_skips_only_its_range() {
        for bad in [
            "q=1.5", "q=2", "q=-1", "q=0.1234", "q=", "q=abc", "q=.5", "q=0.5x", "q=01",
        ] {
            assert_eq!(ranges(&format!("de;{bad}, en")), ["en"], "parameter {bad}");
        }
    }

    #[test]
    fn valid_quality_spellings_are_accepted() {
        for good in [
            "q=0.5", "q=0.123", "q=1", "q=1.", "q=1.0", "q=1.000", "Q=0.5", "q = 0.5",
        ] {
            assert_eq!(ranges(&format!("de;{good}")), ["de"], "parameter {good}");
        }
        assert_eq!(parse_qvalue("0"), Some(0));
        assert_eq!(parse_qvalue("0."), Some(0));
        assert_eq!(parse_qvalue("0.5"), Some(500));
        assert_eq!(parse_qvalue("0.05"), Some(50));
        assert_eq!(parse_qvalue("0.005"), Some(5));
        assert_eq!(parse_qvalue("1.000"), Some(1000));
        assert_eq!(parse_qvalue("1.001"), None);
    }

    #[test]
    fn equal_weights_keep_the_clients_order() {
        assert_eq!(ranges("fr, de, en"), ["fr", "de", "en"]);
        assert_eq!(ranges("fr;q=0.5, de;q=0.5, en;q=0.5"), ["fr", "de", "en"]);
    }

    #[test]
    fn a_repeated_tag_keeps_its_best_position() {
        assert_eq!(ranges("en;q=0.1, de, EN"), ["de", "en"]);
        assert_eq!(ranges("en, de, EN;q=0.1"), ["en", "de"]);
        assert_eq!(ranges("en;q=0.1, de, en;q=0.9"), ["de", "en"]);
    }

    #[test]
    fn unparseable_ranges_and_unknown_parameters_are_tolerated() {
        assert_eq!(
            ranges("x-klingon, de-CH-1996, en;foo=bar, fr;level=1"),
            ["en", "fr"]
        );
    }

    #[test]
    fn the_range_bound_holds() {
        let header = format!("{}en", "de,".repeat(AcceptLanguage::MAX_RANGES));
        assert!(
            AcceptLanguage::parse(&header)
                .ranges()
                .iter()
                .all(|tag| tag.as_str() != "en"),
            "the 33rd range is not read"
        );
        let header = format!("{}en", "de,".repeat(AcceptLanguage::MAX_RANGES - 1));
        assert!(
            AcceptLanguage::parse(&header)
                .ranges()
                .iter()
                .any(|tag| tag.as_str() == "en"),
            "the 32nd range is read"
        );
    }

    #[test]
    fn the_byte_bound_holds_and_a_cut_range_is_not_read() {
        // The boundary falls in the middle of `de-AT`: `de` must not be read from it.
        let mut header = "fr,".repeat((AcceptLanguage::MAX_HEADER_BYTES - 3) / 3);
        while header.len() < AcceptLanguage::MAX_HEADER_BYTES - 2 {
            header.push('x');
        }
        header.push_str(",de-AT");
        assert!(header.len() > AcceptLanguage::MAX_HEADER_BYTES);
        for parsed in [
            AcceptLanguage::parse(&header),
            AcceptLanguage::parse_bytes(header.as_bytes()),
        ] {
            assert!(parsed.ranges().iter().all(|tag| tag.language() != "de"));
            assert!(!parsed.is_empty());
        }
    }

    /// `fr` followed by a parameter long enough that a range starting at the next comma ends
    /// exactly `tail_len` bytes into the header.
    fn header_with_last_range_at(total: usize, last: &str) -> String {
        let prefix = "fr;x=";
        let filler = total - prefix.len() - 1 - last.len();
        format!("{prefix}{}{}{last}", "a".repeat(filler), ",")
    }

    #[test]
    fn a_header_of_exactly_the_bound_keeps_its_last_range() {
        let header = header_with_last_range_at(AcceptLanguage::MAX_HEADER_BYTES, "de");
        assert_eq!(header.len(), AcceptLanguage::MAX_HEADER_BYTES);
        for parsed in [
            AcceptLanguage::parse(&header),
            AcceptLanguage::parse_bytes(header.as_bytes()),
        ] {
            assert_eq!(parsed.ranges().len(), 2, "both ranges are read");
        }
    }

    /// The boundary falls right after `de`, which would be a valid but wrong range if the cut
    /// range were read.
    #[test]
    fn a_range_cut_by_the_bound_is_dropped_by_both_entry_points() {
        let header = format!(
            "{}-AT",
            header_with_last_range_at(AcceptLanguage::MAX_HEADER_BYTES, "de")
        );
        assert!(header.len() > AcceptLanguage::MAX_HEADER_BYTES);
        for parsed in [
            AcceptLanguage::parse(&header),
            AcceptLanguage::parse_bytes(header.as_bytes()),
        ] {
            assert_eq!(parsed.ranges().len(), 1, "only `fr` survives: {parsed:?}");
        }
    }

    #[test]
    fn a_multibyte_character_across_the_bound_does_not_panic() {
        let mut header = "en,fr;x=".to_owned();
        header.push_str(&"a".repeat(AcceptLanguage::MAX_HEADER_BYTES - header.len() - 1));
        header.push('é');
        header.push_str(",de");
        assert!(!header.is_char_boundary(AcceptLanguage::MAX_HEADER_BYTES));
        let parsed = AcceptLanguage::parse(&header);
        assert_eq!(
            parsed
                .ranges()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["en"],
            "the range cut by the bound is dropped"
        );
    }

    #[test]
    fn non_utf8_bytes_do_not_panic() {
        let parsed = AcceptLanguage::parse_bytes(b"de, \xff\xfe, en;q=0.5");
        assert_eq!(
            parsed
                .ranges()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            ["de", "en"]
        );
    }
}
