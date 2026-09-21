//! Minimal `application/problem+json` responses.

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use http::header::{CONTENT_TYPE, HeaderValue};
use serde_json::{Map, Value};
use terrace_legal::LegalError;

/// An `application/problem+json` body as RFC 9457 describes.
///
/// Every problem this crate returns has a stable `type` token that a client can switch on, and a
/// `title` and `status` for a person reading the response. The default responses add a `detail`,
/// the fourth member a host's own problem type carries.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    status: StatusCode,
    kind: &'static str,
    title: &'static str,
    extra: Map<String, Value>,
}

impl Problem {
    /// Creates a problem. `kind` is the `type` member, a short token such as `not-found`.
    #[must_use]
    pub fn new(status: StatusCode, kind: &'static str, title: &'static str) -> Self {
        Self {
            status,
            kind,
            title,
            extra: Map::new(),
        }
    }

    /// Adds an extension member. A member named `type`, `title` or `status` is ignored, because
    /// it would contradict the standard members.
    #[must_use]
    pub fn with(mut self, name: &str, value: impl Into<Value>) -> Self {
        if !matches!(name, "type" | "title" | "status") {
            self.extra.insert(name.to_owned(), value.into());
        }
        self
    }

    /// Returns the HTTP status.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the `type` token.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// A `404` for a document that is not published, or that has no body to return.
    #[must_use]
    pub fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not-found", "Not Found")
    }

    /// A `500` that says nothing about the cause, which is the host's to log.
    #[must_use]
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Internal Server Error",
        )
    }
}

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let mut body = Map::new();
        body.insert("type".into(), self.kind.into());
        body.insert("title".into(), self.title.into());
        body.insert("status".into(), self.status.as_u16().into());
        body.extend(self.extra);

        let bytes = serde_json::to_vec(&Value::Object(body)).unwrap_or_default();
        let mut response = Response::new(Body::from(bytes));
        *response.status_mut() = self.status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

/// A [`LegalError`] that renders as a [`Problem`].
///
/// `LegalError` belongs to another crate, so it cannot implement `IntoResponse` here. A host with
/// its own error type converts the [`LegalError`] instead. The response is a `404` in both cases:
/// an unknown slug has nothing to serve, and an external document has no body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProblem(pub LegalError);

impl From<LegalError> for LegalProblem {
    fn from(error: LegalError) -> Self {
        Self(error)
    }
}

impl IntoResponse for LegalProblem {
    fn into_response(self) -> Response {
        match self.0 {
            LegalError::UnknownSlug => Problem::not_found().with("detail", "no such document"),
            LegalError::External { .. } => Problem::new(
                StatusCode::NOT_FOUND,
                "external-document",
                "Document Is Hosted Elsewhere",
            )
            .with(
                "detail",
                "the document is hosted elsewhere and has no body here",
            ),
        }
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt as _;

    use super::*;

    async fn read(response: Response) -> (StatusCode, String, Value) {
        let status = response.status();
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (
            status,
            content_type,
            serde_json::from_slice(&bytes).expect("json"),
        )
    }

    #[tokio::test]
    async fn a_problem_is_problem_json_with_the_standard_members() {
        let (status, content_type, body) =
            read(Problem::new(StatusCode::CONFLICT, "consent-stale", "Stale").into_response())
                .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(content_type, "application/problem+json");
        assert_eq!(
            body,
            serde_json::json!({"type": "consent-stale", "title": "Stale", "status": 409})
        );
    }

    #[tokio::test]
    async fn an_extension_cannot_overwrite_a_standard_member() {
        let problem = Problem::not_found()
            .with("status", 200)
            .with("type", "x")
            .with("outstanding", vec!["terms"]);
        let (_, _, body) = read(problem.into_response()).await;
        assert_eq!(body["status"], 404);
        assert_eq!(body["type"], "not-found");
        assert_eq!(body["outstanding"], serde_json::json!(["terms"]));
    }

    #[tokio::test]
    async fn a_legal_error_is_a_not_found_problem() {
        let (status, content_type, body) =
            read(LegalProblem(LegalError::UnknownSlug).into_response()).await;
        assert_eq!(
            (status, content_type.as_str()),
            (StatusCode::NOT_FOUND, "application/problem+json")
        );
        assert_eq!(body["type"], "not-found");

        let external = LegalError::External {
            url: "https://example.org/".parse().expect("url"),
        };
        let (status, _, body) = read(LegalProblem::from(external).into_response()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["type"], "external-document");
        assert!(
            body.get("url").is_none(),
            "the problem does not repeat the location"
        );
    }
}
