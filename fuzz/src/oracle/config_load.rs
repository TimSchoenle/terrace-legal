//! Oracle for validating a configuration and serving from it.

use terrace_legal::{Catalog, Legal, LegalConfig, LegalError, Negotiation};

/// Reads the input as a JSON [`LegalConfig`], builds a catalog, and serves from it.
///
/// Input that is not a configuration is skipped: the loader's own parsing is not what is under
/// test. What is: validation never panics, a refusal always names something, and everything a
/// valid catalog lists can be served.
///
/// # Panics
///
/// When validation or serving panics, when a refusal is empty or its prefixed keys are wrong, or
/// when a valid catalog lists a document it cannot serve, tags its index unstably, or fails to
/// accept its own configuration back.
pub fn check(data: &[u8]) {
    let Ok(config) = serde_json::from_slice::<LegalConfig>(data) else {
        return;
    };

    let catalog = match Catalog::build(&config) {
        Ok(catalog) => catalog,
        Err(issues) => {
            assert!(!issues.is_empty(), "a refusal has to name something");
            for issue in issues.with_prefix("legal.section").iter() {
                assert!(
                    issue.key().starts_with("legal.section"),
                    "prefix missing: {issue}"
                );
                assert!(!issue.message().is_empty(), "an issue has no message");
            }
            return;
        }
    };

    let slugs: Vec<String> = catalog.slugs().map(ToString::to_string).collect();
    let legal = Legal::new(catalog);
    let negotiation = Negotiation::default();

    let index = legal.index(&negotiation);
    assert_eq!(
        index.value.len(),
        slugs.len(),
        "the index lists every document"
    );
    assert_eq!(
        legal.index(&negotiation).etag,
        index.etag,
        "the index tag is stable"
    );

    for (entry, slug) in index.value.iter().zip(&slugs) {
        assert_eq!(&entry.slug, slug, "the index follows catalog order");
        match legal.document(slug, &negotiation) {
            Ok(served) => {
                assert!(
                    !served.value.body.trim().is_empty(),
                    "a blank body was served"
                );
                assert_eq!(&served.value.slug, slug);
            }
            Err(LegalError::External { url }) => {
                assert!(matches!(url.scheme(), "http" | "https"));
                assert!(url.host_str().is_some());
                assert!(url.username().is_empty() && url.password().is_none());
            }
            Err(LegalError::UnknownSlug) => panic!("a listed document `{slug}` is not found"),
        }
    }

    legal
        .replace(&config)
        .expect("a configuration that built once builds again");
    assert_eq!(legal.catalog().generation(), 1);
}
