//! Helpers shared by the HTTP tests.
#![allow(
    dead_code,
    reason = "each test binary uses a different subset of these helpers"
)]

use axum::Router;
use axum::body::Body;
use http::{HeaderMap, Method, Request, StatusCode};
use http_body_util::BodyExt as _;
use terrace_legal::{Catalog, Legal, LegalConfig, LegalDocument};
use tower::ServiceExt as _;

pub(crate) fn hosted(bodies: &[(&str, &str)]) -> LegalDocument {
    LegalDocument {
        body: bodies
            .iter()
            .map(|(locale, text)| ((*locale).to_owned(), (*text).to_owned()))
            .collect(),
        ..LegalDocument::default()
    }
}

pub(crate) fn external(url: &str) -> LegalDocument {
    LegalDocument {
        url: Some(url.to_owned()),
        ..LegalDocument::default()
    }
}

pub(crate) fn titled(mut document: LegalDocument, titles: &[(&str, &str)]) -> LegalDocument {
    for (locale, title) in titles {
        document
            .title
            .insert((*locale).to_owned(), (*title).to_owned());
    }
    document
}

pub(crate) fn config(
    default_locale: Option<&str>,
    documents: Vec<(&str, LegalDocument)>,
) -> LegalConfig {
    LegalConfig {
        default_locale: default_locale.map(str::to_owned),
        documents: documents
            .into_iter()
            .map(|(slug, document)| (slug.to_owned(), document))
            .collect(),
    }
}

pub(crate) fn legal(config: &LegalConfig) -> Legal {
    Legal::new(Catalog::build(config).expect("a valid configuration"))
}

/// One response, read to the end.
pub(crate) struct Reply {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    pub(crate) fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .unwrap_or_else(|| panic!("missing header {name}: {:?}", self.headers))
            .to_str()
            .expect("visible ascii")
    }

    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("not json ({error}): {:?}", self.body))
    }
}

/// Sends one request through `app` and reads the whole response.
pub(crate) async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Reply {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let request = builder
        .body(body.map_or_else(Body::empty, |text| Body::from(text.to_owned())))
        .expect("a valid request");
    let response = app.clone().oneshot(request).await.expect("infallible");
    let (parts, body) = response.into_parts();
    Reply {
        status: parts.status,
        headers: parts.headers,
        body: body.collect().await.expect("body").to_bytes().to_vec(),
    }
}

pub(crate) async fn get(app: &Router, uri: &str, headers: &[(&str, &str)]) -> Reply {
    send(app, Method::GET, uri, headers, None).await
}
