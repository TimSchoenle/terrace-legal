//! Consent: checking acceptances against what is served, and storing what was accepted.
//!
//! The pure rule that decides what is outstanding lives in
//! [`terrace_legal_model::consent::evaluate`]. This module adds the parts that need the catalog
//! and a store.

use std::future::Future;
use std::sync::Mutex;

use terrace_legal_model::consent::{
    Acceptance, Channel, ConsentRecord, ConsentStatus, Policy, RecordKind, Requirement, Withdrawal,
    evaluate,
};
use terrace_legal_model::{Digest, LocaleTag, Slug};
use time::OffsetDateTime;

use crate::catalog::Content;
use crate::legal::Legal;

/// Why an acceptance was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AcceptanceError {
    /// No document has that slug, or the document has no consent policy.
    #[error("no such document, or it requires no consent")]
    UnknownDocument,
    /// The document is not published in the named locale.
    #[error("the document is not published in that locale")]
    UnpublishedLocale,
    /// The version or digest differs from what is served now, so the reader cannot have been
    /// shown the text being accepted.
    #[error("the document changed since it was shown")]
    Stale,
}

/// Why a registration was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RegistrationError {
    /// A document that has to be accepted at registration was not.
    #[error("the document `{slug}` has to be accepted")]
    Missing {
        /// The document.
        slug: Slug,
    },
    /// The same document was accepted twice.
    #[error("the document `{slug}` was accepted more than once")]
    Duplicate {
        /// The document.
        slug: String,
    },
    /// An acceptance was refused.
    #[error("the acceptance of `{slug}` was refused: {reason}")]
    Rejected {
        /// The document.
        slug: String,
        /// Why.
        reason: AcceptanceError,
    },
}

/// An acceptance that names the text being served now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAcceptance {
    /// The document.
    pub slug: Slug,
    /// The current revision.
    pub version: String,
    /// The locale of the text shown.
    pub locale: LocaleTag,
    /// The digest of the text shown.
    pub digest: Digest,
    /// What the operator requires of the document.
    pub requirement: Requirement,
}

impl VerifiedAcceptance {
    /// Builds the record that stores this acceptance.
    ///
    /// The record states an acceptance when the document has to be accepted, and an
    /// acknowledgement otherwise.
    #[must_use]
    pub fn into_record<S>(
        self,
        subject: S,
        channel: Channel,
        now: OffsetDateTime,
    ) -> ConsentRecord<S> {
        ConsentRecord {
            subject,
            slug: self.slug,
            version: self.version,
            locale: self.locale,
            digest: self.digest,
            kind: match self.requirement {
                Requirement::Accept => RecordKind::Accepted,
                Requirement::Acknowledge | Requirement::None => RecordKind::Acknowledged,
            },
            channel,
            at: now,
        }
    }
}

impl Legal {
    /// Returns the consent policy of every document that has one, in catalog order.
    #[must_use]
    pub fn consent_policies(&self) -> Vec<(Slug, Policy)> {
        self.catalog()
            .documents()
            .iter()
            .filter_map(|document| {
                document
                    .consent
                    .clone()
                    .map(|policy| (document.slug.clone(), policy))
            })
            .collect()
    }

    /// Works out what a subject owes, given their latest record per document.
    ///
    /// `now` is a parameter because nothing in this library reads a clock.
    #[must_use]
    pub fn consent_status<S>(
        &self,
        latest: &[ConsentRecord<S>],
        now: OffsetDateTime,
    ) -> ConsentStatus {
        evaluate(&self.consent_policies(), latest, now)
    }

    /// Checks that an acceptance names the text that is served now.
    ///
    /// The locale is checked before the version and digest, so a locale that is not published is
    /// reported as such rather than as stale.
    ///
    /// # Errors
    ///
    /// See [`AcceptanceError`].
    pub fn verify_acceptance(
        &self,
        acceptance: &Acceptance,
    ) -> Result<VerifiedAcceptance, AcceptanceError> {
        let catalog = self.catalog();
        let document = catalog
            .get(&acceptance.slug)
            .ok_or(AcceptanceError::UnknownDocument)?;
        let policy = document
            .consent
            .as_ref()
            .ok_or(AcceptanceError::UnknownDocument)?;
        let Content::Hosted(variants) = &document.content else {
            return Err(AcceptanceError::UnknownDocument);
        };
        let variant = variants
            .get(&acceptance.locale)
            .ok_or(AcceptanceError::UnpublishedLocale)?;
        if acceptance.version != policy.version || acceptance.digest != variant.digest {
            return Err(AcceptanceError::Stale);
        }
        Ok(VerifiedAcceptance {
            slug: document.slug.clone(),
            version: policy.version.clone(),
            locale: acceptance.locale.clone(),
            digest: variant.digest.clone(),
            requirement: policy.requirement,
        })
    }

    /// Turns the acceptances submitted with a registration into records, or refuses them.
    ///
    /// Every document whose requirement is [`Requirement::Accept`] has to be accepted, regardless
    /// of any grace period: the grace period exists for subjects who registered under an earlier
    /// revision. A document that only has to be acknowledged is recorded when it is present and
    /// is not required. The records are returned rather than stored, so a host can insert them in
    /// the transaction that creates the account.
    ///
    /// # Errors
    ///
    /// See [`RegistrationError`]. Nothing is returned for a partly valid submission.
    pub fn verify_registration<S: Clone>(
        &self,
        subject: &S,
        acceptances: &[Acceptance],
        now: OffsetDateTime,
    ) -> Result<Vec<ConsentRecord<S>>, RegistrationError> {
        let mut records: Vec<ConsentRecord<S>> = Vec::with_capacity(acceptances.len());
        for acceptance in acceptances {
            let verified = self.verify_acceptance(acceptance).map_err(|reason| {
                RegistrationError::Rejected {
                    slug: acceptance.slug.clone(),
                    reason,
                }
            })?;
            if records.iter().any(|record| record.slug == verified.slug) {
                return Err(RegistrationError::Duplicate {
                    slug: acceptance.slug.clone(),
                });
            }
            records.push(verified.into_record(subject.clone(), Channel::Registration, now));
        }

        for (slug, policy) in self.consent_policies() {
            if policy.requirement == Requirement::Accept
                && !records.iter().any(|record| record.slug == slug)
            {
                return Err(RegistrationError::Missing { slug });
            }
        }
        Ok(records)
    }

    /// Builds the record for a withdrawal.
    ///
    /// The record names the current revision and the locale and digest of the text the subject
    /// last recorded against, or of the first published locale when there is none.
    ///
    /// # Errors
    ///
    /// [`AcceptanceError::UnknownDocument`] when the document does not exist, has no consent
    /// policy, or is hosted elsewhere.
    pub fn withdrawal_record<S: Clone>(
        &self,
        withdrawal: &Withdrawal,
        subject: &S,
        latest: &[ConsentRecord<S>],
        now: OffsetDateTime,
    ) -> Result<ConsentRecord<S>, AcceptanceError> {
        let catalog = self.catalog();
        let document = catalog
            .get(&withdrawal.slug)
            .ok_or(AcceptanceError::UnknownDocument)?;
        let policy = document
            .consent
            .as_ref()
            .ok_or(AcceptanceError::UnknownDocument)?;
        let Content::Hosted(variants) = &document.content else {
            return Err(AcceptanceError::UnknownDocument);
        };
        let seen = latest
            .iter()
            .filter(|record| record.slug == document.slug)
            .max_by_key(|record| record.at)
            .and_then(|record| variants.get(&record.locale).map(|v| (&record.locale, v)));
        let (locale, variant) = seen
            .or_else(|| variants.iter().next())
            .ok_or(AcceptanceError::UnknownDocument)?;
        Ok(ConsentRecord {
            subject: subject.clone(),
            slug: document.slug.clone(),
            version: policy.version.clone(),
            locale: locale.clone(),
            digest: variant.digest.clone(),
            kind: RecordKind::Withdrawn,
            channel: Channel::Api,
            at: now,
        })
    }
}

/// Persistence for consent records, implemented by the host.
///
/// The store is append-only. [`Self::latest`] and [`Self::history`] are defined by the order
/// records were appended in, not by their timestamps, so a store needs no clock of its own.
///
/// With the `testing` feature, `terrace_legal::testing::consent_conformance` runs an
/// implementation against these rules.
pub trait ConsentStore: Send + Sync {
    /// Identifies whom a record is about.
    type Subject: Clone + Eq + Send + Sync;
    /// The store's failure type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Appends `records` in order, all or none.
    fn append(
        &self,
        records: &[ConsentRecord<Self::Subject>],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Returns the most recently appended record of `subject` for each document, in the order
    /// those records were appended.
    fn latest(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<Vec<ConsentRecord<Self::Subject>>, Self::Error>> + Send;

    /// Returns every record of `subject`, in the order they were appended.
    fn history(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<Vec<ConsentRecord<Self::Subject>>, Self::Error>> + Send;

    /// Deletes every record of `subject` and returns how many there were.
    fn erase(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send;
}

/// A shared store is a store, so a host can keep its own handle to what it gave a route.
impl<T: ConsentStore> ConsentStore for std::sync::Arc<T> {
    type Subject = T::Subject;
    type Error = T::Error;

    fn append(
        &self,
        records: &[ConsentRecord<Self::Subject>],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        (**self).append(records)
    }

    fn latest(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<Vec<ConsentRecord<Self::Subject>>, Self::Error>> + Send {
        (**self).latest(subject)
    }

    fn history(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<Vec<ConsentRecord<Self::Subject>>, Self::Error>> + Send {
        (**self).history(subject)
    }

    fn erase(
        &self,
        subject: &Self::Subject,
    ) -> impl Future<Output = Result<u64, Self::Error>> + Send {
        (**self).erase(subject)
    }
}

/// A [`ConsentStore`] held in memory, for tests and as the reference implementation.
#[derive(Debug)]
pub struct MemoryConsentStore<S> {
    records: Mutex<Vec<ConsentRecord<S>>>,
}

impl<S> Default for MemoryConsentStore<S> {
    fn default() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }
}

impl<S> MemoryConsentStore<S> {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<ConsentRecord<S>>> {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<S: Clone + Eq + Send + Sync> ConsentStore for MemoryConsentStore<S> {
    type Subject = S;
    type Error = std::convert::Infallible;

    async fn append(&self, records: &[ConsentRecord<S>]) -> Result<(), Self::Error> {
        self.lock().extend_from_slice(records);
        Ok(())
    }

    async fn latest(&self, subject: &S) -> Result<Vec<ConsentRecord<S>>, Self::Error> {
        let guard = self.lock();
        let mut latest: Vec<ConsentRecord<S>> = Vec::new();
        for record in guard.iter().filter(|record| record.subject == *subject) {
            latest.retain(|kept| kept.slug != record.slug);
            latest.push(record.clone());
        }
        Ok(latest)
    }

    async fn history(&self, subject: &S) -> Result<Vec<ConsentRecord<S>>, Self::Error> {
        Ok(self
            .lock()
            .iter()
            .filter(|record| record.subject == *subject)
            .cloned()
            .collect())
    }

    async fn erase(&self, subject: &S) -> Result<u64, Self::Error> {
        let mut guard = self.lock();
        let before = guard.len();
        guard.retain(|record| record.subject != *subject);
        Ok((before - guard.len()) as u64)
    }
}
