//! Validation a host adds to the built-in checks.

use terrace_config::schema::Refinement;

use crate::config::{LegalConfig, LegalDocument};
use crate::issues::ConfigIssue;

/// One extra check on a configuration.
///
/// The built-in checks decide whether a configuration can be served. A rule decides whether it is
/// acceptable to this host: that an imprint is published, that a document has a German text, that
/// no title is longer than a footer can hold. Every method has an empty default, so a rule
/// implements only the ones it needs.
///
/// A rule is called on every load and every [`crate::Legal::replace`], so it should be cheap and
/// free of side effects. It reports every problem it finds rather than the first.
///
/// # Publishing a rule in the schema
///
/// A check that runs only at boot is invisible to whoever writes the configuration: the published
/// schema says what the types say, and a deployment generated from it fails when the rule refuses
/// it. [`Self::refinements`] states the rule's check in the schema, from the same value that runs
/// it, so the two cannot disagree. A host publishes every registered rule at once through
/// [`crate::CatalogBuilder`], which implements [`terrace_config::schema::Refine`].
///
/// **Soundness: a refinement is never stricter than the check.** Every configuration the refined
/// schema rejects, [`Self::check_config`] or [`Self::check_document`] must reject too. The schema
/// may be weaker, stating part of the check or none of it, but a schema that rejects a
/// configuration the rule accepts sends an operator to "fix" a valid deployment. A rule whose
/// check cannot be stated exactly in the refinement vocabulary publishes the exact part or
/// nothing, and says so in its documentation. It never publishes an approximation.
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

    /// States this rule's check in the configuration schema, as `(path, refinement)` pairs.
    ///
    /// Each path is relative to the section, like a [`ConfigIssue`] key joined with `.`: a rule
    /// on the document map names `documents`, never the key a host mounts the section under.
    /// Every refinement must be at most as strict as the check; see the trait documentation.
    ///
    /// The default states nothing, which is always sound.
    fn refinements(&self) -> Vec<(String, Refinement)> {
        Vec::new()
    }
}

/// Refuses a configuration that leaves out a document the host cannot run without.
///
/// A registration form that links to terms and a privacy notice has nothing to link to when an
/// operator forgets one. This rule turns that into a boot error naming the slug, and publishes the
/// same slugs as the entries the `documents` map requires, so a schema-driven deployment tool
/// refuses the configuration before it is ever deployed.
///
/// The published constraint is exact. The check asks only whether `documents` has an entry under
/// each slug, whatever the entry holds, so an external (`url`-only) document satisfies it. JSON
/// Schema's `required` asserts exactly that presence and nothing about the entry.
///
/// [`Schema::refine_with`](terrace_config::schema::Schema::refine_with) refuses, when the schema
/// is built, a slug that some layer reaching `documents` cannot spell as an entry name. `a__b` is
/// a valid slug, but its environment variable reads as the nested key `a.b`, so the schema would
/// name an entry that variable cannot supply. Prefer slugs without `__`.
///
/// ```
/// use terrace_config::schema::{Refinement, Refine};
/// use terrace_legal::{Catalog, RequiredDocuments};
///
/// let builder = Catalog::builder().rule(RequiredDocuments::new(["terms", "privacy"]));
/// assert_eq!(
///     builder.refinements(),
///     [("documents".to_owned(), Refinement::required_entries(["privacy", "terms"]))],
/// );
/// ```
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

    // Published even when there are no slugs: terrace-config still checks the path and the
    // key's shape for an empty set, so a rule constructed empty cannot hide a broken mount.
    fn refinements(&self) -> Vec<(String, Refinement)> {
        vec![(
            "documents".to_owned(),
            Refinement::required_entries(self.slugs.iter().cloned()),
        )]
    }
}
