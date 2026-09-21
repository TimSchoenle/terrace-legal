//! HTTP responders and consent routes for `terrace-legal`, on axum.
//!
//! The crate turns a [`terrace_legal::Legal`] into responses: a strong `ETag`, a `304` for a
//! matching `If-None-Match`, `Vary: Accept-Language`, and `Cache-Control`. It offers
//! [`respond_index`] and [`respond_document`] for a host that writes its own handlers, and,
//! behind the `router` feature, a ready-made `router()`.
//!
//! # Header contract
//!
//! | Header | Value |
//! |---|---|
//! | `Cache-Control` | `public, max-age=300`, or what [`HttpPolicy`] sets |
//! | `Vary` | `Accept-Language`, on both routes and on the `304` |
//! | `ETag` | Strong, over the whole representation |
//! | `Content-Type` | `application/json` |
//!
//! A request whose `If-None-Match` lists the current tag, or is `*`, gets `304` with the same
//! `ETag`, `Cache-Control` and `Vary`, and no body. The comparison is weak, as RFC 9110 section
//! 13.1.2 requires, so a `W/` prefix is ignored.
//!
//! # Features
//!
//! | Feature | Effect |
//! |---|---|
//! | `router` | `router()` |
//! | `utoipa` | Forwarded: `ToSchema` and `IntoParams` on the wire types |
//! | `consent` | The `consent` module: routes, the `RequireConsent` layer and the subject resolver |
//!
//! # Design record
//!
//! **Why the host keeps its handlers.** `#[utoipa::path(tag = ...)]` takes a constant, and the
//! tag, the operation identifiers that name methods in a generated client, and the error schema
//! all belong to the host. Generic handlers behind `utoipa-axum`'s `routes!` would need a
//! turbofish that the macro does not reliably accept. The responders keep a handler to about
//! fifteen lines.
//!
//! **Why the slug is never a path.** Files are named only by configuration, so nothing a request
//! carries reaches the filesystem. A slug that is not a valid slug is answered exactly like an
//! unknown one.
//!
//! **Why the body is served as Markdown.** The server never renders a document to HTML. Rendering
//! belongs to the client, where `terrace-legal-markdown` builds elements and no HTML string.
//!
//! **Why the routes are unauthenticated.** A reader needs the terms before they have an account.
//! The host classifies both routes as public in whatever access tests it has.
//!
//! **Why `304` is not in a host's `OpenAPI` document.** A browser resolves it before any script
//! sees it, and documenting it changes a generated client. Revisit when a non-browser client
//! appears.

mod problem;
mod respond;

#[cfg(feature = "router")]
mod router;

#[cfg(feature = "consent")]
pub mod consent;

pub use problem::{LegalProblem, Problem};
pub use respond::{HttpPolicy, respond_document, respond_index};
#[cfg(feature = "router")]
pub use router::router;
