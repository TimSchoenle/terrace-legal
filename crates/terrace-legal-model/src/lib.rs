//! Wire types, locale negotiation and consent evaluation for operator-configured legal
//! documents.
//!
//! This crate is the bottom of the dependency graph. It knows nothing about files, terrace,
//! axum or Dioxus, so it compiles for `wasm32-unknown-unknown` and a browser bundle can share the
//! exact types a server serialises.
//!
//! # Contents
//!
//! - [`LocaleTag`] and [`AcceptLanguage`] are the two inputs to [`negotiate`], which picks the
//!   locale of a body or a title.
//! - [`LegalIndexEntry`], [`LegalDocumentView`], [`LegalKind`] and [`LegalParams`] are the
//!   payloads of the two HTTP routes.
//! - [`Slug`] and [`Digest`] are the validated names of a document and of the exact text a
//!   reader was shown.
//!
//! # Features
//!
//! | Feature | Effect |
//! |---|---|
//! | `utoipa` | `ToSchema` and `IntoParams` on the wire types |
//! | `consent` | Consent fields on the wire types, and the `consent` module |
//!
//! Cargo features are additive. If any crate in a dependency graph turns on `consent`, every
//! consumer of this crate sees the extra fields, and a published `OpenAPI` document changes with
//! them. A host that must not change its API should diff the generated document in CI.
//!
//! # Design record
//!
//! **Why the locale grammar is small.** Configuration keys, the `lang` query parameter and
//! `Accept-Language` all have to agree on what a locale is, and an environment variable cannot
//! spell `de-AT` with a hyphen. A grammar of language, script and region, with `_` accepted for
//! `-` and case normalised on parse, gives every input one canonical value. Full BCP 47 variants
//! and extensions would need a registry to validate, and nothing in a legal-document catalogue
//! needs them. A tag outside the grammar is refused where it enters, never guessed at.
//!
//! **Why one function negotiates bodies and titles.** A title chosen by a separate chain can
//! name a language the body is not in, which shows a French heading over a German text. Both are
//! chosen by [`negotiate`] over the same available set.
//!
//! **Why `q=0` is dropped.** RFC 9110 defines it as "not acceptable". Keeping it as a fallback
//! serves a reader the language they refused.
//!
//! **Why `matched_by` is not on the wire.** A client knows which locale it asked for and which
//! the response carries, so it can tell a substitution by comparing them. Adding a field would
//! change the published schema for nothing a client cannot derive.
//!
//! **Why consent evaluation lives here.** It is pure: time and records come in as arguments. A
//! browser can therefore run the same rule the server enforces, and property tests can drive it
//! without a store or a clock.

mod accept;
mod locale;
mod negotiate;
mod slug;
mod wire;

#[cfg(feature = "consent")]
pub mod consent;

pub use accept::AcceptLanguage;
pub use locale::{LocaleTag, ParseLocaleError};
pub use negotiate::{MatchedBy, Negotiated, negotiate};
pub use slug::{MAX_SLUG_LEN, ParseSlugError, Slug};
#[cfg(feature = "consent")]
pub use wire::ConsentSummary;
pub use wire::{
    Digest, DocumentFormat, LegalDocumentView, LegalIndexEntry, LegalKind, LegalParams,
    ParseDigestError,
};
