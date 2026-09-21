//! The four things an application supplies: transport, text, routing and skin.

use std::fmt::Display;
use std::future::Future;
use std::pin::Pin;

use dioxus::prelude::Element;
use terrace_legal_model::{LegalDocumentView, LegalIndexEntry, LocaleTag};

#[cfg(feature = "consent")]
use terrace_legal_model::consent::{Acceptance, ConsentStatus};

/// A future that is not `Send`, because a `wasm32` future is not.
///
/// Requiring `Send` would fail every browser transport, and avoiding it here keeps a
/// `futures-util` dependency out of the crate.
pub type LocalFuture<T> = Pin<Box<dyn Future<Output = T>>>;

/// Fetches documents from the server, in whatever way the application already talks to it.
///
/// Implement it over the application's generated client and convert that client's types into
/// [`terrace_legal_model`]'s, so the frontend never sees server types.
pub trait LegalTransport: 'static {
    /// The transport's failure type. Its `Display` text is what [`crate::LegalDocumentPage`]
    /// hands to its `error` callback.
    type Error: Display + 'static;

    /// Fetches the index in `lang`.
    fn index(&self, lang: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, Self::Error>>;

    /// Fetches one hosted document in `lang`.
    fn document(
        &self,
        slug: &str,
        lang: &str,
    ) -> LocalFuture<Result<LegalDocumentView, Self::Error>>;

    /// Fetches what the signed-in subject still owes.
    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, Self::Error>>;

    /// Records an acceptance.
    #[cfg(feature = "consent")]
    fn accept(&self, acceptance: Acceptance) -> LocalFuture<Result<(), Self::Error>>;
}

/// The words the components show, from the application's own catalogue.
///
/// The components contain no natural-language text, so a translation lives where the rest of the
/// application's translations do.
pub trait LegalText: 'static {
    /// Returns the application's own name for a document, or `None` when it has none. It is
    /// used when the operator set no title, so `terms` can read as "Terms of Service".
    fn known_title(&self, slug: &str) -> Option<String>;

    /// Returns the heading of the group of legal links, for example "Legal".
    fn heading(&self) -> String;

    /// Returns the "last updated" line for `date`, which the operator wrote verbatim.
    fn updated(&self, date: &str) -> String;

    /// Returns the note shown when a document is not in the language the reader asked for, for
    /// example "Shown in English".
    fn shown_in(&self, locale: &LocaleTag) -> String;

    /// Returns the heading of the list of documents the subject has to accept.
    #[cfg(feature = "consent")]
    fn consent_heading(&self) -> String;

    /// Returns the label of the button that accepts one document.
    #[cfg(feature = "consent")]
    fn consent_accept(&self) -> String;

    /// Returns the message shown when an acceptance could not be recorded. `reason` is the
    /// transport's error text.
    #[cfg(feature = "consent")]
    fn consent_failed(&self, reason: &str) -> String;
}

/// Renders a link to a hosted document with the application's router.
///
/// External links are rendered by this crate as plain anchors, so only internal ones need a
/// router.
pub trait LegalRouting: 'static {
    /// Returns a link to the page of the document `slug`, labelled `label` and styled with `class`,
    /// which is empty when the skin sets none.
    ///
    /// # Errors
    ///
    /// An implementation returns the error case of `Element` only to report a render failure of
    /// its own. The components render nothing for it.
    fn document_link(&self, slug: &str, label: String, class: &'static str) -> Element;
}

/// The places a component puts a class.
///
/// A component never writes a class name itself. It asks the [`LegalSkin`] for the class of the
/// part it is rendering, so the class names live in the application, next to the stylesheet that
/// defines them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Part {
    /// A link in a page footer.
    FooterLink,
    /// A link in a menu sheet.
    SheetLink,
    /// A link inside running text, such as [`crate::AcceptanceSentence`].
    Link,
    /// The container of a document page.
    Page,
    /// The "last updated" line.
    Meta,
    /// The note that the document is not in the requested language.
    LocaleNote,
    /// The container of the rendered Markdown.
    Prose,
    /// The container of the consent list.
    ConsentGate,
    /// One document in the consent list.
    ConsentItem,
    /// The button that accepts a document.
    ConsentAction,
}

/// Maps a [`Part`] to the class an application styles it with.
pub trait LegalSkin: 'static {
    /// Returns the class of `part`, or an empty string for none.
    fn class(&self, part: Part) -> &'static str;
}

/// A skin that styles nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unstyled;

impl LegalSkin for Unstyled {
    fn class(&self, _part: Part) -> &'static str {
        ""
    }
}
