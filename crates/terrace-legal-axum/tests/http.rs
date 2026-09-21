//! The header contract and error behaviour of the two legal routes, without a network.
#![cfg(feature = "router")]

mod common;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get as get_route;
use common::{config, external, get, hosted, legal, titled};
use http::{HeaderMap, StatusCode};
use terrace_legal::Legal;
use terrace_legal_axum::{HttpPolicy, LegalProblem, respond_document, respond_index, router};
use terrace_legal_model::LegalParams;

fn documents() -> terrace_legal::LegalConfig {
    config(
        Some("en"),
        vec![
            (
                "terms",
                titled(
                    hosted(&[("en", "# Terms"), ("de", "# Bedingungen")]),
                    &[("en", "Terms"), ("de", "Bedingungen")],
                ),
            ),
            ("imprint", external("https://example.org/impressum")),
        ],
    )
}

fn app(legal: &Legal) -> Router {
    Router::new()
        .nest("/v1/legal", router())
        .with_state(legal.clone())
}

fn standard() -> (Router, Legal) {
    let legal = legal(&documents());
    (app(&legal), legal)
}

#[tokio::test]
async fn both_routes_carry_the_header_contract() {
    let (app, _) = standard();
    for uri in ["/v1/legal", "/v1/legal/terms"] {
        let reply = get(&app, uri, &[]).await;
        assert_eq!(reply.status, StatusCode::OK, "{uri}");
        assert_eq!(reply.header("content-type"), "application/json");
        assert_eq!(reply.header("cache-control"), "public, max-age=300");
        assert_eq!(reply.header("vary"), "Accept-Language");
        let etag = reply.header("etag");
        assert!(
            etag.starts_with('"') && etag.ends_with('"') && !etag.starts_with("W/"),
            "{etag}"
        );
    }
}

/// The response was `Cache-Control: public` and chose its body from `Accept-Language` when `lang`
/// was absent, without `Vary`, so a shared cache served one reader's German document to the next
/// English reader.
#[tokio::test]
async fn the_response_varies_on_accept_language_with_and_without_lang() {
    let (app, _) = standard();
    for uri in [
        "/v1/legal",
        "/v1/legal?lang=de",
        "/v1/legal/terms",
        "/v1/legal/terms?lang=de",
    ] {
        assert_eq!(
            get(&app, uri, &[]).await.header("vary"),
            "Accept-Language",
            "{uri}"
        );
    }
}

#[tokio::test]
async fn different_languages_get_different_tags_so_a_cache_can_tell_them_apart() {
    let (app, _) = standard();
    let en = get(&app, "/v1/legal/terms", &[("accept-language", "en")]).await;
    let de = get(&app, "/v1/legal/terms", &[("accept-language", "de")]).await;
    assert_ne!(en.header("etag"), de.header("etag"));
    assert_eq!(en.json()["body"], "# Terms");
    assert_eq!(de.json()["body"], "# Bedingungen");
}

/// The `ETag` was sent but `If-None-Match` was never honoured, so a conditional request cost a
/// full `200` although the code documented that it cost a `stat`.
#[tokio::test]
async fn a_matching_if_none_match_gets_a_304_without_a_body() {
    let (app, _) = standard();
    for uri in ["/v1/legal", "/v1/legal/terms"] {
        let tag = get(&app, uri, &[]).await.header("etag").to_owned();
        for header in [
            tag.clone(),
            format!("W/{tag}"),
            format!("\"stale\", {tag}"),
            format!("W/\"stale\" , W/{tag}"),
            "*".to_owned(),
        ] {
            let reply = get(&app, uri, &[("if-none-match", &header)]).await;
            assert_eq!(
                reply.status,
                StatusCode::NOT_MODIFIED,
                "{uri} with {header}"
            );
            assert!(reply.body.is_empty());
            assert_eq!(reply.header("etag"), tag);
            assert_eq!(reply.header("cache-control"), "public, max-age=300");
            assert_eq!(reply.header("vary"), "Accept-Language");
            assert!(
                reply.headers.get("content-type").is_none(),
                "a 304 has no representation"
            );
        }
    }
}

#[tokio::test]
async fn a_stale_or_malformed_if_none_match_gets_the_full_response() {
    let (app, _) = standard();
    for uri in ["/v1/legal", "/v1/legal/terms"] {
        for header in ["\"stale\"", "W/\"stale\"", "garbage", "", "\"unterminated"] {
            let reply = get(&app, uri, &[("if-none-match", header)]).await;
            assert_eq!(reply.status, StatusCode::OK, "{uri} with {header:?}");
            assert!(!reply.body.is_empty());
        }
    }
}

#[tokio::test]
async fn a_tag_from_another_language_does_not_revalidate() {
    let (app, _) = standard();
    let english = get(&app, "/v1/legal/terms?lang=en", &[]).await;
    let reply = get(
        &app,
        "/v1/legal/terms?lang=de",
        &[("if-none-match", english.header("etag"))],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
}

/// The tag covered the body only. A configuration reload that changed a title would have been
/// answered with `304`, and the client would have kept the old title.
#[tokio::test]
async fn a_reload_that_changes_only_a_title_invalidates_the_client_copy() {
    let (app, legal) = standard();
    let before = get(&app, "/v1/legal/terms", &[]).await;

    let mut edited = documents();
    edited
        .documents
        .get_mut("terms")
        .expect("present")
        .title
        .insert("en".into(), "Terms of Service".into());
    legal.replace(&edited).expect("valid");

    let after = get(
        &app,
        "/v1/legal/terms",
        &[("if-none-match", before.header("etag"))],
    )
    .await;
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(after.json()["title"], "Terms of Service");

    let index = get(&app, "/v1/legal", &[]).await;
    let stale_index = get(
        &app,
        "/v1/legal",
        &[("if-none-match", index.header("etag"))],
    )
    .await;
    assert_eq!(stale_index.status, StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn a_configuration_swap_invalidates_a_cached_index() {
    let (app, legal) = standard();
    let before = get(&app, "/v1/legal", &[]).await;
    legal.replace(&documents()).expect("valid");
    let after = get(
        &app,
        "/v1/legal",
        &[("if-none-match", before.header("etag"))],
    )
    .await;
    assert_eq!(
        after.status,
        StatusCode::OK,
        "the generation moved even though the content did not"
    );
}

#[tokio::test]
async fn the_index_lists_documents_in_catalog_order_with_negotiated_titles() {
    let (app, _) = standard();
    let reply = get(&app, "/v1/legal", &[("accept-language", "de")]).await;
    assert_eq!(
        reply.json(),
        serde_json::json!([
            {
                "slug": "imprint", "title": null, "updated": null, "kind": "external",
                "url": "https://example.org/impressum", "locales": []
            },
            {
                "slug": "terms", "title": "Bedingungen", "updated": null, "kind": "inline",
                "url": null, "locales": ["de", "en"]
            }
        ])
    );
}

#[tokio::test]
async fn the_lang_parameter_beats_the_header_and_garbage_is_ignored() {
    let (app, _) = standard();
    let body = |reply: common::Reply| reply.json()["body"].as_str().expect("text").to_owned();
    assert_eq!(
        body(
            get(
                &app,
                "/v1/legal/terms?lang=de",
                &[("accept-language", "en")]
            )
            .await
        ),
        "# Bedingungen"
    );
    assert_eq!(
        body(
            get(
                &app,
                "/v1/legal/terms?lang=not-a-locale",
                &[("accept-language", "de")]
            )
            .await
        ),
        "# Bedingungen"
    );
    assert_eq!(body(get(&app, "/v1/legal/terms", &[]).await), "# Terms");
}

#[tokio::test]
async fn several_accept_language_lines_are_one_list() {
    let app = {
        let legal = legal(&config(
            None,
            vec![(
                "terms",
                hosted(&[
                    ("en", "# Terms"),
                    ("de", "# Bedingungen"),
                    ("fr", "# Conditions"),
                ]),
            )],
        ));
        app(&legal)
    };
    let reply = common::send(
        &app,
        http::Method::GET,
        "/v1/legal/terms",
        &[
            ("accept-language", "ja;q=0.9"),
            ("accept-language", "fr;q=0.8, de;q=0.1"),
        ],
        None,
    )
    .await;
    assert_eq!(reply.json()["body"], "# Conditions");
}

#[tokio::test]
async fn a_binary_accept_language_header_does_not_fail_the_request() {
    let (app, _) = standard();
    let request = http::Request::builder()
        .uri("/v1/legal/terms")
        .header(
            "accept-language",
            http::HeaderValue::from_bytes(b"\xff\xfe, de").expect("obs-text"),
        )
        .body(axum::body::Body::empty())
        .expect("request");
    let response = tower::ServiceExt::oneshot(app, request)
        .await
        .expect("infallible");
    assert_eq!(response.status(), StatusCode::OK);
}

/// The slug is a map key and never a path: nothing the request carries reaches the filesystem.
#[tokio::test]
async fn a_path_traversal_slug_is_a_404_problem() {
    let (app, _) = standard();
    for uri in [
        "/v1/legal/..%2F..%2Fetc%2Fpasswd",
        "/v1/legal/%2e%2e%2fetc%2fpasswd",
        "/v1/legal/terms%00",
        "/v1/legal/privacy",
        "/v1/legal/TERMS",
    ] {
        let reply = get(&app, uri, &[]).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(reply.header("content-type"), "application/problem+json");
        assert_eq!(reply.json()["type"], "not-found");
    }
}

#[tokio::test]
async fn a_request_for_the_body_of_an_external_document_is_a_404_problem() {
    let (app, _) = standard();
    let reply = get(&app, "/v1/legal/imprint", &[]).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.header("content-type"), "application/problem+json");
    assert_eq!(reply.json()["type"], "external-document");
}

#[tokio::test]
async fn a_legal_error_renders_as_a_problem() {
    let response: Response = LegalProblem(terrace_legal::LegalError::UnknownSlug).into_response();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
}

#[tokio::test]
async fn an_empty_catalog_serves_an_empty_index_and_no_document() {
    let legal = legal(&config(None, vec![]));
    let app = app(&legal);
    let index = get(&app, "/v1/legal", &[]).await;
    assert_eq!(index.status, StatusCode::OK);
    assert_eq!(index.json(), serde_json::json!([]));
    assert_eq!(
        get(&app, "/v1/legal/terms", &[]).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn the_cache_policy_is_configurable_and_never_invalid() {
    assert_eq!(HttpPolicy::default().cache_control(), "public, max-age=300");
    assert_eq!(HttpPolicy::public(60).cache_control(), "public, max-age=60");
    assert_eq!(HttpPolicy::public(0).cache_control(), "public, max-age=0");
    assert!(HttpPolicy::with_cache_control("no-store").is_some());
    assert!(HttpPolicy::with_cache_control("bad\nvalue").is_none());
}

/// A host with an `OpenAPI` document keeps its own annotated handlers and calls the responders.
#[tokio::test]
async fn a_host_handler_can_wrap_the_responders() {
    async fn index(
        State(legal): State<Legal>,
        Query(params): Query<LegalParams>,
        headers: HeaderMap,
    ) -> Response {
        HttpPolicy::public(10)
            .respond_index(&legal, &headers, params.lang)
            .await
    }
    async fn document(
        State(legal): State<Legal>,
        Path(slug): Path<String>,
        Query(params): Query<LegalParams>,
        headers: HeaderMap,
    ) -> Result<Response, LegalProblem> {
        Ok(respond_document(&legal, &headers, slug, params.lang).await?)
    }
    async fn default_index(State(legal): State<Legal>, headers: HeaderMap) -> Response {
        respond_index(&legal, &headers, None).await
    }
    let legal = legal(&documents());
    let app = Router::new()
        .route("/custom", get_route(index))
        .route("/plain", get_route(default_index))
        .route("/custom/{slug}", get_route(document))
        .with_state(legal);

    assert_eq!(
        get(&app, "/custom", &[]).await.header("cache-control"),
        "public, max-age=10"
    );
    assert_eq!(
        get(&app, "/plain", &[]).await.header("cache-control"),
        "public, max-age=300"
    );
    assert_eq!(get(&app, "/custom/terms", &[]).await.status, StatusCode::OK);
    assert_eq!(
        get(&app, "/custom/nope", &[]).await.status,
        StatusCode::NOT_FOUND
    );
}
