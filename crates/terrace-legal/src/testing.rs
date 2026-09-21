//! Test support for hosts: a configuration fixture and the consent conformance suite.
//!
//! Enable the `testing` feature in `[dev-dependencies]` only.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use terrace_config::Error;
use terrace_config::schema::Describe;
use terrace_config::testing::{Harness, Jail};

use crate::config::LegalConfig;

/// A host configuration that nests [`LegalConfig`] at `legal`, the way a real host does.
///
/// Loading through it, rather than building a [`LegalConfig`] by hand, exercises what the loader
/// accepts: environment spellings, `_FILE` indirection and TOML layers.
#[derive(Debug, Clone, Default, Deserialize, Serialize, Describe)]
#[serde(default, deny_unknown_fields)]
pub struct HostConfig {
    /// The legal documents section.
    #[config(nested)]
    pub legal: LegalConfig,
}

/// Arranges configuration layers for a test, over `terrace_config::testing::Harness`.
///
/// Every key is spelled as a dotted path, and the harness derives the environment variable or file
/// name from its loader, so a test keeps passing when the loader renames its mechanism only if it
/// still tests the mechanism.
#[derive(Debug, Clone, Copy)]
pub struct LegalFixture;

impl LegalFixture {
    /// The environment prefix of the fixture's loader.
    pub const PREFIX: &'static str = "LEGALTEST_";

    /// Returns a harness whose loader uses [`Self::PREFIX`].
    #[must_use]
    pub fn harness() -> Harness {
        Harness::new(Self::PREFIX)
    }

    /// Runs `test` in a jail: a temporary working directory and an empty, restored environment.
    ///
    /// # Panics
    ///
    /// Panics when `test` returns an error, as the harness does.
    pub fn run<T>(test: impl FnOnce(&mut Jail<'_>) -> Result<T, Error>) -> T {
        Self::harness().run(test)
    }

    /// Supplies one locale of one document body through `_FILE` indirection, and returns the path
    /// of the file that holds it.
    ///
    /// # Errors
    ///
    /// Returns the harness's error when the file cannot be written.
    pub fn body_file(
        jail: &mut Jail<'_>,
        slug: &str,
        locale: &str,
        text: &str,
    ) -> Result<PathBuf, Error> {
        jail.indirection(&format!("legal.documents.{slug}.body.{locale}"), text)
    }

    /// Loads [`HostConfig`] from whatever the jail has arranged.
    ///
    /// # Errors
    ///
    /// Returns the loader's error, which names the offending key.
    pub fn load(jail: &Jail<'_>) -> Result<HostConfig, Error> {
        jail.load()
    }
}

#[cfg(feature = "consent")]
pub use conformance::consent_conformance;

#[cfg(feature = "consent")]
mod conformance {
    use std::fmt::Debug;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Poll;

    use terrace_legal_model::consent::{Channel, ConsentRecord, RecordKind};
    use terrace_legal_model::{Digest, LocaleTag, Slug};
    use time::OffsetDateTime;

    use crate::consent::ConsentStore;

    /// Runs the rules every [`ConsentStore`] has to satisfy against `store`.
    ///
    /// `subject` maps a number to a distinct subject, and the suite calls it with several
    /// numbers. The store should be empty, and the suite leaves it holding no records of its
    /// subjects. The rules are: an empty store answers empty and erases nothing; records come back
    /// in the order they were appended; `latest` holds one record per document, the last
    /// appended; a subject never sees another's records; `erase` removes exactly one subject's
    /// records and reports how many; and concurrent appends lose nothing.
    ///
    /// # Panics
    ///
    /// Panics with a message naming the rule an implementation breaks.
    pub async fn consent_conformance<S>(
        store: &S,
        subject: impl Fn(u32) -> S::Subject + Send + Sync,
    ) where
        S: ConsentStore,
        S::Subject: Debug,
    {
        let (alice, bob) = (subject(1), subject(2));

        assert!(
            store.latest(&alice).await.expect("latest").is_empty(),
            "a subject with no records has no latest records"
        );
        assert!(
            store.history(&alice).await.expect("history").is_empty(),
            "a subject with no records has no history"
        );
        assert_eq!(
            store.erase(&alice).await.expect("erase"),
            0,
            "erasing a subject with no records reports zero"
        );
        store.append(&[]).await.expect("appending nothing succeeds");

        let terms_v1 = record(&alice, "terms", "v1", RecordKind::Accepted);
        let privacy_v1 = record(&alice, "privacy", "v1", RecordKind::Acknowledged);
        let terms_v2 = record(&alice, "terms", "v2", RecordKind::Accepted);
        store
            .append(&[terms_v1.clone(), privacy_v1.clone()])
            .await
            .expect("append");
        store
            .append(std::slice::from_ref(&terms_v2))
            .await
            .expect("append");
        store
            .append(&[record(&bob, "terms", "v9", RecordKind::Accepted)])
            .await
            .expect("append");

        assert_eq!(
            store.history(&alice).await.expect("history"),
            [terms_v1, privacy_v1.clone(), terms_v2.clone()],
            "history is in the order records were appended"
        );
        assert_eq!(
            store.latest(&alice).await.expect("latest"),
            [privacy_v1, terms_v2],
            "latest holds the last record per document, in append order"
        );
        let bobs = store.latest(&bob).await.expect("latest");
        assert_eq!(bobs.len(), 1, "a subject never sees another's records");
        assert_eq!(bobs[0].version, "v9");

        let withdrawal = record(&alice, "terms", "v2", RecordKind::Withdrawn);
        store
            .append(std::slice::from_ref(&withdrawal))
            .await
            .expect("append");
        let latest = store.latest(&alice).await.expect("latest");
        assert!(
            latest.contains(&withdrawal)
                && latest.iter().filter(|r| r.slug.as_str() == "terms").count() == 1,
            "a withdrawal is a record like any other and supersedes the acceptance"
        );

        assert_eq!(
            store.erase(&alice).await.expect("erase"),
            4,
            "erase reports how many records it removed"
        );
        assert!(store.history(&alice).await.expect("history").is_empty());
        assert_eq!(
            store.history(&bob).await.expect("history").len(),
            1,
            "erasing one subject leaves the others"
        );
        assert_eq!(store.erase(&bob).await.expect("erase"), 1);

        let carol = subject(3);
        let appends = (0..16)
            .map(|index| {
                let version = format!("v{index}");
                let carol = &carol;
                async move {
                    store
                        .append(&[record(carol, "terms", &version, RecordKind::Accepted)])
                        .await
                }
            })
            .collect();
        for outcome in join_all(appends).await {
            outcome.expect("concurrent append");
        }
        let history = store.history(&carol).await.expect("history");
        let mut versions: Vec<String> = history.iter().map(|r| r.version.clone()).collect();
        versions.sort();
        let mut expected: Vec<String> = (0..16).map(|index| format!("v{index}")).collect();
        expected.sort();
        assert_eq!(
            versions, expected,
            "concurrent appends lose nothing and duplicate nothing"
        );
        assert_eq!(store.erase(&carol).await.expect("erase"), 16);
    }

    fn record<T: Clone>(
        subject: &T,
        slug: &str,
        version: &str,
        kind: RecordKind,
    ) -> ConsentRecord<T> {
        ConsentRecord {
            subject: subject.clone(),
            slug: slug.parse::<Slug>().expect("a valid slug"),
            version: version.to_owned(),
            locale: "en".parse::<LocaleTag>().expect("a valid locale"),
            digest: Digest::from_sha256([7; 32]),
            kind,
            channel: Channel::Api,
            at: OffsetDateTime::UNIX_EPOCH,
        }
    }

    /// Polls every future in turn until all are ready, so their awaits interleave on one task.
    async fn join_all<F: Future>(futures: Vec<F>) -> Vec<F::Output> {
        let mut futures: Vec<Pin<Box<F>>> = futures.into_iter().map(Box::pin).collect();
        let mut outputs: Vec<Option<F::Output>> = futures.iter().map(|_| None).collect();
        std::future::poll_fn(|context| {
            let mut pending = false;
            for (future, output) in futures.iter_mut().zip(outputs.iter_mut()) {
                if output.is_none() {
                    match future.as_mut().poll(context) {
                        Poll::Ready(value) => *output = Some(value),
                        Poll::Pending => pending = true,
                    }
                }
            }
            if pending {
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        outputs
            .into_iter()
            .map(|output| output.expect("every future completed"))
            .collect()
    }
}
