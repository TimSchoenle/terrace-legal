//! A provider, hooks and components that show `terrace-legal` documents in a Dioxus app.
//!
//! The crate knows no router, no translation runtime, no HTTP client, no icon set and no class
//! names. It depends on Dioxus without a renderer, so it builds for the web and the desktop
//! alike, and on `terrace-legal-model` and `terrace-legal-markdown` only: the server side, with
//! its configuration loader, never reaches a browser bundle.
//!
//! # What an application supplies
//!
//! Four traits carry everything application-specific. They are given to [`LegalProvider`], which
//! is mounted once.
//!
//! | Trait | Supplies |
//! |---|---|
//! | [`LegalTransport`] | Fetching the index and a document, over the application's own client |
//! | [`LegalText`] | The words: known titles, headings, the "last updated" line |
//! | [`LegalRouting`] | A link to a hosted document's page, through the application's router |
//! | [`LegalSkin`] | The class for each [`Part`] |
//!
//!
#![cfg_attr(
    not(feature = "consent"),
    doc = r#"
```
# use dioxus::prelude::*;
use terrace_legal_dioxus::{
    LegalLinks, LegalProvider, LegalRouting, LegalText, Part, Shared, SharedTransport,
    Unstyled,
};
# use terrace_legal_dioxus::{LegalTransport, LocalFuture};
# use terrace_legal_model::{LegalDocumentView, LegalIndexEntry, LocaleTag};
# struct Http;
# impl LegalTransport for Http {
#     type Error = String;
#     fn index(&self, _: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
#         Box::pin(async { Ok(Vec::new()) })
#     }
#     fn document(&self, _: &str, _: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
#         Box::pin(async { Err("offline".to_owned()) })
#     }
# }
# struct Words;
# impl LegalText for Words {
#     fn known_title(&self, slug: &str) -> Option<String> { Some(slug.to_owned()) }
#     fn heading(&self) -> String { "Legal".into() }
#     fn updated(&self, date: &str) -> String { format!("Updated {date}") }
#     fn shown_in(&self, locale: &LocaleTag) -> String { format!("Shown in {locale}") }
# }
# struct Router;
# impl LegalRouting for Router {
#     fn document_link(&self, slug: &str, label: String, _: &'static str) -> Element {
#         rsx! { a { href: "/legal/{slug}", "{label}" } }
#     }
# }

#[component]
fn App() -> Element {
    let language = use_signal(|| "en".to_owned());
    rsx! {
        LegalProvider {
            transport: SharedTransport::new(Http),
            text: Shared::<dyn LegalText>::new(Words),
            routing: Shared::<dyn LegalRouting>::new(Router),
            skin: Shared::<dyn terrace_legal_dioxus::LegalSkin>::new(Unstyled),
            language,
            LegalLinks { part: Part::FooterLink }
        }
    }
}
```
"#
)]
//!
//! With the `consent` feature the transport and text traits gain the consent methods listed under
//! Features, and an application implements those as well.
//!
//! # Components
//!
//! | Item | Shows |
//! |---|---|
//! | [`LegalProvider`] | Nothing of its own. Fetches the index once per language and shares it |
//! | [`LegalLinks`] | One link per published document, in the operator's order |
//! | [`LegalDocumentPage`] | A document: title, updated line, a language note, and the Markdown |
//! | [`AcceptanceSentence`] | "By registering you accept the {terms}", with a link at each placeholder |
//! | `ConsentGate` | The documents a subject owes, with a button that accepts each (feature `consent`) |
//!
//! # Features
//!
//! | Feature | Effect |
//! |---|---|
//! | `consent` | `ConsentGate`, `ConsentList`, and the consent methods of the transport and text traits |
//!
//! # Design record
//!
//! **Why no class names.** A component that writes a class name ties the library to one
//! stylesheet, and a utility-class framework then has to be told to scan the library's source.
//! Each component asks the [`LegalSkin`] for the class of its [`Part`]. The names live in the
//! application, in a file the application's own build already scans. A test reads the crate's
//! source and fails if a component contains a class literal.
//!
//! **Why futures are not `Send`.** A `wasm32` future is not, and demanding it would fail every
//! browser transport. [`LocalFuture`] also avoids a `futures-util` dependency.
//!
//! **Why external links are rendered here.** They need no router, and a link that leaves the
//! site has to carry `rel="noopener noreferrer"` however the application routes its own pages.
//!
//! **Why the language note compares languages.** The server answers a request for `de-AT` with
//! `de` when only `de` is published, and that is the language the reader asked for. The note
//! "Shown in German" appears only when the language differs.
//!
//! **Why the sentence disappears when a document is missing.** An acceptance sentence that links
//! to a document the reader cannot open asks them to agree to something unseen. Rendering nothing
//! is the safer failure.

mod acceptance;
mod context;
mod links;
mod page;
mod provider;
mod shared;
mod traits;

#[cfg(feature = "consent")]
mod consent;
#[cfg(test)]
mod tests;

pub use acceptance::{AcceptanceSentence, AcceptanceSentenceProps};
#[cfg(feature = "consent")]
pub use consent::{ConsentGate, ConsentList, ConsentListProps, accept_document, acceptance_for};
pub use context::{legal_title, use_legal_index, use_published};
pub use links::{LegalLinks, LegalLinksProps};
pub use page::{
    LegalDocumentContent, LegalDocumentContentProps, LegalDocumentPage, LegalDocumentPageProps,
};
pub use provider::{LegalProvider, LegalProviderProps};
pub use shared::{ErasedTransport, Shared, SharedTransport};
pub use traits::{LegalRouting, LegalSkin, LegalText, LegalTransport, LocalFuture, Part, Unstyled};
