//! The consent routes and the layer that enforces them.
#![cfg(all(feature = "consent", feature = "router"))]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get as get_route, post};
use common::{config, external, get, hosted, legal, send};
use http::request::Parts;
use http::{Method, StatusCode};
use terrace_legal::consent::{ConsentStore, MemoryConsentStore};
use terrace_legal::{ConsentPolicy, Legal};
use terrace_legal_axum::consent::{
    ConsentState, Exemptions, Mounts, RequireConsent, SubjectResolver, routes,
};
use terrace_legal_axum::router;
use terrace_legal_model::consent::{Acceptance, Requirement};
use time::OffsetDateTime;
use time::macros::datetime;

const INSIDE_GRACE: OffsetDateTime = datetime!(2026-09-10 12:00 UTC);
const AFTER_GRACE: OffsetDateTime = datetime!(2026-09-16 12:00 UTC);

/// Identifies the caller by an `x-user` header, and refuses a request without one.
struct HeaderUser;

impl SubjectResolver for HeaderUser {
    type Subject = u32;

    async fn resolve(&self, parts: &mut Parts) -> Result<u32, Response> {
        parts
            .headers
            .get("x-user")
            .and_then(|value| value.to_str().ok())
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())
    }
}

type State = ConsentState<HeaderUser, Arc<MemoryConsentStore<u32>>>;

fn legal_with_consent() -> Legal {
    let mut terms = hosted(&[("en", "# Terms"), ("de", "# Bedingungen")]);
    terms.consent = ConsentPolicy {
        requirement: Requirement::Accept,
        version: Some("2026-08".into()),
        effective: Some("2026-09-01".into()),
        grace_days: 14,
    };
    let mut privacy = hosted(&[("en", "# Privacy")]);
    privacy.consent = ConsentPolicy {
        requirement: Requirement::Acknowledge,
        version: Some("3".into()),
        effective: None,
        grace_days: 0,
    };
    legal(&config(
        Some("en"),
        vec![
            ("terms", terms),
            ("privacy", privacy),
            ("imprint", external("https://example.org/impressum")),
            ("cookies", hosted(&[("en", "# Cookies")])),
        ],
    ))
}

fn state(legal: &Legal, now: OffsetDateTime) -> (State, Arc<MemoryConsentStore<u32>>) {
    let store = Arc::new(MemoryConsentStore::default());
    let state =
        ConsentState::with_clock(legal.clone(), Arc::clone(&store), HeaderUser, move || now);
    (state, store)
}

fn mounts() -> Mounts {
    Mounts {
        legal: "/v1/legal".into(),
        consent: "/v1/me/consent".into(),
        sign_out: "/v1/auth/logout".into(),
        export: "/v1/me/export".into(),
        erasure: "/v1/me/erasure".into(),
    }
}

async fn ok() -> &'static str {
    "ok"
}

/// A host-shaped application: the legal routes, the consent routes, ordinary account routes, and
/// the layer over all of it.
fn application(legal: &Legal, now: OffsetDateTime) -> (Router, Arc<MemoryConsentStore<u32>>) {
    let (state, store) = state(legal, now);
    let exemptions = Exemptions::new(mounts())
        .expect("valid")
        .extend("/v1/health")
        .expect("valid");
    let app = Router::new()
        .route("/v1/me/profile", get_route(ok))
        .route("/v1/me/export", get_route(ok))
        .route("/v1/me/erasure", delete(ok))
        .route("/v1/auth/logout", post(ok))
        .route("/v1/health", get_route(ok))
        .nest("/v1/legal", router::<Legal>().with_state(legal.clone()))
        .nest("/v1/me/consent", routes::<(), _, _>(state.clone()))
        .layer(RequireConsent::new(state, exemptions));
    (app, store)
}

fn acceptance_json(legal: &Legal, slug: &str, locale: &str) -> String {
    let served = legal
        .document(
            slug,
            &terrace_legal::Negotiation::from_request(Some(locale), None),
        )
        .expect("served");
    serde_json::to_string(&Acceptance {
        slug: slug.into(),
        version: served.value.consent.clone().expect("policy").version,
        locale: served.value.locale.clone(),
        digest: served.value.digest.clone(),
    })
    .expect("json")
}

const USER: &[(&str, &str)] = &[("x-user", "7")];

async fn post_json(app: &Router, uri: &str, body: &str) -> common::Reply {
    send(app, Method::POST, uri, USER, Some(body)).await
}

#[tokio::test]
async fn accepting_the_current_text_is_a_204_and_is_remembered() {
    let legal = legal_with_consent();
    let (app, store) = application(&legal, INSIDE_GRACE);

    let reply = post_json(
        &app,
        "/v1/me/consent",
        &acceptance_json(&legal, "terms", "de"),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    let records = store.latest(&7).await.expect("infallible");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].slug.as_str(), "terms");
    assert_eq!(records[0].locale.as_str(), "de");
    assert_eq!(records[0].at, INSIDE_GRACE);

    let status = get(&app, "/v1/me/consent", USER).await;
    assert_eq!(status.status, StatusCode::OK);
    let documents = status.json()["documents"].clone();
    let terms = documents
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["slug"] == "terms")
        .expect("terms");
    assert_eq!(terms["outstanding"], false);
    let privacy = documents
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["slug"] == "privacy")
        .expect("privacy");
    assert_eq!(privacy["outstanding"], true);
}

/// A reader could accept text they were not shown, because only the slug was checked.
#[tokio::test]
async fn a_stale_version_or_digest_is_a_409_consent_stale() {
    let legal = legal_with_consent();
    let (app, store) = application(&legal, INSIDE_GRACE);
    let good: Acceptance =
        serde_json::from_str(&acceptance_json(&legal, "terms", "en")).expect("json");

    let stale_version = Acceptance {
        version: "2026-07".into(),
        ..good.clone()
    };
    let stale_digest = Acceptance {
        digest: terrace_legal_model::Digest::from_sha256([1; 32]),
        ..good
    };
    for body in [stale_version, stale_digest] {
        let reply = post_json(
            &app,
            "/v1/me/consent",
            &serde_json::to_string(&body).expect("json"),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CONFLICT);
        assert_eq!(reply.header("content-type"), "application/problem+json");
        assert_eq!(reply.json()["type"], "consent-stale");
    }
    assert!(
        store.latest(&7).await.expect("infallible").is_empty(),
        "nothing was recorded"
    );
}

#[tokio::test]
async fn an_unknown_or_non_consentable_slug_is_a_404() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    let template: Acceptance =
        serde_json::from_str(&acceptance_json(&legal, "terms", "en")).expect("json");
    for slug in ["nope", "imprint", "cookies", "../terms"] {
        let body = serde_json::to_string(&Acceptance {
            slug: slug.into(),
            ..template.clone()
        })
        .expect("json");
        let reply = post_json(&app, "/v1/me/consent", &body).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "slug {slug}");
        assert_eq!(reply.json()["type"], "not-found");
    }
}

#[tokio::test]
async fn an_unpublished_locale_is_a_422() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    let template: Acceptance =
        serde_json::from_str(&acceptance_json(&legal, "terms", "en")).expect("json");
    let body = serde_json::to_string(&Acceptance {
        locale: "fr".parse().expect("valid"),
        ..template
    })
    .expect("json");
    let reply = post_json(&app, "/v1/me/consent", &body).await;
    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reply.json()["type"], "unpublished-locale");
}

#[tokio::test]
async fn a_malformed_body_is_a_client_error_and_records_nothing() {
    let legal = legal_with_consent();
    let (app, store) = application(&legal, INSIDE_GRACE);
    for body in [
        "",
        "{}",
        "not json",
        r#"{"slug":"terms","version":"2026-08","locale":"english","digest":"x"}"#,
    ] {
        let reply = post_json(&app, "/v1/me/consent", body).await;
        assert!(
            reply.status.is_client_error(),
            "body {body:?} gave {}",
            reply.status
        );
    }
    assert!(store.history(&7).await.expect("infallible").is_empty());
}

#[tokio::test]
async fn an_anonymous_caller_gets_what_the_resolver_says() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    let reply = get(&app, "/v1/me/consent", &[]).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = send(&app, Method::POST, "/v1/me/consent", &[], Some("{}")).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_withdrawal_makes_the_document_outstanding_again() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    post_json(
        &app,
        "/v1/me/consent",
        &acceptance_json(&legal, "terms", "en"),
    )
    .await;

    let reply = post_json(&app, "/v1/me/consent/withdraw", r#"{"slug":"terms"}"#).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    let status = get(&app, "/v1/me/consent", USER).await.json();
    let terms = status["documents"]
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["slug"] == "terms")
        .expect("terms")
        .clone();
    assert_eq!(terms["outstanding"], true);

    for slug in ["nope", "imprint", "cookies"] {
        let reply = post_json(
            &app,
            "/v1/me/consent/withdraw",
            &format!(r#"{{"slug":"{slug}"}}"#),
        )
        .await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "slug {slug}");
    }
}

#[tokio::test]
async fn one_subject_never_sees_anothers_consent() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    post_json(
        &app,
        "/v1/me/consent",
        &acceptance_json(&legal, "terms", "en"),
    )
    .await;

    let other = get(&app, "/v1/me/consent", &[("x-user", "8")]).await.json();
    let terms = other["documents"]
        .as_array()
        .expect("array")
        .iter()
        .find(|d| d["slug"] == "terms")
        .expect("terms")
        .clone();
    assert_eq!(terms["outstanding"], true);
}

#[tokio::test]
async fn the_layer_lets_a_subject_through_inside_the_grace_period() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    let reply = get(&app, "/v1/me/profile", USER).await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn the_layer_blocks_after_grace_with_the_outstanding_documents() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, AFTER_GRACE);
    let reply = get(&app, "/v1/me/profile", USER).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(reply.header("content-type"), "application/problem+json");
    let body = reply.json();
    assert_eq!(body["type"], "consent-required");
    let slugs: Vec<&str> = body["outstanding"]
        .as_array()
        .expect("array")
        .iter()
        .map(|d| d["slug"].as_str().expect("slug"))
        .collect();
    assert_eq!(
        slugs,
        ["privacy", "terms"],
        "everything outstanding, blocking or not"
    );
}

#[tokio::test]
async fn accepting_lifts_the_block() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, AFTER_GRACE);
    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::FORBIDDEN
    );

    let reply = post_json(
        &app,
        "/v1/me/consent",
        &acceptance_json(&legal, "terms", "en"),
    )
    .await;
    assert_eq!(
        reply.status,
        StatusCode::NO_CONTENT,
        "the consent route is exempt"
    );
    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::OK
    );
}

/// Access to one's data, its deletion, signing out, reading the terms and accepting them must
/// never depend on having accepted the terms.
#[tokio::test]
async fn the_layer_never_blocks_the_default_exemption_set() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, AFTER_GRACE);
    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::FORBIDDEN
    );

    for (method, uri) in [
        (Method::GET, "/v1/legal"),
        (Method::GET, "/v1/legal/terms"),
        (Method::GET, "/v1/me/consent"),
        (Method::POST, "/v1/auth/logout"),
        (Method::GET, "/v1/me/export"),
        (Method::DELETE, "/v1/me/erasure"),
        (Method::GET, "/v1/health"),
    ] {
        let reply = send(&app, method.clone(), uri, USER, None).await;
        assert_eq!(reply.status, StatusCode::OK, "{method} {uri}");
    }
}

#[tokio::test]
async fn an_exempt_route_does_not_even_consult_the_store() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, AFTER_GRACE);
    // No `x-user` header: the resolver would answer 401 if it were asked.
    assert_eq!(get(&app, "/v1/legal", &[]).await.status, StatusCode::OK);
    assert_eq!(
        get(&app, "/v1/me/profile", &[]).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_dot_segment_cannot_borrow_an_exemption() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, AFTER_GRACE);
    for uri in ["/v1/legal/../me/profile", "/v1/legal/%2e%2e/me/profile"] {
        let reply = get(&app, uri, USER).await;
        assert_ne!(reply.status, StatusCode::OK, "{uri}");
    }
}

#[tokio::test]
async fn an_acknowledge_only_requirement_never_blocks() {
    let mut privacy = hosted(&[("en", "# Privacy")]);
    privacy.consent = ConsentPolicy {
        requirement: Requirement::Acknowledge,
        version: Some("3".into()),
        ..ConsentPolicy::default()
    };
    let legal = legal(&config(None, vec![("privacy", privacy)]));
    let (app, _) = application(&legal, AFTER_GRACE);
    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_new_revision_blocks_again_after_its_grace_period() {
    let legal = legal_with_consent();
    let (app, _) = application(&legal, INSIDE_GRACE);
    post_json(
        &app,
        "/v1/me/consent",
        &acceptance_json(&legal, "terms", "en"),
    )
    .await;
    // The clock is fixed, so move time instead of the state: a fresh application after the grace
    // window shares the catalog and the subject's records only through the store, which is new.
    let mut revised = terrace_legal::LegalConfig::default();
    let mut terms = hosted(&[("en", "# Terms v2")]);
    terms.consent = ConsentPolicy {
        requirement: Requirement::Accept,
        version: Some("2026-09".into()),
        effective: Some("2026-09-01".into()),
        grace_days: 0,
    };
    revised.documents.insert("terms".into(), terms);
    legal.replace(&revised).expect("valid");

    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::FORBIDDEN,
        "the accepted version is no longer the current one"
    );
}

#[tokio::test]
async fn the_layer_works_inside_a_nested_router_where_the_path_is_stripped() {
    let legal = legal_with_consent();
    let (state, _) = state(&legal, AFTER_GRACE);
    let exemptions = Exemptions::new(mounts()).expect("valid");
    let inner = Router::new()
        .route("/export", get_route(ok))
        .route("/profile", get_route(ok))
        .layer(RequireConsent::new(state, exemptions));
    let app = Router::new().nest("/v1/me", inner);

    assert_eq!(
        get(&app, "/v1/me/export", USER).await.status,
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/v1/me/profile", USER).await.status,
        StatusCode::FORBIDDEN
    );
}
