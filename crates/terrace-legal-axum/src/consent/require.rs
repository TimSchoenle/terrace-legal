//! The layer that blocks a subject who owes an acceptance.

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::OriginalUri;
use axum::response::{IntoResponse, Response};
use http::{Request, StatusCode};
use terrace_legal::consent::ConsentStore;
use tower::{Layer, Service};

use super::exempt::Exemptions;
use super::state::ConsentState;
use super::subject::SubjectResolver;
use crate::problem::Problem;

/// Answers `403` `consent-required` when the caller owes a blocking acceptance.
///
/// The response carries the outstanding documents in an `outstanding` member. A request whose path
/// is covered by the [`Exemptions`] passes untouched and without a store lookup. The layer never
/// blocks on an outstanding acknowledgement, or on an acceptance still inside its grace period.
///
/// If the resolver cannot identify the caller, its response is returned as it is, so mount the
/// layer where the host's own authentication already ran or is part of the resolver.
pub struct RequireConsent<R, St> {
    state: ConsentState<R, St>,
    exemptions: Arc<Exemptions>,
}

impl<R, St> RequireConsent<R, St> {
    /// Blocks according to `state`, except for the paths `exemptions` covers.
    #[must_use]
    pub fn new(state: ConsentState<R, St>, exemptions: Exemptions) -> Self {
        Self {
            state,
            exemptions: Arc::new(exemptions),
        }
    }
}

impl<R, St> Clone for RequireConsent<R, St> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            exemptions: Arc::clone(&self.exemptions),
        }
    }
}

impl<Inner, R, St> Layer<Inner> for RequireConsent<R, St> {
    type Service = RequireConsentService<Inner, R, St>;

    fn layer(&self, inner: Inner) -> Self::Service {
        RequireConsentService {
            inner,
            layer: self.clone(),
        }
    }
}

/// The service that [`RequireConsent`] wraps around a route.
pub struct RequireConsentService<Inner, R, St> {
    inner: Inner,
    layer: RequireConsent<R, St>,
}

impl<Inner: Clone, R, St> Clone for RequireConsentService<Inner, R, St> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            layer: self.layer.clone(),
        }
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

impl<Inner, R, St> Service<Request<Body>> for RequireConsentService<Inner, R, St>
where
    Inner: Service<Request<Body>, Response = Response, Error = Infallible> + Clone + Send + 'static,
    Inner::Future: Send + 'static,
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = BoxFuture<Result<Response, Infallible>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        // The clone is the one that was polled ready, so the ready service is the one called.
        let ready = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, ready);
        let layer = self.layer.clone();

        Box::pin(async move {
            // A route nested under a prefix sees the path with the prefix stripped, and the
            // original URI in an extension. The exemptions are written against the full path.
            let path = request
                .extensions()
                .get::<OriginalUri>()
                .map_or_else(|| request.uri().path(), |original| original.0.path())
                .to_owned();
            if layer.exemptions.covers(&path) {
                return inner.call(request).await;
            }

            let (mut parts, body) = request.into_parts();
            let state = &layer.state;
            let subject = match state.resolver.resolve(&mut parts).await {
                Ok(subject) => subject,
                Err(response) => return Ok(response),
            };
            let Ok(latest) = state.store.latest(&subject).await else {
                return Ok(Problem::internal().into_response());
            };
            let status = state.legal.consent_status(&latest, state.clock.now());
            if status.is_blocked() {
                let outstanding: Vec<_> = status.outstanding().collect();
                return Ok(Problem::new(
                    StatusCode::FORBIDDEN,
                    "consent-required",
                    "Consent Is Required",
                )
                .with(
                    "outstanding",
                    serde_json::to_value(&outstanding).unwrap_or_default(),
                )
                .into_response());
            }
            inner.call(Request::from_parts(parts, body)).await
        })
    }
}
