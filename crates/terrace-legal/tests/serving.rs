//! What a reader is served, and how a swap of the configuration shows.

mod common;

use std::sync::Arc;

use common::{catalog, config, external, hosted, titled};
use terrace_legal::model::{AcceptLanguage, LegalKind, LocaleTag};
use terrace_legal::{Legal, LegalError, Negotiation};

fn tag(text: &str) -> LocaleTag {
    text.parse().expect("a valid tag")
}

fn ask(lang: Option<&str>, header: Option<&str>) -> Negotiation {
    Negotiation::from_request(lang, header.map(str::as_bytes))
}

fn serve(config: &terrace_legal::LegalConfig) -> Legal {
    Legal::new(catalog(config))
}

fn terms() -> terrace_legal::LegalConfig {
    config(
        Some("en"),
        vec![(
            "terms",
            titled(
                hosted(&[
                    ("en", "# Terms"),
                    ("de", "# Bedingungen"),
                    ("de-AT", "# Servus"),
                ]),
                &[
                    ("en", "Terms"),
                    ("de", "Bedingungen"),
                    ("de-AT", "Nutzungsbedingungen AT"),
                ],
            ),
        )],
    )
}

#[test]
fn the_request_beats_the_header_which_beats_the_default() {
    let legal = serve(&terms());
    let body = |negotiation: Negotiation| {
        legal
            .document("terms", &negotiation)
            .expect("served")
            .value
            .body
            .clone()
    };
    assert_eq!(body(ask(Some("de"), Some("en"))), "# Bedingungen");
    assert_eq!(body(ask(None, Some("de"))), "# Bedingungen");
    assert_eq!(body(ask(None, Some("ja"))), "# Terms");
    assert_eq!(body(ask(None, None)), "# Terms");
}

/// The last fallback was the first configured locale in code order, so `de` sorted before `en` and
/// a French reader was served German with no operator control over it.
#[test]
fn a_reader_of_an_unpublished_language_is_served_the_default_not_the_first_locale() {
    let legal = serve(&terms());
    let served = legal
        .document("terms", &ask(None, Some("fr")))
        .expect("served");
    assert_eq!(served.value.locale, tag("en"));

    let no_default = config(
        None,
        vec![(
            "terms",
            hosted(&[("en", "# Terms"), ("de", "# Bedingungen")]),
        )],
    );
    let served = serve(&no_default)
        .document("terms", &ask(None, Some("fr")))
        .expect("served");
    assert_eq!(
        served.value.locale,
        tag("de"),
        "without a default the first in canonical order"
    );
}

/// A config key `EN` never matched a request, because the keys were case-sensitive.
#[test]
fn a_locale_key_written_in_capitals_is_served() {
    let legal = serve(&config(
        None,
        vec![("terms", hosted(&[("EN", "# Terms"), ("DE_AT", "# Servus")]))],
    ));
    let served = legal
        .document("terms", &ask(Some("en"), None))
        .expect("served");
    assert_eq!(served.value.locale, tag("en"));
    let served = legal
        .document("terms", &ask(Some("de-at"), None))
        .expect("served");
    assert_eq!(served.value.body, "# Servus");
}

#[test]
fn a_regional_request_finds_its_language_and_a_language_finds_a_region() {
    let legal = serve(&config(
        None,
        vec![("terms", hosted(&[("en", "# Terms"), ("de-AT", "# Servus")]))],
    ));
    let de = legal
        .document("terms", &ask(Some("de"), None))
        .expect("served");
    assert_eq!(de.value.locale, tag("de-AT"));
    let en = legal
        .document("terms", &ask(Some("en-GB"), None))
        .expect("served");
    assert_eq!(en.value.locale, tag("en"));
}

/// The title was chosen by its own chain, requested locale first, so a French title could be
/// shown over a German body.
#[test]
fn the_title_always_matches_the_locale_of_the_body() {
    let config = config(
        Some("de"),
        vec![(
            "terms",
            titled(
                hosted(&[("de", "# Bedingungen"), ("en", "# Terms")]),
                &[("fr", "Conditions"), ("de", "Bedingungen")],
            ),
        )],
    );
    let legal = serve(&config);
    // French is requested and titled, but the body is not published in French.
    let negotiation = ask(Some("fr"), None);
    let served = legal.document("terms", &negotiation).expect("served");
    assert_eq!(served.value.locale, tag("de"));
    assert_eq!(served.value.title.as_deref(), Some("Bedingungen"));
    let index = legal.index(&negotiation);
    assert_eq!(index.value[0].title.as_deref(), Some("Bedingungen"));
}

#[test]
fn a_document_without_a_title_in_the_served_locale_has_no_title() {
    let legal = serve(&config(
        Some("en"),
        vec![(
            "terms",
            titled(hosted(&[("en", "x"), ("de", "y")]), &[("en", "Terms")]),
        )],
    ));
    let served = legal
        .document("terms", &ask(Some("de"), None))
        .expect("served");
    assert_eq!(served.value.title, None);
    assert_eq!(legal.index(&ask(Some("de"), None)).value[0].title, None);
}

#[test]
fn an_external_document_takes_its_title_from_its_own_locales() {
    let legal = serve(&config(
        Some("en"),
        vec![(
            "imprint",
            titled(
                external("https://example.org/impressum"),
                &[("en", "Imprint"), ("de", "Impressum")],
            ),
        )],
    ));
    let entry = &legal.index(&ask(None, Some("de"))).value[0];
    assert_eq!(entry.kind, LegalKind::External);
    assert_eq!(entry.title.as_deref(), Some("Impressum"));
    assert_eq!(entry.url.as_deref(), Some("https://example.org/impressum"));
    let entry = &legal.index(&ask(None, Some("fr"))).value[0];
    assert_eq!(entry.title.as_deref(), Some("Imprint"));
}

#[test]
fn requesting_the_body_of_an_external_document_says_where_it_is() {
    let legal = serve(&config(
        None,
        vec![("imprint", external("https://example.org/impressum"))],
    ));
    match legal.document("imprint", &Negotiation::default()) {
        Err(LegalError::External { url }) => {
            assert_eq!(url.as_str(), "https://example.org/impressum")
        }
        other => panic!("expected an external error, got {other:?}"),
    }
}

/// The slug is a map key and never a path, so no request can reach the filesystem through it.
#[test]
fn an_unknown_or_hostile_slug_is_not_found() {
    let legal = serve(&terms());
    for slug in [
        "privacy",
        "",
        "..",
        "../../etc/passwd",
        "..%2F..%2Fetc%2Fpasswd",
        "terms/../terms",
        "TERMS",
        "terms\0",
        "terms ",
    ] {
        assert_eq!(
            legal.document(slug, &Negotiation::default()).unwrap_err(),
            LegalError::UnknownSlug,
            "slug {slug:?}"
        );
    }
}

#[test]
fn a_document_served_from_an_empty_catalog_is_not_found() {
    let legal = serve(&config(None, vec![]));
    assert_eq!(
        legal
            .document("terms", &Negotiation::default())
            .unwrap_err(),
        LegalError::UnknownSlug
    );
    assert!(legal.index(&Negotiation::default()).value.is_empty());
}

/// The tag covered the body only, so once conditional requests were honoured a reload that changed
/// only a title would have been answered with a `304` and shown the old title.
#[test]
fn a_title_only_change_gives_a_new_document_tag() {
    let with_title = |title: &str| {
        config(
            None,
            vec![(
                "terms",
                titled(hosted(&[("en", "# Terms")]), &[("en", title)]),
            )],
        )
    };
    let etag = |config: &terrace_legal::LegalConfig| {
        serve(config)
            .document("terms", &Negotiation::default())
            .expect("served")
            .etag
    };
    assert_ne!(
        etag(&with_title("Terms")),
        etag(&with_title("Terms of Service"))
    );
    assert_eq!(etag(&with_title("Terms")), etag(&with_title("Terms")));
}

#[test]
fn an_updated_line_change_and_a_locale_change_give_new_tags() {
    let base = config(None, vec![("terms", hosted(&[("en", "x"), ("de", "x")]))]);
    let mut changed = base.clone();
    changed.documents.get_mut("terms").expect("present").updated = Some("2026-09-21".into());

    let served = |config: &terrace_legal::LegalConfig, lang| {
        serve(config)
            .document("terms", &ask(Some(lang), None))
            .expect("served")
            .etag
    };
    assert_ne!(served(&base, "en"), served(&changed, "en"));
    assert_ne!(
        served(&base, "en"),
        served(&base, "de"),
        "the same text in two locales"
    );
}

/// The tag came from a hasher whose algorithm is unspecified across Rust releases, so replicas
/// built by different compilers disagreed during a rollout.
#[test]
fn identical_inputs_give_identical_tags_across_instances() {
    let (a, b) = (serve(&terms()), serve(&terms()));
    let negotiation = ask(Some("de"), None);
    assert_eq!(
        a.document("terms", &negotiation).expect("served").etag,
        b.document("terms", &negotiation).expect("served").etag
    );
    assert_eq!(a.index(&negotiation).etag, b.index(&negotiation).etag);
}

#[test]
fn the_index_tag_follows_titles_order_and_content() {
    let base = config(
        None,
        vec![
            ("privacy", hosted(&[("en", "y")])),
            ("terms", titled(hosted(&[("en", "x")]), &[("en", "Terms")])),
        ],
    );
    let tag_of =
        |config: &terrace_legal::LegalConfig| serve(config).index(&Negotiation::default()).etag;
    assert_eq!(tag_of(&base), tag_of(&base.clone()));

    let mut retitled = base.clone();
    retitled
        .documents
        .get_mut("terms")
        .expect("present")
        .title
        .insert("en".into(), "ToS".into());
    assert_ne!(tag_of(&base), tag_of(&retitled));

    let mut reordered = base.clone();
    reordered.documents.get_mut("terms").expect("present").order = -1;
    assert_ne!(
        tag_of(&base),
        tag_of(&reordered),
        "the same entries in another order"
    );

    let mut extended = base.clone();
    extended
        .documents
        .insert("imprint".into(), hosted(&[("en", "z")]));
    assert_ne!(tag_of(&base), tag_of(&extended));

    let mut dated = base.clone();
    dated.documents.get_mut("terms").expect("present").updated = Some("2026-09-21".into());
    assert_ne!(tag_of(&base), tag_of(&dated));
}

#[test]
fn a_reader_who_prefers_another_language_gets_another_index_tag() {
    let legal = serve(&terms());
    assert_ne!(
        legal.index(&ask(Some("de"), None)).etag,
        legal.index(&ask(Some("en"), None)).etag
    );
}

#[test]
fn a_repeated_read_shares_the_same_view() {
    let legal = serve(&terms());
    let first = legal
        .document("terms", &Negotiation::default())
        .expect("served");
    let second = legal
        .document("terms", &Negotiation::default())
        .expect("served");
    assert!(
        Arc::ptr_eq(&first.value, &second.value),
        "a body is not copied per request"
    );
}

#[test]
fn an_invalid_replacement_leaves_the_previous_catalog_serving() {
    let legal = serve(&terms());
    let before = legal.index(&Negotiation::default());

    let mut broken = terms();
    broken
        .documents
        .get_mut("terms")
        .expect("present")
        .body
        .insert("en".into(), String::new());
    let issues = legal.replace(&broken).expect_err("refused");
    assert_eq!(issues.len(), 1);

    assert_eq!(legal.index(&Negotiation::default()), before);
    assert_eq!(legal.catalog().generation(), 0);
    assert_eq!(
        legal
            .document("terms", &ask(Some("en"), None))
            .expect("still served")
            .value
            .body,
        "# Terms"
    );
}

#[test]
fn a_valid_replacement_applies_and_moves_the_index_tag() {
    let legal = serve(&terms());
    let before = legal.index(&Negotiation::default());

    let mut edited = terms();
    edited
        .documents
        .get_mut("terms")
        .expect("present")
        .body
        .insert("en".into(), "# New".into());
    legal.replace(&edited).expect("valid");

    assert_eq!(legal.catalog().generation(), 1);
    let after = legal.index(&Negotiation::default());
    assert_ne!(after.etag, before.etag);
    assert_eq!(
        legal
            .document("terms", &ask(Some("en"), None))
            .expect("served")
            .value
            .body,
        "# New"
    );

    // Replacing with the same content still moves the generation, so a cache is never stale.
    legal.replace(&edited).expect("valid");
    assert_eq!(legal.catalog().generation(), 2);
    assert_ne!(legal.index(&Negotiation::default()).etag, after.etag);
}

#[test]
fn a_clone_shares_the_catalog() {
    let legal = serve(&terms());
    let clone = legal.clone();
    let mut edited = terms();
    edited
        .documents
        .get_mut("terms")
        .expect("present")
        .body
        .insert("en".into(), "# Shared".into());
    legal.replace(&edited).expect("valid");
    assert_eq!(
        clone
            .document("terms", &ask(Some("en"), None))
            .expect("served")
            .value
            .body,
        "# Shared"
    );
}

#[test]
fn a_replacement_uses_the_limits_the_handle_was_built_with() {
    let limits = terrace_legal::Limits { max_body_bytes: 4 };
    let small = config(None, vec![("terms", hosted(&[("en", "1234")]))]);
    let legal = Legal::with_limits(
        terrace_legal::Catalog::build_with(&small, &limits).expect("valid"),
        limits,
    );
    let big = config(None, vec![("terms", hosted(&[("en", "12345")]))]);
    assert!(legal.replace(&big).is_err());
}

#[test]
fn readers_never_see_a_half_swapped_catalog() {
    let legal = serve(&config(None, vec![("terms", hosted(&[("en", "0")]))]));
    let readers: Vec<_> = (0..4)
        .map(|_| {
            let legal = legal.clone();
            std::thread::spawn(move || {
                for _ in 0..500 {
                    let served = legal
                        .document("terms", &Negotiation::default())
                        .expect("always served");
                    assert!(served.value.body.parse::<u32>().is_ok());
                }
            })
        })
        .collect();
    for version in 1..200u32 {
        let next = config(
            None,
            vec![("terms", hosted(&[("en", &version.to_string())]))],
        );
        legal.replace(&next).expect("valid");
    }
    for reader in readers {
        reader.join().expect("no reader panicked");
    }
}

#[test]
fn negotiation_from_a_request_is_lenient() {
    let n = Negotiation::from_request(Some("not a locale"), Some(b"\xff\xfe, de"));
    assert_eq!(n.requested, None);
    assert_eq!(n.accept, AcceptLanguage::parse("de"));

    let n = Negotiation::from_request(Some(" DE_at "), None);
    assert_eq!(n.requested, Some(tag("de-AT")));
    assert!(n.accept.is_empty());
}

/// The index lists the locales an inline document is published in, so a client can offer them, and
/// none for an external document.
#[test]
fn the_index_lists_the_published_locales_of_an_inline_document_only() {
    let legal = serve(&config(
        None,
        vec![
            ("terms", hosted(&[("en", "x"), ("de_at", "y"), ("DE", "z")])),
            (
                "imprint",
                titled(external("https://example.org/"), &[("en", "Imprint")]),
            ),
        ],
    ));
    let index = legal.index(&Negotiation::default()).value;
    let by = |slug: &str| {
        index
            .iter()
            .find(|entry| entry.slug == slug)
            .expect("listed")
    };
    let locales: Vec<&str> = by("terms").locales.iter().map(|l| l.as_str()).collect();
    assert_eq!(locales, ["de", "de-AT", "en"]);
    assert!(by("imprint").locales.is_empty());
    assert_eq!(by("terms").kind, LegalKind::Inline);
    assert_eq!(by("imprint").kind, LegalKind::External);
}

#[test]
fn the_served_document_is_markdown_with_the_published_members() {
    let legal = serve(&terms());
    let served = legal
        .document("terms", &Negotiation::default())
        .expect("served");
    assert!(served.value.format.is_markdown());
}
