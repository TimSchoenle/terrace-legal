//! Conditional, cache-aware JSON responses for the two legal routes.

use axum::body::Body;
use axum::response::Response;
use http::StatusCode;
use http::header::{
    ACCEPT_LANGUAGE, CACHE_CONTROL, CONTENT_TYPE, ETAG, HeaderMap, HeaderValue, IF_NONE_MATCH, VARY,
};
use serde::Serialize;
use terrace_legal::{ETag, Legal, LegalError, Negotiation};

use crate::problem::Problem;

/// The caching headers of the legal routes.
///
/// The documents are public and change a few times a year, so a shared cache may keep them for
/// five minutes by default. The bound is what limits how long a cache serves a retired text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpPolicy {
    cache_control: HeaderValue,
}

impl HttpPolicy {
    /// The default `Cache-Control`: `public, max-age=300`.
    pub const DEFAULT_MAX_AGE_SECS: u32 = 300;

    /// Allows shared caches to keep a response for `max_age_secs` seconds.
    #[must_use]
    pub fn public(max_age_secs: u32) -> Self {
        Self::with_cache_control(&format!("public, max-age={max_age_secs}")).unwrap_or_default()
    }

    /// Uses `value` as the `Cache-Control` header verbatim.
    ///
    /// Returns `None` when `value` is not a valid header value.
    #[must_use]
    pub fn with_cache_control(value: &str) -> Option<Self> {
        HeaderValue::from_str(value)
            .ok()
            .map(|cache_control| Self { cache_control })
    }

    /// Returns the `Cache-Control` value.
    #[must_use]
    pub fn cache_control(&self) -> &HeaderValue {
        &self.cache_control
    }

    /// Answers a request for the index.
    ///
    /// The response is `200` with a JSON array, or `304` with no body when `If-None-Match` names
    /// the current tag. Both carry `ETag`, `Cache-Control` and `Vary: Accept-Language`.
    pub async fn respond_index(
        &self,
        legal: &Legal,
        headers: &HeaderMap,
        lang: Option<String>,
    ) -> Response {
        let negotiation = negotiation(headers, lang.as_deref());
        let tagged = legal.index(&negotiation);
        self.conditional(headers, &tagged.etag, &tagged.value)
    }

    /// Answers a request for one document.
    ///
    /// # Errors
    ///
    /// [`LegalError::UnknownSlug`] when nothing is published under `slug`, and
    /// [`LegalError::External`] when the document is hosted elsewhere.
    pub async fn respond_document(
        &self,
        legal: &Legal,
        headers: &HeaderMap,
        slug: String,
        lang: Option<String>,
    ) -> Result<Response, LegalError> {
        let negotiation = negotiation(headers, lang.as_deref());
        let tagged = legal.document(&slug, &negotiation)?;
        Ok(self.conditional(headers, &tagged.etag, &*tagged.value))
    }

    fn conditional<T: Serialize + ?Sized>(
        &self,
        request: &HeaderMap,
        etag: &ETag,
        value: &T,
    ) -> Response {
        let unchanged = request
            .get_all(IF_NONE_MATCH)
            .iter()
            .filter_map(|header| header.to_str().ok())
            .any(|header| etag.matches_if_none_match(header));

        let (status, body) = if unchanged {
            (StatusCode::NOT_MODIFIED, Body::empty())
        } else {
            match serde_json::to_vec(value) {
                Ok(bytes) => (StatusCode::OK, Body::from(bytes)),
                Err(_) => return axum::response::IntoResponse::into_response(Problem::internal()),
            }
        };

        let mut response = Response::new(body);
        *response.status_mut() = status;
        let headers = response.headers_mut();
        if let Ok(value) = HeaderValue::from_str(etag.as_str()) {
            headers.insert(ETAG, value);
        }
        headers.insert(CACHE_CONTROL, self.cache_control.clone());
        // The body and the title are chosen from `Accept-Language` when `lang` is absent, so a
        // shared cache has to key on it.
        headers.insert(VARY, HeaderValue::from_static("Accept-Language"));
        if status == StatusCode::OK {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        response
    }
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            cache_control: HeaderValue::from_static("public, max-age=300"),
        }
    }
}

fn negotiation(headers: &HeaderMap, lang: Option<&str>) -> Negotiation {
    // Several `Accept-Language` lines are one list, joined by commas.
    let mut joined: Vec<u8> = Vec::new();
    for value in headers.get_all(ACCEPT_LANGUAGE) {
        if !joined.is_empty() {
            joined.push(b',');
        }
        joined.extend_from_slice(value.as_bytes());
    }
    Negotiation::from_request(lang, (!joined.is_empty()).then_some(joined.as_slice()))
}

/// Answers a request for the index with the default [`HttpPolicy`].
pub async fn respond_index(legal: &Legal, headers: &HeaderMap, lang: Option<String>) -> Response {
    HttpPolicy::default()
        .respond_index(legal, headers, lang)
        .await
}

/// Answers a request for one document with the default [`HttpPolicy`].
///
/// # Errors
///
/// [`LegalError::UnknownSlug`] when nothing is published under `slug`, and
/// [`LegalError::External`] when the document is hosted elsewhere.
pub async fn respond_document(
    legal: &Legal,
    headers: &HeaderMap,
    slug: String,
    lang: Option<String>,
) -> Result<Response, LegalError> {
    HttpPolicy::default()
        .respond_document(legal, headers, slug, lang)
        .await
}
