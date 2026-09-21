//! The consent routes.

use axum::extract::{FromRequestParts, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use http::StatusCode;
use http::request::Parts;
use terrace_legal::consent::{AcceptanceError, ConsentStore};
use terrace_legal_model::consent::{Acceptance, Channel, Withdrawal};

use super::state::ConsentState;
use super::subject::SubjectResolver;
use crate::problem::Problem;

/// Serves `GET /`, `POST /` and `POST /withdraw`.
///
/// Mount it with [`Router::nest`] under a prefix that the host's authentication already covers.
/// The prefix has to be in the [`Mounts`](super::Mounts) given to the layer, or the layer would
/// block a subject from accepting the terms it is asking them to accept.
pub fn routes<S, R, St>(state: ConsentState<R, St>) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    Router::new()
        .route("/", get(status::<R, St>).post(accept::<R, St>))
        .route("/withdraw", post(withdraw::<R, St>))
        .with_state(state)
}

/// The caller, resolved by the host's [`SubjectResolver`].
struct Caller<T>(T);

impl<R, St> FromRequestParts<ConsentState<R, St>> for Caller<R::Subject>
where
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ConsentState<R, St>,
    ) -> Result<Self, Self::Rejection> {
        state.resolver.resolve(parts).await.map(Self)
    }
}

async fn status<R, St>(
    State(state): State<ConsentState<R, St>>,
    Caller(subject): Caller<R::Subject>,
) -> Response
where
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    match state.store.latest(&subject).await {
        Ok(latest) => Json(state.legal.consent_status(&latest, state.clock.now())).into_response(),
        Err(_) => Problem::internal().into_response(),
    }
}

async fn accept<R, St>(
    State(state): State<ConsentState<R, St>>,
    Caller(subject): Caller<R::Subject>,
    Json(acceptance): Json<Acceptance>,
) -> Response
where
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    let verified = match state.legal.verify_acceptance(&acceptance) {
        Ok(verified) => verified,
        Err(error) => return refusal(error).into_response(),
    };
    let record = verified.into_record(subject, Channel::Api, state.clock.now());
    match state.store.append(std::slice::from_ref(&record)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => Problem::internal().into_response(),
    }
}

async fn withdraw<R, St>(
    State(state): State<ConsentState<R, St>>,
    Caller(subject): Caller<R::Subject>,
    Json(withdrawal): Json<Withdrawal>,
) -> Response
where
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    let Ok(latest) = state.store.latest(&subject).await else {
        return Problem::internal().into_response();
    };
    let record =
        match state
            .legal
            .withdrawal_record(&withdrawal, &subject, &latest, state.clock.now())
        {
            Ok(record) => record,
            Err(error) => return refusal(error).into_response(),
        };
    match state.store.append(std::slice::from_ref(&record)).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(_) => Problem::internal().into_response(),
    }
}

/// Maps a refused acceptance to its problem.
fn refusal(error: AcceptanceError) -> Problem {
    match error {
        AcceptanceError::UnknownDocument => Problem::not_found(),
        AcceptanceError::UnpublishedLocale => Problem::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unpublished-locale",
            "Locale Is Not Published",
        ),
        AcceptanceError::Stale => Problem::new(
            StatusCode::CONFLICT,
            "consent-stale",
            "The Document Changed Since It Was Shown",
        ),
    }
}
