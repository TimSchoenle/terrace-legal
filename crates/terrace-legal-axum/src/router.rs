//! A ready-made router for hosts that publish no `OpenAPI` document.

use axum::Router;
use axum::extract::{FromRef, Path, Query, State};
use axum::response::Response;
use axum::routing::get;
use http::HeaderMap;
use terrace_legal::Legal;
use terrace_legal_model::LegalParams;

use crate::problem::LegalProblem;
use crate::respond::{respond_document, respond_index};

/// Serves the index at `/` and one document at `/{slug}`.
///
/// Mount it under a prefix of the host's choosing with [`Router::nest`]. Both routes are public:
/// registering is the act of accepting the terms, so a reader needs them before they have an
/// account. A host has to classify them that way in its access rules.
///
/// A host that publishes an `OpenAPI` document writes two annotated handlers around
/// [`respond_index`] and [`respond_document`] instead, because an operation's tag and identifier
/// are compile-time constants that a generic handler cannot carry.
pub fn router<S>() -> Router<S>
where
    Legal: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(index))
        .route("/{slug}", get(document))
}

async fn index(
    State(legal): State<Legal>,
    Query(params): Query<LegalParams>,
    headers: HeaderMap,
) -> Response {
    respond_index(&legal, &headers, params.lang).await
}

async fn document(
    State(legal): State<Legal>,
    Path(slug): Path<String>,
    Query(params): Query<LegalParams>,
    headers: HeaderMap,
) -> Result<Response, LegalProblem> {
    Ok(respond_document(&legal, &headers, slug, params.lang).await?)
}
