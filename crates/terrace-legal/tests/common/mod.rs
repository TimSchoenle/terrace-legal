//! Builders shared by the integration tests.
#![allow(
    dead_code,
    reason = "each test binary uses a different subset of these helpers"
)]

use terrace_legal::{Catalog, ConfigIssues, LegalConfig, LegalDocument};

pub(crate) mod json_schema;

/// A hosted document with one body per `(locale key, text)` pair.
pub(crate) fn hosted(bodies: &[(&str, &str)]) -> LegalDocument {
    LegalDocument {
        body: bodies
            .iter()
            .map(|(locale, text)| ((*locale).to_owned(), (*text).to_owned()))
            .collect(),
        ..LegalDocument::default()
    }
}

/// An external document at `url`.
pub(crate) fn external(url: &str) -> LegalDocument {
    LegalDocument {
        url: Some(url.to_owned()),
        ..LegalDocument::default()
    }
}

/// Adds titles, given as `(locale key, title)` pairs.
pub(crate) fn titled(mut document: LegalDocument, titles: &[(&str, &str)]) -> LegalDocument {
    for (locale, title) in titles {
        document
            .title
            .insert((*locale).to_owned(), (*title).to_owned());
    }
    document
}

/// A configuration holding `documents` under the given slugs.
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

/// Builds a catalog that is expected to be valid.
pub(crate) fn catalog(config: &LegalConfig) -> Catalog {
    Catalog::build(config).unwrap_or_else(|issues| panic!("expected a valid catalog: {issues}"))
}

/// Builds a catalog that is expected to be refused, and returns each issue as `key: message`.
pub(crate) fn refused(config: &LegalConfig) -> Vec<String> {
    match Catalog::build(config) {
        Ok(_) => panic!("expected the configuration to be refused"),
        Err(issues) => lines(&issues),
    }
}

pub(crate) fn lines(issues: &ConfigIssues) -> Vec<String> {
    issues.iter().map(ToString::to_string).collect()
}

/// Asserts that some issue contains every fragment.
pub(crate) fn assert_issue(issues: &[String], fragments: &[&str]) {
    assert!(
        issues
            .iter()
            .any(|issue| fragments.iter().all(|fragment| issue.contains(fragment))),
        "no issue contains all of {fragments:?} in {issues:#?}"
    );
}
