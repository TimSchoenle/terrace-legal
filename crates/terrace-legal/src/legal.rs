//! The framework-free service API: negotiation over a catalog, and swapping the catalog.

use std::sync::{Arc, RwLock};

use sha2::{Digest as _, Sha256};
use terrace_legal_model::{
    AcceptLanguage, LegalDocumentView, LegalIndexEntry, LocaleTag, negotiate,
};
use url::Url;

use crate::catalog::{Catalog, CatalogBuilder, Content, Document, Limits};
use crate::config::LegalConfig;
use crate::etag::ETag;
use crate::issues::ConfigIssues;

/// What a reader asked for: an explicit locale, and their `Accept-Language` header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Negotiation {
    /// The `lang` request parameter, when it is a valid locale.
    pub requested: Option<LocaleTag>,
    /// The parsed `Accept-Language` header.
    pub accept: AcceptLanguage,
}

impl Negotiation {
    /// Builds a negotiation from the raw request inputs.
    ///
    /// Neither input can fail the request. A `lang` that is not a valid locale is treated as
    /// absent, and the header is parsed leniently, so a client that sends garbage is served the
    /// default rather than an error.
    #[must_use]
    pub fn from_request(lang: Option<&str>, accept_language: Option<&[u8]>) -> Self {
        Self {
            requested: lang.and_then(|text| text.trim().parse().ok()),
            accept: accept_language
                .map(AcceptLanguage::parse_bytes)
                .unwrap_or_default(),
        }
    }
}

/// A value together with the entity tag of its representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tagged<T> {
    /// The value.
    pub value: T,
    /// The strong tag of the representation, which covers every field of `value`.
    pub etag: ETag,
}

/// Why a document could not be returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LegalError {
    /// No document is published under the slug.
    #[error("no such document")]
    UnknownSlug,
    /// The document is hosted elsewhere, so there is no body to return.
    #[error("the document is hosted at {url}")]
    External {
        /// Where the document lives.
        url: Url,
    },
}

/// The served legal documents, shared and replaceable.
///
/// A `Legal` is a cheap handle: cloning it shares the same catalog. Neither [`Self::index`] nor
/// [`Self::document`] performs I/O, so both are synchronous and cheap on any runtime.
///
/// A host that rebuilds its runtime on every configuration change builds a fresh `Legal` per
/// generation. A host that keeps its state across a reload calls [`Self::replace`].
#[derive(Debug, Clone)]
pub struct Legal {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    builder: CatalogBuilder,
    catalog: RwLock<Arc<Catalog>>,
}

impl Legal {
    /// Serves `catalog`, and validates later replacements against the default [`Limits`].
    #[must_use]
    pub fn new(catalog: Catalog) -> Self {
        Self::with_limits(catalog, Limits::default())
    }

    /// Serves `catalog`, and validates later replacements against `limits`.
    #[must_use]
    pub fn with_limits(catalog: Catalog, limits: Limits) -> Self {
        Self::with_builder(catalog, CatalogBuilder::new().limits(limits))
    }

    /// Serves `catalog`, and validates later replacements with `builder`, so the rules that
    /// accepted the first configuration also judge the next.
    #[must_use]
    pub fn with_builder(catalog: Catalog, builder: CatalogBuilder) -> Self {
        Self {
            inner: Arc::new(Inner {
                builder,
                catalog: RwLock::new(Arc::new(catalog)),
            }),
        }
    }

    /// Validates `config` and, when it is valid, serves it from now on.
    ///
    /// The swap is atomic: a request sees the old catalog or the new one, never a mix. The new
    /// catalog is numbered one above the old, which changes the index tag.
    ///
    /// # Errors
    ///
    /// Returns every problem found in `config`. The previous catalog keeps serving, so a broken
    /// edit never takes documents offline.
    pub fn replace(&self, config: &LegalConfig) -> Result<(), ConfigIssues> {
        let next = self.inner.builder.build(config)?;
        let mut current = self
            .inner
            .catalog
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = current.generation().wrapping_add(1);
        *current = Arc::new(next.with_generation(generation));
        Ok(())
    }

    /// Returns the catalog being served.
    #[must_use]
    pub fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(
            &self
                .inner
                .catalog
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }

    /// Lists the published documents in catalog order, titled for the reader.
    ///
    /// A hosted document's title is the one for the locale its body is served in, so the two never
    /// disagree. An external document's title is negotiated among its own title locales.
    #[must_use]
    pub fn index(&self, request: &Negotiation) -> Tagged<Vec<LegalIndexEntry>> {
        let catalog = self.catalog();
        let default = catalog.default_locale();
        let entries: Vec<LegalIndexEntry> = catalog
            .documents()
            .iter()
            .map(|document| {
                let locale = negotiate(
                    &document.locales,
                    request.requested.as_ref(),
                    &request.accept,
                    default,
                )
                .map(|negotiated| negotiated.locale);
                let entry = LegalIndexEntry::new(document.slug.to_string(), document.kind())
                    .with_title(
                        locale
                            .as_ref()
                            .and_then(|locale| document.titles.get(locale))
                            .cloned(),
                    )
                    .with_updated(document.updated.clone());
                let entry = match &document.content {
                    Content::External(url) => entry.with_url(Some(url.to_string())),
                    Content::Hosted(_) => entry.with_locales(document.locales.clone()),
                };
                #[cfg(feature = "consent")]
                let entry = entry.with_consent(document.consent_summary());
                entry
            })
            .collect();
        let etag = index_etag(catalog.generation(), &entries);
        Tagged {
            value: entries,
            etag,
        }
    }

    /// Returns one hosted document in the negotiated locale.
    ///
    /// # Errors
    ///
    /// [`LegalError::UnknownSlug`] when nothing is published under `slug`, including when `slug`
    /// is not a valid slug. [`LegalError::External`] when the document is hosted elsewhere.
    pub fn document(
        &self,
        slug: &str,
        request: &Negotiation,
    ) -> Result<Tagged<Arc<LegalDocumentView>>, LegalError> {
        let catalog = self.catalog();
        let document: &Document = catalog.get(slug).ok_or(LegalError::UnknownSlug)?;
        let variants = match &document.content {
            Content::Hosted(variants) => variants,
            Content::External(url) => return Err(LegalError::External { url: url.clone() }),
        };
        let negotiated = negotiate(
            &document.locales,
            request.requested.as_ref(),
            &request.accept,
            catalog.default_locale(),
        )
        .ok_or(LegalError::UnknownSlug)?;
        let variant = variants
            .get(&negotiated.locale)
            .ok_or(LegalError::UnknownSlug)?;
        Ok(Tagged {
            value: Arc::clone(&variant.view),
            etag: variant.etag.clone(),
        })
    }
}

/// Hashes the index content. It covers the generation and every entry field, so a title change, a
/// reorder, or a configuration swap each change the tag.
fn index_etag(generation: u64, entries: &[LegalIndexEntry]) -> ETag {
    let mut hasher = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    };
    field(&generation.to_be_bytes());
    for entry in entries {
        field(entry.slug.as_bytes());
        field(match entry.kind {
            terrace_legal_model::LegalKind::Inline => b"inline",
            terrace_legal_model::LegalKind::External => b"external",
            // `LegalKind` may grow; a kind this build does not know still changes the tag.
            _ => b"other",
        });
        for optional in [&entry.title, &entry.url, &entry.updated] {
            // A leading marker keeps an absent value distinct from an empty one.
            match optional {
                Some(text) => {
                    field(b"1");
                    field(text.as_bytes());
                }
                None => field(b"0"),
            }
        }
        field(&(entry.locales.len() as u64).to_be_bytes());
        for locale in &entry.locales {
            field(locale.as_str().as_bytes());
        }
        #[cfg(feature = "consent")]
        match &entry.consent {
            Some(summary) => {
                field(summary.requirement.as_str().as_bytes());
                field(summary.version.as_bytes());
            }
            None => field(b"-"),
        }
    }
    let hash: [u8; 32] = hasher.finalize().into();
    ETag::of(&[&hash])
}
