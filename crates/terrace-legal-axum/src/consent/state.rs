//! What the consent routes and the layer share.

use std::fmt;
use std::sync::Arc;

use terrace_legal::Legal;
use terrace_legal::consent::ConsentStore;

use super::clock::{Clock, SystemClock};
use super::subject::SubjectResolver;

/// The served documents, the store, the resolver and the clock, shared by cloning.
pub struct ConsentState<R, St> {
    pub(super) legal: Legal,
    pub(super) store: Arc<St>,
    pub(super) resolver: Arc<R>,
    pub(super) clock: Arc<dyn Clock>,
}

impl<R, St> ConsentState<R, St>
where
    R: SubjectResolver,
    St: ConsentStore<Subject = R::Subject> + 'static,
{
    /// Uses the system clock.
    #[must_use]
    pub fn new(legal: Legal, store: St, resolver: R) -> Self {
        Self::with_clock(legal, store, resolver, SystemClock)
    }

    /// Uses `clock`, which a test fixes to a chosen time.
    #[must_use]
    pub fn with_clock(legal: Legal, store: St, resolver: R, clock: impl Clock) -> Self {
        Self {
            legal,
            store: Arc::new(store),
            resolver: Arc::new(resolver),
            clock: Arc::new(clock),
        }
    }
}

// Derived, `Clone` would demand `R: Clone` and `St: Clone` for a clone that only copies `Arc`s.
impl<R, St> Clone for ConsentState<R, St> {
    fn clone(&self) -> Self {
        Self {
            legal: self.legal.clone(),
            store: Arc::clone(&self.store),
            resolver: Arc::clone(&self.resolver),
            clock: Arc::clone(&self.clock),
        }
    }
}

impl<R, St> fmt::Debug for ConsentState<R, St> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsentState").finish_non_exhaustive()
    }
}
