//! Shared, comparable handles to the application's trait objects.

use std::fmt;
use std::ops::Deref;
use std::rc::Rc;

use terrace_legal_model::{LegalDocumentView, LegalIndexEntry};

use crate::traits::{LegalRouting, LegalSkin, LegalText, LegalTransport, LocalFuture};

#[cfg(feature = "consent")]
use terrace_legal_model::consent::{Acceptance, ConsentStatus};

/// A reference-counted handle that a component can take as a property.
///
/// Dioxus re-renders a component when its properties change, and needs to compare them. Two
/// handles are equal when they point at the same value, which is the right question for an object
/// the application creates once and passes down.
pub struct Shared<T: ?Sized>(Rc<T>);

impl<T: ?Sized> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: ?Sized> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl<T: ?Sized> Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

impl Shared<dyn LegalText> {
    /// Shares `text`.
    pub fn new(text: impl LegalText) -> Self {
        Self(Rc::new(text))
    }
}

impl Shared<dyn LegalRouting> {
    /// Shares `routing`.
    pub fn new(routing: impl LegalRouting) -> Self {
        Self(Rc::new(routing))
    }
}

impl Shared<dyn LegalSkin> {
    /// Shares `skin`.
    pub fn new(skin: impl LegalSkin) -> Self {
        Self(Rc::new(skin))
    }
}

/// A transport with its error type erased to text.
pub type SharedTransport = Shared<dyn ErasedTransport>;

impl Shared<dyn ErasedTransport> {
    /// Shares `transport`.
    pub fn new(transport: impl LegalTransport) -> Self {
        Self(Rc::new(Erased(transport)))
    }
}

mod private {
    pub trait Sealed {}
}

/// A [`LegalTransport`] whose error is its `Display` text.
///
/// It is implemented for every [`LegalTransport`], and only for those. A component holds this
/// instead of the concrete transport, so its properties do not carry the transport's type.
pub trait ErasedTransport: private::Sealed + 'static {
    /// As [`LegalTransport::index`].
    fn index(&self, lang: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>>;

    /// As [`LegalTransport::document`].
    fn document(&self, slug: &str, lang: &str) -> LocalFuture<Result<LegalDocumentView, String>>;

    /// As [`LegalTransport::consent_status`].
    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>>;

    /// As [`LegalTransport::accept`].
    #[cfg(feature = "consent")]
    fn accept(&self, acceptance: Acceptance) -> LocalFuture<Result<(), String>>;
}

struct Erased<T>(T);

impl<T> private::Sealed for Erased<T> {}

impl<T: LegalTransport> ErasedTransport for Erased<T> {
    fn index(&self, lang: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
        let future = self.0.index(lang);
        Box::pin(async move { future.await.map_err(|error| error.to_string()) })
    }

    fn document(&self, slug: &str, lang: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
        let future = self.0.document(slug, lang);
        Box::pin(async move { future.await.map_err(|error| error.to_string()) })
    }

    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>> {
        let future = self.0.consent_status();
        Box::pin(async move { future.await.map_err(|error| error.to_string()) })
    }

    #[cfg(feature = "consent")]
    fn accept(&self, acceptance: Acceptance) -> LocalFuture<Result<(), String>> {
        let future = self.0.accept(acceptance);
        Box::pin(async move { future.await.map_err(|error| error.to_string()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::Unstyled;

    #[test]
    fn handles_compare_by_identity() {
        let a = Shared::<dyn LegalSkin>::new(Unstyled);
        let b = a.clone();
        let c = Shared::<dyn LegalSkin>::new(Unstyled);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
