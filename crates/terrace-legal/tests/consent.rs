//! Registration checks, evaluation through the catalog, and the in-memory store.
#![cfg(feature = "consent")]

mod common;

use common::{catalog, config, external, hosted};
use terrace_legal::consent::{
    AcceptanceError, ConsentStore, MemoryConsentStore, RegistrationError,
};
use terrace_legal::model::consent::{
    Acceptance, Channel, ConsentRecord, RecordKind, Requirement, Withdrawal,
};
use terrace_legal::model::{Digest, LocaleTag};
use terrace_legal::{ConsentPolicy, Legal, Negotiation};
use time::OffsetDateTime;
use time::macros::datetime;

const NOW: OffsetDateTime = datetime!(2026-09-21 12:00 UTC);

fn tag(text: &str) -> LocaleTag {
    text.parse().expect("valid")
}

fn legal() -> Legal {
    let mut terms = hosted(&[("en", "# Terms"), ("de", "# Bedingungen")]);
    terms.consent = ConsentPolicy {
        requirement: Requirement::Accept,
        version: Some("2026-08".into()),
        effective: Some("2026-09-01".into()),
        grace_days: 14,
    };
    let mut privacy = hosted(&[("en", "# Privacy")]);
    privacy.consent = ConsentPolicy {
        requirement: Requirement::Acknowledge,
        version: Some("3".into()),
        effective: None,
        grace_days: 0,
    };
    Legal::new(catalog(&config(
        Some("en"),
        vec![
            ("terms", terms),
            ("privacy", privacy),
            ("imprint", external("https://example.org/impressum")),
            ("cookies", hosted(&[("en", "# Cookies")])),
        ],
    )))
}

/// The acceptance a reader sends after being shown `slug` in `locale`.
fn shown(legal: &Legal, slug: &str, locale: &str) -> Acceptance {
    let served = legal
        .document(slug, &Negotiation::from_request(Some(locale), None))
        .expect("served");
    let consent = served.value.consent.clone().expect("has a policy");
    Acceptance {
        slug: slug.to_owned(),
        version: consent.version,
        locale: served.value.locale.clone(),
        digest: served.value.digest.clone(),
    }
}

#[test]
fn the_wire_types_carry_the_policy_and_the_digest_of_what_is_shown() {
    let legal = legal();
    let negotiation = Negotiation::from_request(Some("de"), None);
    let served = legal.document("terms", &negotiation).expect("served");
    assert_eq!(
        served.value.consent.as_ref().expect("policy").version,
        "2026-08"
    );
    assert_eq!(
        served.value.consent.as_ref().expect("policy").requirement,
        Requirement::Accept
    );
    assert!(served.value.digest.as_str().starts_with("sha256:"));

    let index = legal.index(&negotiation);
    let by_slug = |slug: &str| index.value.iter().find(|e| e.slug == slug).expect("listed");
    assert_eq!(
        by_slug("terms").consent.as_ref().expect("policy").version,
        "2026-08"
    );
    assert_eq!(
        by_slug("privacy")
            .consent
            .as_ref()
            .expect("policy")
            .requirement,
        Requirement::Acknowledge
    );
    assert!(by_slug("cookies").consent.is_none());
    assert!(by_slug("imprint").consent.is_none());
}

#[test]
fn the_digest_differs_per_locale_and_follows_the_text() {
    let legal = legal();
    let en = shown(&legal, "terms", "en");
    let de = shown(&legal, "terms", "de");
    assert_ne!(en.digest, de.digest);

    let mut edited = terrace_legal::LegalConfig::default();
    edited.documents.insert("terms".into(), {
        let mut terms = hosted(&[("en", "# Terms, edited")]);
        terms.consent = ConsentPolicy {
            requirement: Requirement::Accept,
            version: Some("2026-08".into()),
            ..ConsentPolicy::default()
        };
        terms
    });
    let other = Legal::new(catalog(&edited));
    assert_ne!(shown(&other, "terms", "en").digest, en.digest);
}

#[test]
fn a_registration_that_accepts_every_required_document_yields_records() {
    let legal = legal();
    let records = legal
        .verify_registration(&7u32, &[shown(&legal, "terms", "de")], NOW)
        .expect("valid");
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.subject, 7);
    assert_eq!(record.slug.as_str(), "terms");
    assert_eq!(record.locale, tag("de"));
    assert_eq!(record.kind, RecordKind::Accepted);
    assert_eq!(record.channel, Channel::Registration);
    assert_eq!(record.at, NOW);
    assert_eq!(record.version, "2026-08");
}

#[test]
fn an_acknowledge_only_document_is_recorded_when_present_and_not_required() {
    let legal = legal();
    let with = legal
        .verify_registration(
            &7u32,
            &[shown(&legal, "terms", "en"), shown(&legal, "privacy", "en")],
            NOW,
        )
        .expect("valid");
    assert_eq!(with.len(), 2);
    let privacy = with
        .iter()
        .find(|r| r.slug.as_str() == "privacy")
        .expect("recorded");
    assert_eq!(privacy.kind, RecordKind::Acknowledged);

    let without = legal
        .verify_registration(&7u32, &[shown(&legal, "terms", "en")], NOW)
        .expect("privacy is not required");
    assert_eq!(without.len(), 1);
}

#[test]
fn a_registration_missing_an_accept_document_is_refused_regardless_of_grace() {
    let legal = legal();
    // The grace period has not started to run out: it applies to existing subjects only.
    assert_eq!(
        legal.verify_registration(&7u32, &[], NOW),
        Err(RegistrationError::Missing {
            slug: "terms".parse().expect("valid")
        })
    );
    assert!(matches!(
        legal.verify_registration(&7u32, &[shown(&legal, "privacy", "en")], NOW),
        Err(RegistrationError::Missing { .. })
    ));
}

#[test]
fn a_stale_version_or_a_wrong_digest_is_refused() {
    let legal = legal();
    let good = shown(&legal, "terms", "en");

    let stale = Acceptance {
        version: "2026-07".into(),
        ..good.clone()
    };
    assert_eq!(
        legal.verify_registration(&7u32, &[stale], NOW),
        Err(RegistrationError::Rejected {
            slug: "terms".into(),
            reason: AcceptanceError::Stale
        })
    );

    let wrong = Acceptance {
        digest: Digest::from_sha256([9; 32]),
        ..good.clone()
    };
    assert_eq!(
        legal.verify_registration(&7u32, &[wrong], NOW),
        Err(RegistrationError::Rejected {
            slug: "terms".into(),
            reason: AcceptanceError::Stale
        })
    );

    // The German digest does not verify the English text.
    let mixed = Acceptance {
        digest: shown(&legal, "terms", "de").digest,
        ..good
    };
    assert!(legal.verify_registration(&7u32, &[mixed], NOW).is_err());
}

#[test]
fn an_unknown_external_or_unconsentable_document_cannot_be_accepted() {
    let legal = legal();
    let template = shown(&legal, "terms", "en");
    for slug in ["nope", "imprint", "cookies", "../terms", ""] {
        let acceptance = Acceptance {
            slug: slug.to_owned(),
            ..template.clone()
        };
        assert_eq!(
            legal.verify_acceptance(&acceptance).unwrap_err(),
            AcceptanceError::UnknownDocument,
            "slug {slug:?}"
        );
    }
}

#[test]
fn an_unpublished_locale_is_reported_as_such_before_staleness() {
    let legal = legal();
    let acceptance = Acceptance {
        locale: tag("fr"),
        version: "wrong".into(),
        ..shown(&legal, "terms", "en")
    };
    assert_eq!(
        legal.verify_acceptance(&acceptance).unwrap_err(),
        AcceptanceError::UnpublishedLocale
    );
}

#[test]
fn a_document_accepted_twice_in_one_registration_is_refused() {
    let legal = legal();
    let terms = shown(&legal, "terms", "en");
    assert_eq!(
        legal.verify_registration(&7u32, &[terms.clone(), terms], NOW),
        Err(RegistrationError::Duplicate {
            slug: "terms".into()
        })
    );
}

#[test]
fn the_status_follows_the_records_and_the_grace_window() {
    let legal = legal();
    let inside_grace = datetime!(2026-09-10 0:00 UTC);
    let after_grace = datetime!(2026-09-15 0:00 UTC);

    let status = legal.consent_status::<u32>(&[], inside_grace);
    assert_eq!(status.outstanding().count(), 2);
    assert!(!status.is_blocked(), "still inside the grace period");
    assert!(legal.consent_status::<u32>(&[], after_grace).is_blocked());

    let accepted = legal
        .verify_registration(&7u32, &[shown(&legal, "terms", "en")], inside_grace)
        .expect("valid");
    let status = legal.consent_status(&accepted, after_grace);
    assert!(!status.is_blocked());
    assert_eq!(
        status
            .outstanding()
            .map(|d| d.slug.as_str())
            .collect::<Vec<_>>(),
        ["privacy"],
        "only the acknowledgement is still owed"
    );
}

#[test]
fn a_withdrawal_names_the_current_revision_and_the_text_last_recorded() {
    let legal = legal();
    let accepted = legal
        .verify_registration(&7u32, &[shown(&legal, "terms", "de")], NOW)
        .expect("valid");
    let withdrawal = Withdrawal {
        slug: "terms".into(),
    };

    let record = legal
        .withdrawal_record(&withdrawal, &7u32, &accepted, NOW)
        .expect("valid");
    assert_eq!(record.kind, RecordKind::Withdrawn);
    assert_eq!(
        record.locale,
        tag("de"),
        "the locale the subject accepted in"
    );
    assert_eq!(record.digest, accepted[0].digest);
    assert_eq!(record.channel, Channel::Api);

    let record = legal
        .withdrawal_record(&withdrawal, &7u32, &[], NOW)
        .expect("valid");
    assert_eq!(
        record.locale,
        tag("de"),
        "the first published locale without a record"
    );

    for slug in ["nope", "imprint", "cookies"] {
        assert_eq!(
            legal
                .withdrawal_record(&Withdrawal { slug: slug.into() }, &7u32, &[], NOW)
                .unwrap_err(),
            AcceptanceError::UnknownDocument
        );
    }
    let after: Vec<ConsentRecord<u32>> = accepted.into_iter().chain([record]).collect();
    assert!(
        legal
            .consent_status(&after, datetime!(2026-01-01 0:00 UTC))
            .outstanding()
            .any(|d| d.slug == "terms")
    );
}

#[test]
fn a_replaced_catalog_changes_what_an_old_acceptance_verifies_against() {
    let legal = legal();
    let acceptance = shown(&legal, "terms", "en");
    assert!(legal.verify_acceptance(&acceptance).is_ok());

    let mut edited = terrace_legal::LegalConfig::default();
    let mut terms = hosted(&[("en", "# Terms v2")]);
    terms.consent = ConsentPolicy {
        requirement: Requirement::Accept,
        version: Some("2026-08".into()),
        ..ConsentPolicy::default()
    };
    edited.documents.insert("terms".into(), terms);
    legal.replace(&edited).expect("valid");

    assert_eq!(
        legal.verify_acceptance(&acceptance).unwrap_err(),
        AcceptanceError::Stale,
        "an acceptance of text that is no longer served is stale even with the same version"
    );
}

#[tokio::test]
async fn the_memory_store_stores_appended_records_per_subject() {
    let legal = legal();
    let store = MemoryConsentStore::<u32>::default();
    let records = legal
        .verify_registration(&7u32, &[shown(&legal, "terms", "en")], NOW)
        .expect("valid");
    store.append(&records).await.expect("infallible");
    assert_eq!(store.latest(&7).await.expect("infallible"), records);
    assert!(store.latest(&8).await.expect("infallible").is_empty());
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn the_memory_store_passes_the_conformance_suite() {
    terrace_legal::testing::consent_conformance(&MemoryConsentStore::<u32>::default(), |n| n).await;
}

#[cfg(feature = "testing")]
#[tokio::test]
async fn the_memory_store_passes_the_conformance_suite_for_a_string_subject() {
    terrace_legal::testing::consent_conformance(&MemoryConsentStore::<String>::default(), |n| {
        format!("user-{n}")
    })
    .await;
}
