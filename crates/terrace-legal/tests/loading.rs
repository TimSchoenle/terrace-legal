//! What the configuration loader accepts, exercised through the same loader a host uses.
//!
//! Each test pins a behaviour of terrace-config that this crate depends on, so that an upgrade
//! that changes one fails here rather than in a deployment.
#![cfg(feature = "testing")]

mod common;

use terrace_legal::testing::{HostConfig, LegalFixture};
use terrace_legal::{Catalog, Legal, Negotiation};

fn body(config: &HostConfig, slug: &str, locale: &str) -> String {
    config.legal.documents[slug].body[locale].clone()
}

#[test]
fn a_file_indirection_reads_the_named_file() {
    LegalFixture::run(|jail| {
        LegalFixture::body_file(jail, "terms", "en", "# Terms\n\nBe kind.")?;
        let config = LegalFixture::load(jail)?;
        assert_eq!(body(&config, "terms", "en"), "# Terms\n\nBe kind.");
        Ok(())
    });
}

/// An environment variable cannot contain `-`, so `de-AT` has to be writable as `DE_AT`, and the
/// loader folds the key to lower case.
#[test]
fn an_underscored_key_two_levels_down_loads_and_normalises() {
    LegalFixture::run(|jail| {
        LegalFixture::body_file(jail, "terms", "de_at", "# Servus")?;
        LegalFixture::body_file(jail, "terms", "EN", "# Terms")?;
        let config = LegalFixture::load(jail)?;
        let catalog = Catalog::build(&config.legal).expect("valid");
        let legal = Legal::new(catalog);
        let served = legal
            .document("terms", &Negotiation::from_request(Some("de-AT"), None))
            .expect("served");
        assert_eq!(served.value.locale.as_str(), "de-AT");
        assert_eq!(served.value.body, "# Servus");
        let served = legal
            .document("terms", &Negotiation::from_request(Some("en"), None))
            .expect("served");
        assert_eq!(served.value.body, "# Terms");
        Ok(())
    });
}

/// The loader strips trailing line breaks. The digest has to cover the text that is served, which
/// is the stripped text.
#[test]
fn trailing_line_breaks_are_stripped_and_the_digest_covers_what_is_served() {
    let digest_of = |contents: &str| {
        LegalFixture::run(|jail| {
            LegalFixture::body_file(jail, "terms", "en", contents)?;
            let config = LegalFixture::load(jail)?;
            let legal = Legal::new(Catalog::build(&config.legal).expect("valid"));
            let served = legal
                .document("terms", &Negotiation::default())
                .expect("served");
            Ok((served.value.body.clone(), served.etag))
        })
    };
    let (plain, plain_tag) = digest_of("# Terms");
    let (unix, unix_tag) = digest_of("# Terms\n");
    let (windows, windows_tag) = digest_of("# Terms\r\n\r\n");
    assert_eq!(
        (plain.as_str(), unix.as_str(), windows.as_str()),
        ("# Terms", "# Terms", "# Terms")
    );
    assert_eq!(plain_tag, unix_tag);
    assert_eq!(plain_tag, windows_tag);
}

#[test]
fn a_missing_body_file_refuses_the_load_naming_the_variable() {
    LegalFixture::run(|jail| {
        jail.indirection_at("legal.documents.terms.body.en", "/definitely/not/here.md");
        let error = LegalFixture::load(jail).expect_err("a missing file is not a 404 at runtime");
        assert!(
            error
                .to_string()
                .contains("LEGALTEST_LEGAL__DOCUMENTS__TERMS__BODY__EN_FILE"),
            "the error names the variable: {error}"
        );
        Ok(())
    });
}

#[test]
fn a_body_file_that_is_not_utf8_refuses_the_load_naming_the_variable() {
    LegalFixture::run(|jail| {
        let path = jail.write("bad.md", [0xff, 0xfe, 0x00, 0x80])?;
        jail.indirection_at("legal.documents.terms.body.en", path);
        let error = LegalFixture::load(jail).expect_err("not text");
        assert!(
            error.to_string().contains("BODY__EN_FILE"),
            "the error names the variable: {error}"
        );
        Ok(())
    });
}

#[test]
fn the_directory_of_a_body_file_is_watched() {
    LegalFixture::run(|jail| {
        let file = LegalFixture::body_file(jail, "terms", "en", "# Terms")?;
        let loaded = jail.load_watched::<HostConfig>()?;
        let directory = file.parent().expect("a file has a directory");
        assert!(
            loaded
                .sources
                .watch_paths()
                .iter()
                .any(|watched| watched == directory),
            "{:?} does not watch {directory:?}",
            loaded.sources.watch_paths()
        );
        Ok(())
    });
}

/// A text edit must not need a restart. Editing the mounted file changes what the loader reports,
/// which is what the reload supervisor acts on, and the catalog built from it carries the new text
/// and a new digest.
#[test]
fn an_edited_body_file_is_a_changed_configuration() {
    LegalFixture::run(|jail| {
        let file = LegalFixture::body_file(jail, "terms", "en", "# Terms v1")?;
        let before = jail.load_watched::<HostConfig>()?;
        let unchanged = jail.load_watched::<HostConfig>()?;
        assert!(
            !unchanged.sources.differs_from(&before.sources),
            "an untouched file is no change"
        );

        std::fs::write(&file, "# Terms v2").expect("the file can be edited");
        let after = jail.load_watched::<HostConfig>()?;
        assert!(after.sources.differs_from(&before.sources));

        let old = Legal::new(Catalog::build(&before.value.legal).expect("valid"));
        let new = Legal::new(Catalog::build(&after.value.legal).expect("valid"));
        let (old, new) = (
            old.document("terms", &Negotiation::default())
                .expect("served"),
            new.document("terms", &Negotiation::default())
                .expect("served"),
        );
        assert_eq!(new.value.body, "# Terms v2");
        assert_ne!(old.etag, new.etag);
        Ok(())
    });
}

#[test]
fn an_inline_multiline_body_loads_byte_exact() {
    LegalFixture::run(|jail| {
        jail.config(
            "[legal.documents.privacy]\nbody.en = '''\n# Privacy\n\n  * indented\n\ttabbed \\n stays\n'''\n",
        )?;
        let config = LegalFixture::load(jail)?;
        assert_eq!(
            body(&config, "privacy", "en"),
            "# Privacy\n\n  * indented\n\ttabbed \\n stays\n"
        );
        Ok(())
    });
}

#[test]
fn toml_environment_and_indirection_layers_compose() {
    LegalFixture::run(|jail| {
        jail.config(
            "[legal]\ndefault_locale = \"en\"\n\n[legal.documents.terms]\norder = 10\nupdated = \"2026-08-04\"\ntitle = { en = \"Terms\", de = \"Bedingungen\" }\n\n[legal.documents.imprint]\nurl = \"https://example.org/impressum\"\norder = 30\n",
        )?;
        LegalFixture::body_file(jail, "terms", "en", "# Terms")?;
        jail.env_key("legal.documents.terms.body.de", "# Bedingungen");
        let config = LegalFixture::load(jail)?;

        let catalog = Catalog::build(&config.legal).expect("valid");
        let slugs: Vec<&str> = catalog.slugs().map(|s| s.as_str()).collect();
        assert_eq!(slugs, ["terms", "imprint"]);
        assert_eq!(catalog.default_locale().map(|l| l.as_str()), Some("en"));
        Ok(())
    });
}

/// After the section moved into the library, a leftover `dir` from before the migration was
/// silently ignored and the operator published nothing without being told.
#[test]
fn a_leftover_dir_or_sources_key_is_refused_and_named() {
    LegalFixture::run(|jail| {
        jail.config("[legal]\ndir = \"/etc/legal\"\n")?;
        let error = LegalFixture::load(jail).expect_err("`dir` is no longer a key");
        assert!(error.to_string().contains("dir"), "{error}");
        Ok(())
    });
    LegalFixture::run(|jail| {
        jail.config("[legal.documents.terms]\nsources = { en = \"terms.en.md\" }\n")?;
        let error = LegalFixture::load(jail).expect_err("`sources` is no longer a key");
        assert!(error.to_string().contains("sources"), "{error}");
        Ok(())
    });
}

#[test]
fn a_misspelt_document_key_is_refused_but_an_unknown_slug_is_not() {
    LegalFixture::run(|jail| {
        jail.config("[legal.documents.terms]\nupdatd = \"2026-01-01\"\nbody.en = \"x\"\n")?;
        let error = LegalFixture::load(jail).expect_err("a typo");
        assert!(error.to_string().contains("updatd"), "{error}");
        Ok(())
    });
    LegalFixture::run(|jail| {
        jail.config("[legal.documents.a-slug-nobody-declared]\nbody.en = \"x\"\n")?;
        let config = LegalFixture::load(jail)?;
        assert!(Catalog::build(&config.legal).is_ok());
        Ok(())
    });
}

#[cfg(not(feature = "consent"))]
#[test]
fn a_consent_key_is_refused_by_a_build_that_cannot_enforce_it() {
    LegalFixture::run(|jail| {
        jail.config(
            "[legal.documents.terms]\nbody.en = \"x\"\n\n[legal.documents.terms.consent]\nrequirement = \"accept\"\n",
        )?;
        let error = LegalFixture::load(jail).expect_err("consent is not compiled in");
        assert!(error.to_string().contains("consent"), "{error}");
        Ok(())
    });
}

#[cfg(feature = "consent")]
#[test]
fn a_consent_policy_loads_when_the_feature_is_on() {
    LegalFixture::run(|jail| {
        jail.config(
            "[legal.documents.terms]\nbody.en = \"x\"\n\n[legal.documents.terms.consent]\nrequirement = \"accept\"\nversion = \"2026-08\"\neffective = \"2026-09-01\"\ngrace_days = 14\n",
        )?;
        let config = LegalFixture::load(jail)?;
        let catalog = Catalog::build(&config.legal).expect("valid");
        assert_eq!(catalog.len(), 1);
        Ok(())
    });
    LegalFixture::run(|jail| {
        jail.config("[legal.documents.terms]\nbody.en = \"x\"\n\n[legal.documents.terms.consent]\nrequirement = \"maybe\"\n")?;
        assert!(
            LegalFixture::load(jail).is_err(),
            "an unknown requirement is refused"
        );
        Ok(())
    });
}

/// A file mounted by mistake is read in full by the loader, and only then can the catalog refuse
/// it. The refusal names the key so the operator finds the wrong path.
#[test]
fn an_oversized_body_file_refuses_the_build_naming_the_key() {
    LegalFixture::run(|jail| {
        let huge = "x".repeat(1024 * 1024 + 1);
        LegalFixture::body_file(jail, "terms", "en", &huge)?;
        let config = LegalFixture::load(jail)?;
        let issues = Catalog::build(&config.legal).expect_err("over the cap");
        assert_eq!(
            issues
                .with_prefix("legal")
                .iter()
                .map(|i| i.key())
                .collect::<Vec<_>>(),
            ["legal.documents.terms.body.en"]
        );
        Ok(())
    });
}

/// The configuration surface is an operator contract. Any change to a key, its default or its
/// documentation shows up as a diff of the committed snapshot.
///
/// Regenerate with `UPDATE_SNAPSHOTS=1 cargo test -p terrace-legal --features testing`.
#[test]
fn the_configuration_surface_matches_the_committed_snapshot() {
    let schema = terrace_config::Terrace::new(LegalFixture::PREFIX)
        .schema::<HostConfig>()
        .with_defaults_from(&HostConfig::default())
        .expect("defaults serialise");
    let actual = schema.to_json().expect("renders");

    let name = if cfg!(feature = "consent") {
        "schema.consent.json"
    } else {
        "schema.json"
    };
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    // Read at compile time: a jail in a parallel test empties the process environment, so a
    // runtime lookup would race with it.
    if option_env!("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &actual).expect("write snapshot");
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing snapshot {name}; run with UPDATE_SNAPSHOTS=1"));
    assert_eq!(
        expected.replace("\r\n", "\n"),
        actual,
        "the configuration surface changed; if that is intended, regenerate {name}"
    );
}
