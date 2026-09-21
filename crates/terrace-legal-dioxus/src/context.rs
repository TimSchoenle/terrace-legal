//! The state a provider shares with the components below it.

use dioxus::prelude::*;
use terrace_legal_model::{LegalDocumentView, LegalIndexEntry};

use crate::shared::{Shared, SharedTransport};
use crate::traits::{LegalRouting, LegalSkin, LegalText, Part};

/// What [`crate::LegalProvider`] puts in context.
#[derive(Clone)]
pub(crate) struct LegalContext {
    pub(crate) transport: SharedTransport,
    pub(crate) text: Shared<dyn LegalText>,
    pub(crate) routing: Shared<dyn LegalRouting>,
    pub(crate) skin: Shared<dyn LegalSkin>,
    /// The language documents are requested in.
    pub(crate) language: ReadSignal<String>,
    /// The published documents. `None` until they are fetched, and again if the fetch fails.
    pub(crate) index: Signal<Option<Vec<LegalIndexEntry>>>,
}

impl LegalContext {
    /// Returns the class a skin gives `part`, or `None` for an empty class so that no empty
    /// attribute is written.
    pub(crate) fn class(&self, part: Part) -> Option<&'static str> {
        let class = self.skin.class(part);
        (!class.is_empty()).then_some(class)
    }
}

/// Returns the published documents, or `None` when there is no provider or the index has not
/// arrived.
///
/// It reads context without registering a hook, so it may be called after an early return. The
/// read subscribes the calling component to the index.
pub fn use_legal_index() -> Option<Vec<LegalIndexEntry>> {
    let context = try_consume_context::<LegalContext>()?;
    let index = context.index.read();
    index.clone()
}

/// Returns the published document `slug`, or `None` when it is not published or the index has not
/// arrived.
///
/// As [`use_legal_index`], it registers no hook.
pub fn use_published(slug: &str) -> Option<LegalIndexEntry> {
    let context = try_consume_context::<LegalContext>()?;
    let index = context.index.read();
    index
        .as_ref()?
        .iter()
        .find(|entry| entry.slug == slug)
        .cloned()
}

/// Returns the title to show for `entry`.
///
/// The operator's title wins, then the application's own name for the slug, then the slug.
pub fn legal_title(text: &dyn LegalText, entry: &LegalIndexEntry) -> String {
    entry
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| text.known_title(&entry.slug))
        .unwrap_or_else(|| entry.slug.clone())
}

/// Returns the title to show above a loaded document, chosen like [`legal_title`].
pub(crate) fn view_title(text: &dyn LegalText, view: &LegalDocumentView) -> String {
    view.title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| text.known_title(&view.slug))
        .unwrap_or_else(|| view.slug.clone())
}

/// Renders a link to `entry`: an anchor for an external document, and the router's link for a
/// hosted one.
pub(crate) fn entry_link(context: &LegalContext, entry: &LegalIndexEntry, part: Part) -> Element {
    let title = legal_title(&*context.text, entry);
    let class = context.skin.class(part);
    match (&entry.kind, &entry.url) {
        (terrace_legal_model::LegalKind::External, Some(url)) => {
            let class = context.class(part);
            rsx! {
                a { class, href: "{url}", target: "_blank", rel: "noopener noreferrer", "{title}" }
            }
        }
        _ => context.routing.document_link(&entry.slug, title, class),
    }
}
