//! Configuration, validation and serving state for operator-configured legal documents.
//!
//! An operator publishes documents such as terms of service, a privacy notice and an imprint by
//! configuring them. This crate defines that configuration, validates it into a [`Catalog`], and
//! serves the result through a [`Legal`] handle. It does no file I/O and knows no web framework:
//! the axum and Dioxus adapters are separate crates.
//!
//! # Getting started
//!
//! A host nests [`LegalConfig`] in its own configuration under a key of its choosing, builds a
//! [`Catalog`] wherever it loads configuration, and refuses the configuration when that fails.
//!
//! ```
//! use terrace_legal::{Catalog, Legal, LegalConfig, Negotiation};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut config = LegalConfig::default();
//! config.default_locale = Some("en".into());
//! let mut terms = terrace_legal::LegalDocument::default();
//! terms.body.insert("en".into(), "# Terms\n\nBe kind.".into());
//! config.documents.insert("terms".into(), terms);
//!
//! let legal = Legal::new(Catalog::build(&config)?);
//! let served = legal.document("terms", &Negotiation::default())?;
//! assert_eq!(served.value.body, "# Terms\n\nBe kind.");
//! # Ok(())
//! # }
//! ```
//!
//! A refused configuration reports every problem at once, with `.with_prefix("legal")` turning
//! `documents.terms.body.EN` into the key the operator wrote.
//!
//! # Validation
//!
//! [`Catalog::build`] refuses a configuration that breaks any of these rules, and reports all of
//! its problems together.
//!
//! | Rule | Why |
//! |---|---|
//! | A slug matches `^[a-z0-9][a-z0-9_-]{0,63}$` | It becomes a URL path segment. `_` is allowed because an environment variable cannot contain `-` |
//! | A document has exactly one of `body` and `url` | It is either served here or linked |
//! | Every body is non-blank and within the size limit | A mis-pointed `_FILE`, such as a log file, is refused by key |
//! | A `url` is absolute `http` or `https`, has a host and no credentials | A link is published, so a password in it would be too |
//! | Locale keys of `body`, `title` and `default_locale` are locales, and two keys of one map do not normalise to the same tag | `de_at` and `de-AT` are the same locale |
//! | A title is not blank | It would render an empty link |
//!
//! With the `consent` feature there are three more: a policy with a requirement needs a `version`
//! and a hosted `body`, `effective` is a `YYYY-MM-DD` date, and a `grace_days` above zero needs
//! `effective`.
//!
//! # Rules
//!
//! Nothing here needs a fork to change. A host adds validation with a [`Rule`], registered on a
//! [`CatalogBuilder`] and kept for every later [`Legal::replace`] through
//! [`Legal::with_builder`]. [`RequiredDocuments`] is one, and shows the shape.
//!
//! A rule that only runs at boot is invisible to whoever writes the configuration, so a rule also
//! states what it checks in the configuration schema, through [`Rule::refinements`]. The builder
//! implements terrace-config's [`Refine`](terrace_config::schema::Refine), so one call publishes
//! every registered rule and every built-in check the schema can state, under whatever key the
//! host mounts the section at. Build the builder in one place and use it for both the schema and
//! the runtime, and the published contract and the check cannot drift apart:
//!
//! ```
//! use terrace_config::Terrace;
//! use terrace_config::schema::Describe;
//! use terrace_legal::{Catalog, CatalogBuilder, LegalConfig, RequiredDocuments};
//!
//! #[derive(Default, serde::Serialize, Describe)]
//! struct HostConfig {
//!     /// The legal documents.
//!     #[config(nested)]
//!     legal: LegalConfig,
//! }
//!
//! /// The one source of the host's legal rules, for the schema and for `Legal::with_builder`.
//! fn legal_rules() -> CatalogBuilder {
//!     Catalog::builder().rule(RequiredDocuments::new(["terms", "privacy"]))
//! }
//!
//! # fn main() -> Result<(), terrace_config::Error> {
//! let schema = Terrace::new("PORTFOLIO_")
//!     .schema::<HostConfig>()
//!     .with_defaults_from(&HostConfig::default())?
//!     .refine_with("legal", &legal_rules())?;
//!
//! let documents = schema.keys.iter().find(|key| key.path == "legal.documents").unwrap();
//! let constraint = documents.constraint.as_ref().unwrap();
//! assert_eq!(constraint["required"], serde_json::json!(["privacy", "terms"]));
//! // An empty map is refused at boot, so it is no longer published as the default.
//! assert!(documents.required);
//! assert_eq!(documents.default_value, None);
//! # Ok(())
//! # }
//! ```
//!
//! A refinement is never stricter than the check it states: any configuration the schema
//! rejects, the rule rejects too. The schema may say less than the rule, never more. [`Rule`]
//! documents the invariant a custom rule has to keep.
//!
//! The builder, not [`Legal`], is the source: a schema is generated where no configuration has
//! been loaded, so no catalog, and therefore no [`Legal`], exists yet.
//!
//! # Extending
//!
//! Beyond rules, the wire types in
//! [`model`] are `#[non_exhaustive]` and built with setters, so a field can be added without
//! breaking a caller, and `DocumentFormat` is a value, so a new body format needs no new type.
//! The consent store, the subject resolver and the clock in the adapters are traits for the same
//! reason.
//!
//! # Reloading
//!
//! Every change, whether a document's text, the document set or a title, is a configuration
//! change and arrives the way the host's loader delivers any other. A host that rebuilds its
//! runtime on reload builds a fresh [`Legal`] per generation. A host that keeps its state calls
//! [`Legal::replace`], which swaps the catalog atomically and leaves the previous one serving when
//! the new configuration is refused.
//!
//! # Features
//!
//! | Feature | Effect |
//! |---|---|
//! | `consent` | The `consent` configuration keys, the `consent` module, and registration checks |
//! | `testing` | The `testing` module. For dev-dependencies only |
//!
//! # Design record
//!
//! **Why the text is configuration.** The obvious design is a directory of files with a cache
//! that re-reads them when their modification time moves. That duplicates what the configuration
//! loader already does: it reads a mounted file through `_FILE` indirection, watches its
//! directory, rebuilds when it changes, keeps the previous runtime when the new one fails, and
//! documents the keys. Treating each body as a configuration value therefore removes file I/O, a
//! cache, a size check that races with the read, and blocking calls inside async handlers. What it
//! costs: a text edit is a configuration reload, a missing file refuses the load instead of
//! answering `404`, and the loader reads a mounted file in full before [`Catalog::build`] can
//! refuse it for size. Legal text changes a few times a year, so none of that is worth a second
//! mechanism. A host with a lighter reload uses [`Legal::replace`].
//!
//! **Why parsing produces a [`Catalog`].** [`LegalConfig`] is only what was deserialised. A value
//! that can be served has normalised locales, parsed URLs, a computed digest and a fixed order,
//! and the type system should say which of the two a function has. Everything derived from a body
//! is computed once per load, so a request is a map lookup.
//!
//! **Why the digest is always computed.** Consent needs it, and the entity tag uses it. It is
//! SHA-256 over the exact text served, which replaces a hash whose algorithm the standard library
//! does not specify and which could therefore differ between replicas built by different
//! compilers.
//!
//! **Why the entity tag covers the whole representation.** The response carries the title, the
//! updated line and the locale as well as the body. A tag over the body alone would answer a
//! conditional request with `304` after a reload that changed only a title.
//!
//! **Why consent refuses external documents.** The library cannot prove which text a reader saw
//! on a page it does not serve, so an acceptance of it would record nothing demonstrable.
//!
//! **Why a title is chosen by the locale of the body.** A separate chain can pick a language the
//! body is not in. The index takes a hosted document's title from the locale its body is served
//! in.

mod catalog;
mod config;
mod etag;
mod issues;
mod legal;
mod rules;

#[cfg(feature = "consent")]
pub mod consent;
#[cfg(feature = "testing")]
pub mod testing;

pub use catalog::{Catalog, CatalogBuilder, Limits};
#[cfg(feature = "consent")]
pub use config::ConsentPolicy;
pub use config::{LegalConfig, LegalDocument};
pub use etag::ETag;
pub use issues::{ConfigIssue, ConfigIssues};
pub use legal::{Legal, LegalError, Negotiation, Tagged};
pub use rules::{RequiredDocuments, Rule};
pub use terrace_legal_model as model;
