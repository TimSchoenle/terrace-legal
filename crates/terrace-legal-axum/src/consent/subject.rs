//! Identifying the caller of a consent route.

use std::future::Future;

use axum::response::Response;
use http::request::Parts;

/// Says who is calling, in terms of the host's own authentication.
///
/// An implementation usually wraps the host's auth extractor or reads what an earlier layer put
/// in the request extensions. It is created once and shared, so it holds whatever state it needs.
pub trait SubjectResolver: Send + Sync + 'static {
    /// Identifies a subject in the host's consent store.
    type Subject: Clone + Eq + Send + Sync + 'static;

    /// Returns the caller, or the response to send instead, such as `401`.
    ///
    /// The returned response is sent as it is, so the host controls its shape.
    fn resolve(
        &self,
        parts: &mut Parts,
    ) -> impl Future<Output = Result<Self::Subject, Response>> + Send;
}
