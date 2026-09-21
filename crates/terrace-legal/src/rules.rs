//! Validation a host adds to the built-in checks.

use crate::config::{LegalConfig, LegalDocument};
use crate::issues::ConfigIssue;

/// One extra check on a configuration.
///
/// The built-in checks decide whether a configuration can be served. A rule decides whether it is
/// acceptable to this host: that an imprint is published, that a document has a German text, that
/// no title is longer than a footer can hold. Both methods have empty defaults, so a rule
/// implements only the one it needs.
///
/// A rule is called on every load and every [`crate::Legal::replace`], so it should be cheap and
/// free of side effects. It reports every problem it finds rather than the first.
pub trait Rule: Send + Sync + 'static {
    /// Checks one document. `slug` is the key the operator wrote, and the issues' keys are
    /// relative to the document, such as `["title", "en"]`.
    fn check_document(&self, slug: &str, document: &LegalDocument) -> Vec<ConfigIssue> {
        let _ = (slug, document);
        Vec::new()
    }

    /// Checks the configuration as a whole. The issues' keys are relative to the section.
    fn check_config(&self, config: &LegalConfig) -> Vec<ConfigIssue> {
        let _ = config;
        Vec::new()
    }
}

/// Refuses a configuration that leaves out a document the host cannot run without.
///
/// A registration form that links to terms and a privacy notice has nothing to link to when an
/// operator forgets one. This rule turns that into a boot error naming the slug.
#[derive(Debug, Clone)]
pub struct RequiredDocuments {
    slugs: Vec<String>,
}

impl RequiredDocuments {
    /// Requires each of `slugs` to be published.
    pub fn new<I, S>(slugs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            slugs: slugs.into_iter().map(Into::into).collect(),
        }
    }
}

impl Rule for RequiredDocuments {
    fn check_config(&self, config: &LegalConfig) -> Vec<ConfigIssue> {
        self.slugs
            .iter()
            .filter(|slug| !config.documents.contains_key(slug.as_str()))
            .map(|slug| {
                ConfigIssue::new(
                    ["documents", slug.as_str()],
                    "is required by this deployment but is not published",
                )
            })
            .collect()
    }
}
