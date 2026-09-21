//! Consent over HTTP: three routes, and a layer that blocks a subject who owes an acceptance.
//!
//! The host supplies three things: a [`SubjectResolver`] that says who is calling, a
//! [`terrace_legal::consent::ConsentStore`] that persists records, and, optionally, a [`Clock`].
//! They meet in a [`ConsentState`], which both [`routes`] and [`RequireConsent`] take.
//!
//! # Routes
//!
//! Mount [`routes`] under the host's own authenticated prefix, for example `/v1/me/consent`.
//!
//! | Route | Behaviour |
//! |---|---|
//! | `GET /` | The caller's [`ConsentStatus`](terrace_legal_model::consent::ConsentStatus) |
//! | `POST /` with an [`Acceptance`](terrace_legal_model::consent::Acceptance) | `204` when the version, locale and digest match what is served now. `409` `consent-stale` when the version or digest differs, so a reader cannot accept text they were not shown. `404` for an unknown or non-consentable document. `422` for a locale that is not published |
//! | `POST /withdraw` with a [`Withdrawal`](terrace_legal_model::consent::Withdrawal) | `204`, appending a withdrawal. The host decides what follows from it |
//!
//! # The layer
//!
//! [`RequireConsent`] answers `403` with problem type `consent-required` and the outstanding
//! documents, when the caller owes a blocking acceptance. It never blocks the [`Exemptions`]:
//! the legal routes, the consent routes, sign-out, account export and account erasure. Access to
//! one's own data and its deletion must never depend on accepting new terms.

mod clock;
mod exempt;
mod require;
mod routes;
mod state;
mod subject;

pub use clock::{Clock, SystemClock};
pub use exempt::{ExemptionError, Exemptions, Mounts};
pub use require::{RequireConsent, RequireConsentService};
pub use routes::routes;
pub use state::ConsentState;
pub use subject::SubjectResolver;
