//! Property tests for the pure functions that take untrusted input.

use proptest::prelude::*;
use terrace_legal_model::{AcceptLanguage, LocaleTag, negotiate};

fn arb_tag() -> impl Strategy<Value = LocaleTag> {
    (
        "[a-z]{2,3}",
        proptest::option::of("[A-Z][a-z]{3}"),
        proptest::option::of(prop_oneof!["[A-Z]{2}", "[0-9]{3}"]),
    )
        .prop_map(|(language, script, region)| {
            let mut text = language;
            for part in [script, region].into_iter().flatten() {
                text.push('-');
                text.push_str(&part);
            }
            text.parse().expect("generated tags are valid")
        })
}

proptest! {
    #[test]
    fn accept_language_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..6000)) {
        let _ = AcceptLanguage::parse_bytes(&bytes);
    }

    #[test]
    fn accept_language_never_panics_on_arbitrary_text(text in ".{0,6000}") {
        let _ = AcceptLanguage::parse(&text);
    }

    #[test]
    fn accept_language_output_is_duplicate_free_and_bounded(
        ranges in proptest::collection::vec((arb_tag(), 0u16..=1000), 0..60),
    ) {
        let header = ranges
            .iter()
            .map(|(tag, q)| format!("{tag};q=0.{:03}", q % 1000))
            .collect::<Vec<_>>()
            .join(", ");
        let parsed = AcceptLanguage::parse(&header);
        let output = parsed.ranges();
        prop_assert!(output.len() <= AcceptLanguage::MAX_RANGES);
        for (index, tag) in output.iter().enumerate() {
            prop_assert!(!output[..index].contains(tag));
        }
    }

    #[test]
    fn a_locale_tag_survives_its_own_canonical_text(tag in arb_tag()) {
        let again: LocaleTag = tag.as_str().parse().expect("canonical text parses");
        prop_assert_eq!(&again, &tag);
        prop_assert_eq!(again.to_string(), tag.to_string());
    }

    #[test]
    fn a_locale_tag_parser_never_panics(text in ".{0,40}") {
        let _ = text.parse::<LocaleTag>();
    }

    #[test]
    fn negotiation_answers_iff_something_is_available_and_answers_from_it(
        available in proptest::collection::vec(arb_tag(), 0..6),
        requested in proptest::option::of(arb_tag()),
        header in proptest::collection::vec(arb_tag(), 0..4),
        default in proptest::option::of(arb_tag()),
    ) {
        let accept = AcceptLanguage::parse(
            &header.iter().map(ToString::to_string).collect::<Vec<_>>().join(","),
        );
        let outcome = negotiate(&available, requested.as_ref(), &accept, default.as_ref());
        prop_assert_eq!(outcome.is_some(), !available.is_empty());
        if let Some(negotiated) = outcome {
            prop_assert!(available.contains(&negotiated.locale));
        }
    }

    #[test]
    fn negotiation_does_not_depend_on_the_order_of_the_available_tags(
        available in proptest::collection::vec(arb_tag(), 1..6),
        requested in proptest::option::of(arb_tag()),
    ) {
        let accept = AcceptLanguage::default();
        let mut reversed = available.clone();
        reversed.reverse();
        prop_assert_eq!(
            negotiate(&available, requested.as_ref(), &accept, None),
            negotiate(&reversed, requested.as_ref(), &accept, None)
        );
    }
}
