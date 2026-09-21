//! Consent records, policies and their pure evaluation.
//!
//! Nothing here reads a clock or stores anything. [`evaluate`] takes the time as an argument and
//! the records as a slice, so a host decides where each comes from.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::{Date, Duration, OffsetDateTime};

use crate::locale::LocaleTag;
use crate::slug::Slug;
use crate::wire::Digest;

/// What a reader has to do with a document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub enum Requirement {
    /// Nothing is recorded or evaluated. This is the default.
    #[default]
    None,
    /// The document is shown and the reader's acknowledgement is recorded. It never blocks.
    Acknowledge,
    /// The reader has to accept the document: at registration always, and for an existing
    /// subject once the grace window ends.
    Accept,
}

impl Requirement {
    /// Every variant, for a test that compares them with a hand-written list of spellings.
    pub const ALL: [Self; 3] = [Self::None, Self::Acknowledge, Self::Accept];

    /// Returns the spelling used in configuration and on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Acknowledge => "acknowledge",
            Self::Accept => "accept",
        }
    }
}

/// What a consent record states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordKind {
    /// The subject accepted the document.
    Accepted,
    /// The subject acknowledged the document.
    Acknowledged,
    /// The subject withdrew an earlier acceptance or acknowledgement.
    Withdrawn,
}

impl RecordKind {
    /// Orders records that carry the same timestamp so the outcome does not depend on their
    /// order in a slice. A withdrawal wins, because it is the subject's last word.
    fn tie_break(self) -> u8 {
        match self {
            Self::Acknowledged => 0,
            Self::Accepted => 1,
            Self::Withdrawn => 2,
        }
    }
}

/// How a consent record came to exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// The subject accepted while creating the account.
    Registration,
    /// The subject accepted a prompt shown to an existing account.
    Prompt,
    /// The subject accepted or withdrew through the API.
    Api,
}

/// One append-only fact about a subject and a document.
///
/// `digest` and `locale` identify exactly which bytes the subject was shown, independently of
/// `version`. That is what makes an acceptance demonstrable later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord<S> {
    /// Who the record is about; the host chooses the type.
    pub subject: S,
    /// The document.
    pub slug: Slug,
    /// The operator's revision the subject saw.
    pub version: String,
    /// The locale of the text the subject saw.
    pub locale: LocaleTag,
    /// The digest of the text the subject saw.
    pub digest: Digest,
    /// What the record states.
    pub kind: RecordKind,
    /// How the record came to exist.
    pub channel: Channel,
    /// When the record was made.
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

/// The current consent policy of one document, with the digest of every published locale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// What the reader has to do.
    pub requirement: Requirement,
    /// The operator's current revision. Empty only when `requirement` is [`Requirement::None`].
    pub version: String,
    /// The first day the current revision applied, which starts the grace period.
    pub effective: Option<Date>,
    /// Days after `effective` during which an existing subject may continue without accepting.
    pub grace_days: u32,
    /// The digest of the text served for each published locale.
    pub digests: BTreeMap<LocaleTag, Digest>,
}

impl Policy {
    /// Returns the instant from which a missing acceptance blocks, or `None` when it blocks
    /// immediately.
    ///
    /// A grace period too far in the future to represent counts as never ending.
    fn blocks_from(&self) -> Option<Option<OffsetDateTime>> {
        let effective = self.effective?;
        Some(
            effective
                .midnight()
                .assume_utc()
                .checked_add(Duration::days(i64::from(self.grace_days))),
        )
    }
}

/// A subject's standing on one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct DocumentConsent {
    /// The document.
    pub slug: String,
    /// What the operator requires.
    pub requirement: Requirement,
    /// The operator's current revision.
    pub version: String,
    /// Whether the subject still owes an acceptance or acknowledgement of this revision.
    pub outstanding: bool,
    /// Whether the outstanding acceptance already denies the subject access to the service.
    pub blocking: bool,
    /// Whether the text changed since the subject's record without a new revision. It is
    /// informational, for audit, and never blocks.
    pub content_changed_since: bool,
}

/// A subject's standing on every document that has a policy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ConsentStatus {
    /// One entry per document whose requirement is not [`Requirement::None`], in catalog order.
    pub documents: Vec<DocumentConsent>,
}

impl ConsentStatus {
    /// Returns the documents the subject still owes something on.
    pub fn outstanding(&self) -> impl Iterator<Item = &DocumentConsent> {
        self.documents
            .iter()
            .filter(|document| document.outstanding)
    }

    /// Returns whether any outstanding document blocks the subject.
    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.documents.iter().any(|document| document.blocking)
    }
}

/// A subject's claim that they were shown a specific text and accept it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Acceptance {
    /// The document.
    pub slug: String,
    /// The revision the subject saw.
    pub version: String,
    /// The locale of the text the subject saw.
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub locale: LocaleTag,
    /// The digest of the text the subject saw.
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub digest: Digest,
}

/// A subject's request to withdraw from a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Withdrawal {
    /// The document.
    pub slug: String,
}

/// Works out what a subject still owes.
///
/// A document is outstanding when its requirement is not `None` and the subject's latest record
/// for it is missing, names another version, is a withdrawal, or is an acknowledgement of a
/// document that has to be accepted. It is blocking when it is outstanding, its requirement is
/// [`Requirement::Accept`], and either there is no `effective` date or `now` has reached
/// `effective` plus `grace_days`.
///
/// `latest` may hold any number of records in any order, including several per document: the
/// latest by timestamp decides, and a withdrawal wins a tie. Records for documents without a
/// policy are ignored.
#[must_use]
pub fn evaluate<S>(
    policies: &[(Slug, Policy)],
    latest: &[ConsentRecord<S>],
    now: OffsetDateTime,
) -> ConsentStatus {
    let documents = policies
        .iter()
        .filter(|(_, policy)| policy.requirement != Requirement::None)
        .map(|(slug, policy)| {
            let record = latest
                .iter()
                .filter(|record| record.slug == *slug)
                .max_by(|a, b| {
                    (a.at, a.kind.tie_break(), &a.version, &a.digest).cmp(&(
                        b.at,
                        b.kind.tie_break(),
                        &b.version,
                        &b.digest,
                    ))
                });

            let satisfied = record.is_some_and(|record| {
                record.version == policy.version
                    && match record.kind {
                        RecordKind::Accepted => true,
                        RecordKind::Acknowledged => policy.requirement == Requirement::Acknowledge,
                        RecordKind::Withdrawn => false,
                    }
            });
            let outstanding = !satisfied;
            let blocking = outstanding
                && policy.requirement == Requirement::Accept
                && match policy.blocks_from() {
                    None => true,
                    Some(Some(from)) => now >= from,
                    Some(None) => false,
                };
            let content_changed_since = record.is_some_and(|record| {
                record.version == policy.version
                    && record.kind != RecordKind::Withdrawn
                    && policy.digests.get(&record.locale) != Some(&record.digest)
            });

            DocumentConsent {
                slug: slug.to_string(),
                requirement: policy.requirement,
                version: policy.version.clone(),
                outstanding,
                blocking,
                content_changed_since,
            }
        })
        .collect();
    ConsentStatus { documents }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use time::macros::{date, datetime};

    fn slug(text: &str) -> Slug {
        text.parse().expect("a valid slug")
    }

    fn locale() -> LocaleTag {
        "en".parse().expect("valid")
    }

    fn digest(seed: u8) -> Digest {
        Digest::from_sha256([seed; 32])
    }

    fn policy(requirement: Requirement, effective: Option<Date>, grace_days: u32) -> Policy {
        Policy {
            requirement,
            version: "v2".into(),
            effective,
            grace_days,
            digests: BTreeMap::from([(locale(), digest(2))]),
        }
    }

    fn record(kind: RecordKind, version: &str, seed: u8, at: OffsetDateTime) -> ConsentRecord<u8> {
        ConsentRecord {
            subject: 1,
            slug: slug("terms"),
            version: version.into(),
            locale: locale(),
            digest: digest(seed),
            kind,
            channel: Channel::Prompt,
            at,
        }
    }

    const NOW: OffsetDateTime = datetime!(2026-09-21 12:00 UTC);

    fn status(
        policy: Policy,
        records: &[ConsentRecord<u8>],
        now: OffsetDateTime,
    ) -> DocumentConsent {
        let policies = [(slug("terms"), policy)];
        let mut out = evaluate(&policies, records, now).documents;
        assert_eq!(out.len(), 1);
        out.remove(0)
    }

    #[test]
    fn a_requirement_of_none_is_not_evaluated() {
        let policies = [(slug("terms"), policy(Requirement::None, None, 0))];
        assert!(evaluate::<u8>(&policies, &[], NOW).documents.is_empty());
    }

    #[test]
    fn requirement_spellings_match_the_wire() {
        for requirement in Requirement::ALL {
            assert_eq!(
                serde_json::to_string(&requirement).expect("serialises"),
                format!("\"{}\"", requirement.as_str())
            );
        }
    }

    /// The table of the consent rules: requirement x record x grace position.
    #[test]
    fn the_evaluation_table() {
        let before_grace = Some(date!(2026 - 09 - 20)); // grace_days 14 ends 2026-10-04
        let after_grace = Some(date!(2026 - 08 - 01)); // ended 2026-08-15
        let current = |kind| record(kind, "v2", 2, datetime!(2026-09-01 0:00 UTC));
        let old = |kind| record(kind, "v1", 1, datetime!(2026-01-01 0:00 UTC));

        // (requirement, records, effective, expected outstanding, expected blocking)
        type Case = (
            Requirement,
            Vec<ConsentRecord<u8>>,
            Option<Date>,
            bool,
            bool,
        );
        let cases: Vec<Case> = vec![
            // Acknowledge never blocks.
            (Requirement::Acknowledge, vec![], after_grace, true, false),
            (Requirement::Acknowledge, vec![], None, true, false),
            (
                Requirement::Acknowledge,
                vec![old(RecordKind::Acknowledged)],
                after_grace,
                true,
                false,
            ),
            (
                Requirement::Acknowledge,
                vec![current(RecordKind::Acknowledged)],
                after_grace,
                false,
                false,
            ),
            (
                Requirement::Acknowledge,
                vec![current(RecordKind::Accepted)],
                after_grace,
                false,
                false,
            ),
            (
                Requirement::Acknowledge,
                vec![current(RecordKind::Withdrawn)],
                after_grace,
                true,
                false,
            ),
            // Accept blocks outside the grace window and when there is no effective date.
            (Requirement::Accept, vec![], before_grace, true, false),
            (Requirement::Accept, vec![], after_grace, true, true),
            (Requirement::Accept, vec![], None, true, true),
            (
                Requirement::Accept,
                vec![old(RecordKind::Accepted)],
                before_grace,
                true,
                false,
            ),
            (
                Requirement::Accept,
                vec![old(RecordKind::Accepted)],
                after_grace,
                true,
                true,
            ),
            (
                Requirement::Accept,
                vec![current(RecordKind::Accepted)],
                after_grace,
                false,
                false,
            ),
            (
                Requirement::Accept,
                vec![current(RecordKind::Accepted)],
                None,
                false,
                false,
            ),
            // An acknowledgement is not an acceptance.
            (
                Requirement::Accept,
                vec![current(RecordKind::Acknowledged)],
                after_grace,
                true,
                true,
            ),
            // A withdrawal makes the document outstanding again.
            (
                Requirement::Accept,
                vec![current(RecordKind::Withdrawn)],
                after_grace,
                true,
                true,
            ),
            (
                Requirement::Accept,
                vec![current(RecordKind::Withdrawn)],
                before_grace,
                true,
                false,
            ),
        ];

        for (index, (requirement, records, effective, outstanding, blocking)) in
            cases.into_iter().enumerate()
        {
            let got = status(policy(requirement, effective, 14), &records, NOW);
            assert_eq!(
                (got.outstanding, got.blocking),
                (outstanding, blocking),
                "case {index}: {requirement:?} {records:?} {effective:?}"
            );
        }
    }

    #[test]
    fn the_grace_window_ends_exactly_at_effective_plus_grace_days() {
        let accept = || policy(Requirement::Accept, Some(date!(2026 - 09 - 01)), 14);
        // 2026-09-01 + 14 days = 2026-09-15 00:00 UTC.
        let last_second = datetime!(2026-09-14 23:59:59 UTC);
        let first_second = datetime!(2026-09-15 0:00 UTC);
        assert!(!status(accept(), &[], last_second).blocking);
        assert!(status(accept(), &[], first_second).blocking);
    }

    #[test]
    fn a_grace_period_beyond_the_representable_range_never_ends() {
        let far = policy(Requirement::Accept, Some(Date::MAX), 365);
        assert!(!status(far, &[], NOW).blocking);
    }

    #[test]
    fn a_newer_record_supersedes_an_older_one() {
        let records = [
            record(
                RecordKind::Accepted,
                "v2",
                2,
                datetime!(2026-09-01 0:00 UTC),
            ),
            record(
                RecordKind::Withdrawn,
                "v2",
                2,
                datetime!(2026-09-02 0:00 UTC),
            ),
        ];
        assert!(status(policy(Requirement::Accept, None, 0), &records, NOW).outstanding);
        let records = [
            record(
                RecordKind::Withdrawn,
                "v2",
                2,
                datetime!(2026-09-01 0:00 UTC),
            ),
            record(
                RecordKind::Accepted,
                "v2",
                2,
                datetime!(2026-09-02 0:00 UTC),
            ),
        ];
        assert!(!status(policy(Requirement::Accept, None, 0), &records, NOW).outstanding);
    }

    #[test]
    fn a_withdrawal_wins_a_timestamp_tie_in_either_order() {
        let at = datetime!(2026-09-01 0:00 UTC);
        let accepted = record(RecordKind::Accepted, "v2", 2, at);
        let withdrawn = record(RecordKind::Withdrawn, "v2", 2, at);
        for records in [[accepted.clone(), withdrawn.clone()], [withdrawn, accepted]] {
            assert!(status(policy(Requirement::Accept, None, 0), &records, NOW).outstanding);
        }
    }

    #[test]
    fn an_edit_without_a_new_version_is_reported_but_never_blocks() {
        // The subject accepted v2 when its text digested to 1; it now digests to 2.
        let records = [record(
            RecordKind::Accepted,
            "v2",
            1,
            datetime!(2026-09-01 0:00 UTC),
        )];
        let got = status(policy(Requirement::Accept, None, 0), &records, NOW);
        assert!(got.content_changed_since);
        assert!(!got.outstanding && !got.blocking);

        let records = [record(
            RecordKind::Accepted,
            "v2",
            2,
            datetime!(2026-09-01 0:00 UTC),
        )];
        assert!(!status(policy(Requirement::Accept, None, 0), &records, NOW).content_changed_since);
    }

    #[test]
    fn records_for_other_documents_are_ignored() {
        let mut other = record(
            RecordKind::Accepted,
            "v2",
            2,
            datetime!(2026-09-01 0:00 UTC),
        );
        other.slug = slug("privacy");
        assert!(status(policy(Requirement::Accept, None, 0), &[other], NOW).outstanding);
    }

    #[test]
    fn status_helpers_report_outstanding_and_blocked() {
        let policies = [
            (slug("terms"), policy(Requirement::Accept, None, 0)),
            (slug("privacy"), policy(Requirement::None, None, 0)),
        ];
        let status = evaluate::<u8>(&policies, &[], NOW);
        assert!(status.is_blocked());
        assert_eq!(status.outstanding().count(), 1);
        assert!(!ConsentStatus::default().is_blocked());
    }

    #[test]
    fn the_records_serialise_with_an_rfc_3339_timestamp() {
        let json = serde_json::to_value(record(
            RecordKind::Accepted,
            "v2",
            2,
            datetime!(2026-09-01 10:30 UTC),
        ))
        .expect("serialises");
        assert_eq!(json["at"], "2026-09-01T10:30:00Z");
        assert_eq!(json["kind"], "accepted");
        assert_eq!(json["channel"], "prompt");
    }

    fn arb_record() -> impl Strategy<Value = ConsentRecord<u8>> {
        (
            prop_oneof![
                Just(RecordKind::Accepted),
                Just(RecordKind::Acknowledged),
                Just(RecordKind::Withdrawn)
            ],
            prop_oneof![Just("v1"), Just("v2")],
            0u8..3,
            0i64..5,
        )
            .prop_map(|(kind, version, seed, day)| {
                record(
                    kind,
                    version,
                    seed,
                    datetime!(2026-09-01 0:00 UTC) + Duration::days(day),
                )
            })
    }

    proptest! {
        #[test]
        fn the_result_does_not_depend_on_the_order_of_the_records(
            records in proptest::collection::vec(arb_record(), 0..8),
            rotation in 0usize..8,
        ) {
            let policies = [(slug("terms"), policy(Requirement::Accept, Some(date!(2026 - 09 - 01)), 7))];
            let mut shuffled = records.clone();
            shuffled.reverse();
            if !shuffled.is_empty() {
                let by = rotation % shuffled.len();
                shuffled.rotate_left(by);
            }
            prop_assert_eq!(
                evaluate(&policies, &records, NOW),
                evaluate(&policies, &shuffled, NOW)
            );
        }

        #[test]
        fn accepting_the_current_version_leaves_nothing_outstanding(
            records in proptest::collection::vec(arb_record(), 0..8),
        ) {
            let policies = [(slug("terms"), policy(Requirement::Accept, None, 0))];
            let mut with_acceptance = records;
            with_acceptance.push(record(RecordKind::Accepted, "v2", 2, datetime!(2030-01-01 0:00 UTC)));
            let status = evaluate(&policies, &with_acceptance, NOW);
            prop_assert_eq!(status.outstanding().count(), 0);
            prop_assert!(!status.is_blocked());
        }

        #[test]
        fn a_later_withdrawal_always_makes_the_document_outstanding(
            records in proptest::collection::vec(arb_record(), 0..8),
        ) {
            let policies = [(slug("terms"), policy(Requirement::Acknowledge, None, 0))];
            let mut with_withdrawal = records;
            with_withdrawal.push(record(RecordKind::Withdrawn, "v2", 2, datetime!(2030-01-01 0:00 UTC)));
            prop_assert_eq!(evaluate(&policies, &with_withdrawal, NOW).outstanding().count(), 1);
        }
    }
}
