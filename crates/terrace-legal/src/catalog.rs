//! Validation of a [`LegalConfig`] into something that can be served.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

use sha2::{Digest as _, Sha256};
use terrace_config::schema::{Refine, Refinement};
use terrace_legal_model::{Digest, LegalDocumentView, LegalKind, LocaleTag, Slug};
use url::Url;

use crate::config::{LegalConfig, LegalDocument};
use crate::etag::ETag;
use crate::issues::{ConfigIssue, ConfigIssues};
use crate::rules::Rule;

#[cfg(feature = "consent")]
use terrace_legal_model::{
    ConsentSummary,
    consent::{Policy, Requirement},
};

/// The bounds a configuration is validated against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// The largest body, in bytes, that is accepted for one locale of one document.
    pub max_body_bytes: usize,
}

impl Limits {
    /// The default body cap of 1 MiB. Nothing an operator publishes as a legal document is
    /// larger, and a file mounted by mistake, such as a log, usually is.
    pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
        }
    }
}

/// One locale of a hosted document, with everything derived from it computed once.
#[derive(Debug)]
pub(crate) struct Variant {
    pub(crate) view: Arc<LegalDocumentView>,
    /// Read only when an acceptance is checked against the text that is served.
    #[cfg(feature = "consent")]
    pub(crate) digest: Digest,
    pub(crate) etag: ETag,
}

#[derive(Debug)]
pub(crate) enum Content {
    Hosted(BTreeMap<LocaleTag, Variant>),
    External(Url),
}

#[derive(Debug)]
pub(crate) struct Document {
    pub(crate) slug: Slug,
    pub(crate) order: i32,
    pub(crate) updated: Option<String>,
    pub(crate) titles: BTreeMap<LocaleTag, String>,
    /// The locales negotiation chooses from: the body locales of a hosted document and the title
    /// locales of an external one.
    pub(crate) locales: Vec<LocaleTag>,
    pub(crate) content: Content,
    /// Present only when the requirement is not `none`.
    #[cfg(feature = "consent")]
    pub(crate) consent: Option<Policy>,
}

impl Document {
    pub(crate) fn kind(&self) -> LegalKind {
        match self.content {
            Content::Hosted(_) => LegalKind::Inline,
            Content::External(_) => LegalKind::External,
        }
    }

    #[cfg(feature = "consent")]
    pub(crate) fn consent_summary(&self) -> Option<ConsentSummary> {
        self.consent.as_ref().map(|policy| ConsentSummary {
            requirement: policy.requirement,
            version: policy.version.clone(),
        })
    }
}

/// A validated set of documents, ready to serve.
///
/// A catalog holds each body, its digest and its `ETag`, all computed once when it is built, so
/// serving a request is a map lookup. There is no cache to invalidate: the next version of a body
/// is the next catalog.
///
/// Documents are ordered by their `order` value and then by slug.
#[derive(Debug)]
pub struct Catalog {
    generation: u64,
    default_locale: Option<LocaleTag>,
    documents: Vec<Document>,
    by_slug: HashMap<Slug, usize>,
    warnings: Vec<ConfigIssue>,
}

impl Catalog {
    /// Validates `config` against the default [`Limits`].
    ///
    /// # Errors
    ///
    /// Returns every problem found, not only the first, so an operator can fix the whole file in
    /// one pass. The crate documentation lists the rules.
    pub fn build(config: &LegalConfig) -> Result<Self, ConfigIssues> {
        Self::build_with(config, &Limits::default())
    }

    /// Validates `config` against `limits`.
    ///
    /// # Errors
    ///
    /// As [`Self::build`].
    pub fn build_with(config: &LegalConfig, limits: &Limits) -> Result<Self, ConfigIssues> {
        CatalogBuilder::new().limits(*limits).build(config)
    }

    /// Starts a [`CatalogBuilder`], which validates with limits and extra [`Rule`]s of the host's
    /// choosing.
    #[must_use]
    pub fn builder() -> CatalogBuilder {
        CatalogBuilder::new()
    }

    /// Returns the number of the configuration this catalog was built from.
    ///
    /// A fresh catalog is generation 0. [`crate::Legal::replace`] numbers each swap one higher
    /// than the catalog it replaces, so a cached index is invalidated by a swap.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns a catalog with the same content under another generation number.
    #[must_use]
    pub fn with_generation(mut self, generation: u64) -> Self {
        self.generation = generation;
        self
    }

    /// Returns the number of published documents.
    #[must_use]
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Returns whether nothing is published.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Iterates the slugs in catalog order.
    pub fn slugs(&self) -> impl Iterator<Item = &Slug> {
        self.documents.iter().map(|document| &document.slug)
    }

    /// Returns the configured default locale.
    #[must_use]
    pub fn default_locale(&self) -> Option<&LocaleTag> {
        self.default_locale.as_ref()
    }

    /// Returns what is accepted but worth an operator's attention, such as an external document
    /// on plain `http`. The catalog is valid regardless.
    #[must_use]
    pub fn warnings(&self) -> &[ConfigIssue] {
        &self.warnings
    }

    pub(crate) fn documents(&self) -> &[Document] {
        &self.documents
    }

    pub(crate) fn get(&self, slug: &str) -> Option<&Document> {
        // Only a valid slug can be a key, so the parse is the whole lookup guard: a path such as
        // `..%2F..%2Fetc%2Fpasswd` never reaches the map.
        let slug: Slug = slug.parse().ok()?;
        self.by_slug.get(&slug).map(|&index| &self.documents[index])
    }
}

/// Validates a [`LegalConfig`] into a [`Catalog`] with the host's own limits and rules.
///
/// The built-in checks always run. A [`Rule`] adds to them, so a host can require an imprint or
/// ban a locale without this crate knowing about either. Every issue from the built-in checks and
/// from every rule is reported together.
///
/// ```
/// use terrace_legal::{Catalog, LegalConfig, RequiredDocuments};
///
/// let builder = Catalog::builder().rule(RequiredDocuments::new(["terms", "privacy"]));
/// let issues = builder.build(&LegalConfig::default()).unwrap_err();
/// assert_eq!(issues.len(), 2);
/// ```
///
/// The builder is also the schema's source for everything it checks: it implements
/// [`Refine`], publishing the refinements of every registered [`Rule`] together with those of the
/// built-in checks. A host passes the same builder value to
/// [`Schema::refine_with`](terrace_config::schema::Schema::refine_with), under the key it mounts
/// [`LegalConfig`] at, and to [`crate::Legal::with_builder`]. The crate documentation shows it.
#[derive(Clone, Default)]
pub struct CatalogBuilder {
    limits: Limits,
    rules: Vec<Arc<dyn Rule>>,
}

impl fmt::Debug for CatalogBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CatalogBuilder")
            .field("limits", &self.limits)
            .field("rules", &self.rules.len())
            .finish()
    }
}

impl CatalogBuilder {
    /// Starts with the default [`Limits`] and no rules.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Adds a rule, which runs after the built-in checks and every rule added before it.
    #[must_use]
    pub fn rule(mut self, rule: impl Rule) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    /// Returns the limits in force.
    #[must_use]
    pub fn limits_in_force(&self) -> Limits {
        self.limits
    }

    /// Validates `config`.
    ///
    /// # Errors
    ///
    /// Returns every problem the built-in checks and the rules found.
    pub fn build(&self, config: &LegalConfig) -> Result<Catalog, ConfigIssues> {
        let mut builder = Builder {
            limits: &self.limits,
            issues: Vec::new(),
            warnings: Vec::new(),
        };
        let default_locale = builder.default_locale(config);
        let mut documents = Vec::with_capacity(config.documents.len());
        for (key, source) in &config.documents {
            documents.extend(builder.document(key, source));
        }
        for rule in &self.rules {
            for (key, source) in &config.documents {
                builder.issues.extend(
                    rule.check_document(key, source)
                        .into_iter()
                        .map(|issue| issue.under(["documents", key.as_str()])),
                );
            }
            builder.issues.extend(rule.check_config(config));
        }
        if let Some(issues) = ConfigIssues::from_vec(builder.issues) {
            return Err(issues);
        }

        documents.sort_by(|a: &Document, b: &Document| {
            a.order.cmp(&b.order).then_with(|| a.slug.cmp(&b.slug))
        });
        let by_slug = documents
            .iter()
            .enumerate()
            .map(|(index, document)| (document.slug.clone(), index))
            .collect();
        Ok(Catalog {
            generation: 0,
            default_locale,
            documents,
            by_slug,
            warnings: builder.warnings,
        })
    }
}

impl Refine for CatalogBuilder {
    /// The built-in checks' refinements, then each rule's in registration order. Paths are
    /// relative to the section, as [`Rule::refinements`] states them.
    fn refinements(&self) -> Vec<(String, Refinement)> {
        let mut refinements = built_in_refinements();
        for rule in &self.rules {
            refinements.extend(rule.refinements());
        }
        refinements
    }
}

/// What the built-in checks publish beyond the schema `LegalConfig` derives. Currently nothing.
///
/// Every built-in check was audited against the refinement vocabulary, and none can be stated in
/// it exactly. The only refinement is a map's required entries, and no built-in check requires an
/// entry:
///
/// - Already in the derived schema: unknown keys (`deny_unknown_fields`), the `order` range, and
///   with `consent` the `requirement` values and the `grace_days` range. The derived range is
///   stricter than this check, which skips `grace_days` when `requirement` is `none`.
/// - Expressible exactly with a regular expression the vocabulary cannot yet publish: the slug
///   syntax of `documents` entry names, the locale syntax of `body` and `title` entry names and
///   of `default_locale`, and non-blank `body` and `title` text. The last needs an explicit
///   class of the Unicode `White_Space` characters `str::trim` removes: JSON Schema's `\S` also
///   treats U+FEFF as blank, which would reject text this check accepts.
/// - Needing conditional or exclusive keywords the vocabulary lacks: exactly one of `body` and
///   `url`, and with `consent` a `version`, a hosted `body` and, for a `grace_days` above zero,
///   `effective`, each only for a requirement other than `none`.
/// - Not expressible exactly in JSON Schema: two locale keys normalising to the same tag, a
///   body's size in bytes (`maxLength` counts characters), URL parsing, and `effective`, which
///   the parser accepts in every ISO 8601 date form (`20260922`, `2026-265`, `2026-W39-2`), so a
///   `YYYY-MM-DD` pattern would reject valid dates.
///
/// A refinement belongs here once the vocabulary can state a check without rejecting anything the
/// check accepts; see [`Rule`] for that invariant.
fn built_in_refinements() -> Vec<(String, Refinement)> {
    Vec::new()
}

struct Builder<'a> {
    limits: &'a Limits,
    issues: Vec<ConfigIssue>,
    warnings: Vec<ConfigIssue>,
}

impl Builder<'_> {
    fn issue<S: Into<String>>(
        &mut self,
        path: impl IntoIterator<Item = S>,
        message: impl Into<String>,
    ) {
        self.issues.push(ConfigIssue::new(path, message));
    }

    fn default_locale(&mut self, config: &LegalConfig) -> Option<LocaleTag> {
        let text = config.default_locale.as_deref()?;
        match text.parse() {
            Ok(tag) => Some(tag),
            Err(error) => {
                self.issue(
                    ["default_locale"],
                    format!("{text:?} is not a locale: {error}"),
                );
                None
            }
        }
    }

    /// Validates one document and returns it, or `None` when it added any issue.
    fn document(&mut self, key: &str, source: &LegalDocument) -> Option<Document> {
        let before = self.issues.len();

        let slug = match key.parse::<Slug>() {
            Ok(slug) => Some(slug),
            Err(error) => {
                self.issue(["documents", key], format!("is not a valid slug: {error}"));
                None
            }
        };
        let titles = self.locale_map(key, "title", &source.title);
        let updated = source
            .updated
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        for (locale, text) in &titles {
            if text.trim().is_empty() {
                self.issue(["documents", key, "title", locale.as_str()], "is blank");
            }
        }

        let hosted = !source.body.is_empty();
        let bodies = match (hosted, &source.url) {
            (false, None) => {
                self.issue(["documents", key], "needs either `body` or `url`");
                None
            }
            (true, Some(_)) => {
                self.issue(["documents", key], "has both `body` and `url`; use one");
                None
            }
            (true, None) => Some(self.bodies(key, &source.body)),
            (false, Some(_)) => None,
        };
        let external = source.url.as_deref().and_then(|text| self.url(key, text));

        #[cfg(feature = "consent")]
        let consent = self.consent(key, source, bodies.as_ref());

        if self.issues.len() != before {
            return None;
        }
        let slug = slug?;

        let (content, locales) = if let Some(bodies) = bodies {
            let mut variants = BTreeMap::new();
            for (locale, (text, digest)) in bodies {
                let title = titles.get(&locale).cloned();
                let etag = ETag::of(&[
                    slug.as_str().as_bytes(),
                    locale.as_str().as_bytes(),
                    title.as_deref().unwrap_or_default().as_bytes(),
                    updated.as_deref().unwrap_or_default().as_bytes(),
                    digest.as_str().as_bytes(),
                ]);
                let view = LegalDocumentView::new(
                    slug.to_string(),
                    locale.clone(),
                    text,
                    #[cfg(feature = "consent")]
                    digest.clone(),
                )
                .with_title(title)
                .with_updated(updated.clone());
                #[cfg(feature = "consent")]
                let view = view.with_consent(consent.as_ref().map(|policy| ConsentSummary {
                    requirement: policy.requirement,
                    version: policy.version.clone(),
                }));
                variants.insert(
                    locale,
                    Variant {
                        view: Arc::new(view),
                        #[cfg(feature = "consent")]
                        digest,
                        etag,
                    },
                );
            }
            let locales = variants.keys().cloned().collect();
            (Content::Hosted(variants), locales)
        } else {
            let url = external?;
            (Content::External(url), titles.keys().cloned().collect())
        };

        Some(Document {
            slug,
            order: source.order,
            updated,
            titles,
            locales,
            content,
            #[cfg(feature = "consent")]
            consent,
        })
    }

    /// Parses the locale keys of a map, refusing two keys that normalise to the same tag.
    fn locale_map(
        &mut self,
        key: &str,
        field: &str,
        map: &BTreeMap<String, String>,
    ) -> BTreeMap<LocaleTag, String> {
        let mut out: BTreeMap<LocaleTag, String> = BTreeMap::new();
        let mut spelled: BTreeMap<LocaleTag, &str> = BTreeMap::new();
        for (locale_key, text) in map {
            let tag = match locale_key.parse::<LocaleTag>() {
                Ok(tag) => tag,
                Err(error) => {
                    self.issue(
                        ["documents", key, field, locale_key],
                        format!("is not a locale: {error}"),
                    );
                    continue;
                }
            };
            if let Some(first) = spelled.get(&tag) {
                self.issue(
                    ["documents", key, field, locale_key],
                    format!("names the same locale, `{tag}`, as `{first}`"),
                );
                continue;
            }
            spelled.insert(tag.clone(), locale_key);
            out.insert(tag, text.clone());
        }
        out
    }

    /// Validates every body and computes its digest.
    fn bodies(
        &mut self,
        key: &str,
        map: &BTreeMap<String, String>,
    ) -> BTreeMap<LocaleTag, (String, Digest)> {
        let mut out = BTreeMap::new();
        for (locale, text) in self.locale_map(key, "body", map) {
            if text.trim().is_empty() {
                self.issue(["documents", key, "body", locale.as_str()], "is blank");
                continue;
            }
            if text.len() > self.limits.max_body_bytes {
                self.issue(
                    ["documents", key, "body", locale.as_str()],
                    format!(
                        "is {} bytes, over the limit of {}; a file of the wrong kind may be mounted",
                        text.len(),
                        self.limits.max_body_bytes
                    ),
                );
                continue;
            }
            let digest = Digest::from_sha256(Sha256::digest(text.as_bytes()).into());
            out.insert(locale, (text, digest));
        }
        out
    }

    fn url(&mut self, key: &str, text: &str) -> Option<Url> {
        let path = ["documents", key, "url"];
        let url = match Url::parse(text) {
            Ok(url) => url,
            Err(error) => {
                self.issue(path, format!("is not an absolute URL: {error}"));
                return None;
            }
        };
        if !matches!(url.scheme(), "http" | "https") {
            self.issue(
                path,
                format!("has scheme `{}`; only http and https", url.scheme()),
            );
            return None;
        }
        if url.host_str().is_none_or(str::is_empty) {
            self.issue(path, "has no host");
            return None;
        }
        if !url.username().is_empty() || url.password().is_some() {
            self.issue(path, "contains credentials, which would be published");
            return None;
        }
        if url.scheme() == "http" {
            self.warnings.push(ConfigIssue::new(
                path,
                "uses plain http, so the link is not protected in transit",
            ));
        }
        Some(url)
    }

    #[cfg(feature = "consent")]
    fn consent(
        &mut self,
        key: &str,
        source: &LegalDocument,
        bodies: Option<&BTreeMap<LocaleTag, (String, Digest)>>,
    ) -> Option<Policy> {
        use time::Date;
        use time::format_description::well_known::Iso8601;

        let config = &source.consent;
        let path = |field: &'static str| ["documents", key, "consent", field];

        if config.requirement == Requirement::None {
            return None;
        }
        let version = config.version.as_deref().map(str::trim).unwrap_or_default();
        if version.is_empty() {
            self.issue(
                path("version"),
                "is required when `requirement` is not `none`, or an acceptance cannot say what was accepted",
            );
        }
        if source.url.is_some() {
            self.issue(
                path("requirement"),
                "needs a hosted `body`: the text of an external document cannot be proven to have been shown",
            );
        }
        let effective = config.effective.as_deref().and_then(|text| {
            Date::parse(text.trim(), &Iso8601::DATE)
                .map_err(|_| {
                    self.issue(
                        path("effective"),
                        format!("{text:?} is not a date; expected YYYY-MM-DD"),
                    )
                })
                .ok()
        });
        if config.grace_days > 365 {
            self.issue(path("grace_days"), "is above the maximum of 365");
        }
        if config.grace_days > 0 && config.effective.is_none() {
            self.issue(
                path("grace_days"),
                "needs `effective`, which starts the grace period",
            );
        }

        Some(Policy {
            requirement: config.requirement,
            version: version.to_owned(),
            effective,
            grace_days: config.grace_days,
            digests: bodies
                .map(|bodies| {
                    bodies
                        .iter()
                        .map(|(locale, (_, digest))| (locale.clone(), digest.clone()))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
}
