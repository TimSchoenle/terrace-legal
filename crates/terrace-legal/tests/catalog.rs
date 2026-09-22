//! Validation of a configuration into a catalog.

mod common;

use common::{assert_issue, catalog, config, external, hosted, refused, titled};
use terrace_legal::{Catalog, Limits};

#[test]
fn an_empty_section_publishes_nothing_and_is_valid() {
    let catalog = catalog(&config(None, vec![]));
    assert!(catalog.is_empty());
    assert_eq!(catalog.len(), 0);
    assert_eq!(catalog.generation(), 0);
}

#[test]
fn a_document_with_no_body_and_no_url_is_refused_by_slug() {
    let issues = refused(&config(None, vec![("terms", hosted(&[]))]));
    assert_eq!(issues.len(), 1);
    assert_issue(
        &issues,
        &["documents.terms", "needs either `body` or `url`"],
    );
}

#[test]
fn a_document_that_is_both_hosted_and_external_is_refused() {
    let mut document = hosted(&[("en", "# Terms")]);
    document.url = Some("https://example.org/terms".into());
    let issues = refused(&config(None, vec![("terms", document)]));
    assert_issue(&issues, &["documents.terms", "both `body` and `url`"]);
}

/// A relative URL was refused, but `http://` with no host and `https://user:pass@host` were
/// accepted, so a broken link and a published password both got through.
#[test]
fn a_relative_url_is_refused_but_an_absolute_one_is_not() {
    for bad in [
        "/impressum",
        "impressum",
        "//example.org/impressum",
        "http://",
        "https://",
        "https://user:pass@example.org/impressum",
        "https://user@example.org/impressum",
        "ftp://example.org/impressum",
        "javascript:alert(1)",
        "mailto:legal@example.org",
        "",
    ] {
        let issues = refused(&config(None, vec![("imprint", external(bad))]));
        assert_issue(&issues, &["documents.imprint.url"]);
    }
    for good in [
        "https://example.org/impressum",
        "https://example.org:8443/a?b=c#d",
        "http://example.org/impressum",
    ] {
        let catalog = catalog(&config(None, vec![("imprint", external(good))]));
        assert_eq!(catalog.len(), 1, "url {good}");
    }
}

#[test]
fn credentials_in_a_url_are_named_as_the_reason() {
    let issues = refused(&config(
        None,
        vec![("imprint", external("https://user:pass@example.org/"))],
    ));
    assert_issue(&issues, &["credentials"]);
    assert!(
        issues.iter().all(|issue| !issue.contains("pass@")),
        "an issue must not repeat the credential: {issues:?}"
    );
}

/// Plain `http` stayed silent, so an operator never learned the link was unprotected.
#[test]
fn plain_http_is_accepted_with_a_warning() {
    let catalog = catalog(&config(
        None,
        vec![
            ("imprint", external("http://example.org/")),
            ("privacy", external("https://example.org/")),
        ],
    ));
    assert_eq!(catalog.warnings().len(), 1);
    assert_eq!(catalog.warnings()[0].key(), "documents.imprint.url");
}

#[test]
fn slugs_are_restricted_to_what_a_url_segment_and_a_variable_name_can_carry() {
    for good in ["terms", "a", "0", "privacy_policy", "data-protection"] {
        catalog(&config(None, vec![(good, hosted(&[("en", "x")]))]));
    }
    for bad in ["Terms", "-terms", "te rms", "te/rms", "..", "terms.md", ""] {
        let issues = refused(&config(None, vec![(bad, hosted(&[("en", "x")]))]));
        assert_issue(&issues, &["not a valid slug"]);
    }
    let long = "a".repeat(65);
    let issues = refused(&config(None, vec![(&long, hosted(&[("en", "x")]))]));
    assert_issue(&issues, &["not a valid slug"]);
    catalog(&config(
        None,
        vec![(&"a".repeat(64), hosted(&[("en", "x")]))],
    ));
}

/// Locale keys were case-sensitive, so a config key `EN` never matched a request, and `de-AT`
/// could not be written as an environment variable at all.
#[test]
fn locale_keys_are_normalised() {
    for key in ["en", "EN", "En"] {
        catalog(&config(None, vec![("terms", hosted(&[(key, "x")]))]));
    }
    for key in ["de_at", "DE-at", "de-AT", "DE_AT"] {
        catalog(&config(None, vec![("terms", hosted(&[(key, "x")]))]));
    }
}

#[test]
fn two_keys_that_normalise_to_the_same_locale_are_refused() {
    let issues = refused(&config(
        None,
        vec![("terms", hosted(&[("de_at", "a"), ("DE-AT", "b")]))],
    ));
    assert_eq!(issues.len(), 1);
    assert_issue(&issues, &["documents.terms.body", "de-AT"]);

    let titled = titled(hosted(&[("en", "x")]), &[("en", "A"), ("EN", "B")]);
    let issues = refused(&config(None, vec![("terms", titled)]));
    assert_issue(&issues, &["documents.terms.title", "en"]);
}

#[test]
fn a_key_that_is_not_a_locale_is_refused_by_key() {
    let issues = refused(&config(
        None,
        vec![("terms", hosted(&[("english", "x"), ("de-", "y")]))],
    ));
    assert_issue(&issues, &["documents.terms.body.english"]);
    assert_issue(&issues, &["documents.terms.body.de-"]);
}

#[test]
fn a_default_locale_that_is_not_a_locale_is_refused() {
    let issues = refused(&config(Some("english"), vec![]));
    assert_issue(&issues, &["default_locale"]);
}

/// The size cap was checked on metadata and then the file was read without a bound, so a file that
/// grew between the two calls bypassed it. The cap is now enforced on the text that is served.
#[test]
fn an_oversized_body_is_refused_naming_its_key() {
    let limits = Limits { max_body_bytes: 8 };
    let at_limit = config(None, vec![("terms", hosted(&[("en", "12345678")]))]);
    Catalog::build_with(&at_limit, &limits).expect("a body at the limit is accepted");

    let over = config(None, vec![("terms", hosted(&[("en", "123456789")]))]);
    let issues = Catalog::build_with(&over, &limits).expect_err("over the limit");
    let issues: Vec<String> = issues.iter().map(ToString::to_string).collect();
    assert_issue(
        &issues,
        &["documents.terms.body.en", "9 bytes", "limit of 8"],
    );
}

#[test]
fn the_default_limit_is_one_mebibyte() {
    assert_eq!(Limits::default().max_body_bytes, 1024 * 1024);
    let at = "x".repeat(1024 * 1024);
    catalog(&config(None, vec![("terms", hosted(&[("en", &at)]))]));
    let over = "x".repeat(1024 * 1024 + 1);
    let issues = refused(&config(None, vec![("terms", hosted(&[("en", &over)]))]));
    assert_issue(&issues, &["documents.terms.body.en"]);
}

#[test]
fn a_blank_body_or_title_is_refused() {
    let issues = refused(&config(None, vec![("terms", hosted(&[("en", " \n\t ")]))]));
    assert_issue(&issues, &["documents.terms.body.en", "blank"]);

    let document = titled(hosted(&[("en", "x")]), &[("en", "  ")]);
    let issues = refused(&config(None, vec![("terms", document)]));
    assert_issue(&issues, &["documents.terms.title.en", "blank"]);
}

/// Refusing only the first problem sent the operator through a fix-and-retry loop, one boot per
/// mistake.
#[test]
fn every_issue_is_reported_at_once_with_full_key_paths() {
    let config = config(
        Some("nope"),
        vec![
            ("Bad Slug", hosted(&[("en", "x")])),
            ("empty", hosted(&[])),
            ("both", {
                let mut document = hosted(&[("en", "x")]);
                document.url = Some("https://example.org/".into());
                document
            }),
            ("relative", external("/x")),
            ("blank", hosted(&[("en", "")])),
        ],
    );
    let issues = Catalog::build(&config).expect_err("refused");
    assert_eq!(issues.len(), 6);
    let prefixed: Vec<String> = issues
        .with_prefix("app.legal")
        .iter()
        .map(|issue| issue.key())
        .collect();
    for key in [
        "app.legal.default_locale",
        "app.legal.documents.Bad Slug",
        "app.legal.documents.empty",
        "app.legal.documents.both",
        "app.legal.documents.relative.url",
        "app.legal.documents.blank.body.en",
    ] {
        assert!(
            prefixed.iter().any(|k| k == key),
            "missing {key} in {prefixed:?}"
        );
    }
}

/// The index was in slug order (`imprint, privacy, terms`), so an operator could not choose the
/// order of the footer.
#[test]
fn documents_are_ordered_by_their_order_value_then_by_slug() {
    let mut a = hosted(&[("en", "x")]);
    a.order = 30;
    let mut b = hosted(&[("en", "x")]);
    b.order = 10;
    let mut c = hosted(&[("en", "x")]);
    c.order = 10;
    let mut d = hosted(&[("en", "x")]);
    d.order = -5;
    let catalog = catalog(&config(
        None,
        vec![("imprint", a), ("terms", b), ("privacy", c), ("zzz", d)],
    ));
    let slugs: Vec<&str> = catalog.slugs().map(|slug| slug.as_str()).collect();
    assert_eq!(slugs, ["zzz", "privacy", "terms", "imprint"]);
}

#[test]
fn a_blank_updated_line_is_treated_as_absent() {
    let mut document = hosted(&[("en", "x")]);
    document.updated = Some("   ".into());
    let catalog = catalog(&config(None, vec![("terms", document)]));
    let legal = terrace_legal::Legal::new(catalog);
    let index = legal.index(&terrace_legal::Negotiation::default());
    assert_eq!(index.value[0].updated, None);
}

#[cfg(feature = "consent")]
mod consent {
    use super::*;
    use terrace_legal::ConsentPolicy;
    use terrace_legal_model::consent::Requirement;

    fn with_policy(policy: ConsentPolicy) -> terrace_legal::LegalConfig {
        let mut document = hosted(&[("en", "x")]);
        document.consent = policy;
        config(None, vec![("terms", document)])
    }

    fn policy(requirement: Requirement) -> ConsentPolicy {
        ConsentPolicy {
            requirement,
            version: Some("2026-08".into()),
            effective: Some("2026-09-01".into()),
            grace_days: 14,
        }
    }

    #[test]
    fn a_valid_policy_is_accepted_for_each_requirement() {
        for requirement in Requirement::ALL {
            catalog(&with_policy(policy(requirement)));
        }
    }

    #[test]
    fn a_requirement_without_a_version_is_refused() {
        for version in [None, Some(String::new()), Some("  ".into())] {
            let issues = refused(&with_policy(ConsentPolicy {
                version,
                ..policy(Requirement::Accept)
            }));
            assert_issue(&issues, &["documents.terms.consent.version"]);
        }
    }

    #[test]
    fn a_policy_that_is_off_needs_nothing() {
        catalog(&with_policy(ConsentPolicy {
            requirement: Requirement::None,
            version: None,
            effective: Some("garbage".into()),
            grace_days: 365,
        }));
    }

    /// The schema publishes the `grace_days` range for every requirement, so the check cannot
    /// accept above it even where the value is unused.
    #[test]
    fn a_grace_period_is_at_most_a_year_even_when_off() {
        let issues = refused(&with_policy(ConsentPolicy {
            requirement: Requirement::None,
            version: None,
            effective: None,
            grace_days: 366,
        }));
        assert_issue(&issues, &["documents.terms.consent.grace_days", "365"]);
    }

    #[test]
    fn consent_on_an_external_document_is_refused() {
        let mut document = external("https://example.org/terms");
        document.consent = policy(Requirement::Accept);
        let issues = refused(&config(None, vec![("terms", document)]));
        assert_issue(&issues, &["documents.terms.consent.requirement", "hosted"]);
    }

    #[test]
    fn effective_has_to_be_a_date() {
        for bad in [
            "2026-13-01",
            "2025-02-29",
            "01.09.2026",
            "tomorrow",
            "2026-09",
            "",
        ] {
            let issues = refused(&with_policy(ConsentPolicy {
                effective: Some(bad.into()),
                ..policy(Requirement::Accept)
            }));
            assert_issue(&issues, &["documents.terms.consent.effective"]);
        }
    }

    /// ISO 8601 has more date forms than the documented one, and a year may carry a sign or
    /// more digits. Each is a valid date, but not one written as `YYYY-MM-DD`.
    #[test]
    fn effective_is_written_as_year_month_day() {
        for other in [
            "20260922",
            "2026-265",
            "2026-W39-2",
            "+2026-09-22",
            "-2026-09-22",
            "+002026-09-22",
            "2026-9-22",
            "2026-09-22T00:00",
        ] {
            let issues = refused(&with_policy(ConsentPolicy {
                effective: Some(other.into()),
                ..policy(Requirement::Accept)
            }));
            assert_issue(
                &issues,
                &["documents.terms.consent.effective", "YYYY-MM-DD"],
            );
        }
        for good in ["2026-09-22", "2024-02-29", " 2026-09-22\n"] {
            catalog(&with_policy(ConsentPolicy {
                effective: Some(good.into()),
                ..policy(Requirement::Accept)
            }));
        }
    }

    #[test]
    fn a_grace_period_needs_a_start() {
        let issues = refused(&with_policy(ConsentPolicy {
            effective: None,
            ..policy(Requirement::Accept)
        }));
        assert_issue(
            &issues,
            &["documents.terms.consent.grace_days", "effective"],
        );

        catalog(&with_policy(ConsentPolicy {
            effective: None,
            grace_days: 0,
            ..policy(Requirement::Accept)
        }));
    }

    #[test]
    fn a_grace_period_is_at_most_a_year() {
        catalog(&with_policy(ConsentPolicy {
            grace_days: 365,
            ..policy(Requirement::Accept)
        }));
        let issues = refused(&with_policy(ConsentPolicy {
            grace_days: 366,
            ..policy(Requirement::Accept)
        }));
        assert_issue(&issues, &["documents.terms.consent.grace_days", "365"]);
    }
}

mod rules {
    use super::*;
    use terrace_config::schema::{Refine, Refinement};
    use terrace_legal::{
        CatalogBuilder, ConfigIssue, Legal, LegalConfig, LegalDocument, RequiredDocuments, Rule,
    };

    /// Refuses a title that a footer cannot hold, and a document without a German text.
    struct FooterRules;

    impl Rule for FooterRules {
        fn check_document(&self, _slug: &str, document: &LegalDocument) -> Vec<ConfigIssue> {
            let mut issues = Vec::new();
            for (locale, title) in &document.title {
                if title.len() > 12 {
                    issues.push(ConfigIssue::new(["title", locale.as_str()], "is too long"));
                }
            }
            if !document.body.is_empty() && !document.body.keys().any(|key| key == "de") {
                issues.push(ConfigIssue::new(["body"], "has no German text"));
            }
            issues
        }
    }

    #[test]
    fn a_required_document_that_is_missing_is_refused_by_slug() {
        let builder = Catalog::builder().rule(RequiredDocuments::new(["terms", "imprint"]));
        let config = config(None, vec![("terms", hosted(&[("en", "x")]))]);
        let issues = builder.build(&config).expect_err("imprint is missing");
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues.iter().next().map(ConfigIssue::key).as_deref(),
            Some("documents.imprint")
        );
    }

    #[test]
    fn a_document_rule_reports_under_the_documents_key_and_together_with_built_in_issues() {
        let builder = Catalog::builder().rule(FooterRules);
        let config = config(
            None,
            vec![
                (
                    "terms",
                    titled(hosted(&[("en", "x")]), &[("en", "A very long title")]),
                ),
                ("blank", hosted(&[("de", "")])),
            ],
        );
        let issues = builder.build(&config).expect_err("refused");
        let lines: Vec<String> = issues.iter().map(ToString::to_string).collect();
        assert_issue(&lines, &["documents.terms.title.en", "too long"]);
        assert_issue(&lines, &["documents.terms.body", "no German text"]);
        assert_issue(&lines, &["documents.blank.body.de", "blank"]);
        assert_eq!(lines.len(), 3, "{lines:?}");
    }

    #[test]
    fn rules_accept_a_configuration_they_have_no_quarrel_with() {
        let builder = Catalog::builder()
            .rule(FooterRules)
            .rule(RequiredDocuments::new(["terms"]));
        let ok = config(None, vec![("terms", hosted(&[("de", "x"), ("en", "y")]))]);
        assert_eq!(builder.build(&ok).expect("valid").len(), 1);
    }

    #[test]
    fn a_rule_outlives_the_first_configuration_through_replace() {
        let builder = CatalogBuilder::new().rule(RequiredDocuments::new(["terms"]));
        let first = config(None, vec![("terms", hosted(&[("en", "x")]))]);
        let legal = Legal::with_builder(builder.build(&first).expect("valid"), builder);

        let without = LegalConfig::default();
        assert!(
            legal.replace(&without).is_err(),
            "the rule judges the replacement too"
        );
        assert_eq!(
            legal.catalog().len(),
            1,
            "the previous catalog keeps serving"
        );
    }

    /// A host's own rule that states its check in the schema, exactly as strict as the check.
    struct ImprintRequired;

    impl Rule for ImprintRequired {
        fn check_config(&self, config: &LegalConfig) -> Vec<ConfigIssue> {
            if config.documents.contains_key("imprint") {
                Vec::new()
            } else {
                vec![ConfigIssue::new(["documents", "imprint"], "is required")]
            }
        }

        fn refinements(&self) -> Vec<(String, Refinement)> {
            vec![(
                "documents".to_owned(),
                Refinement::required_entries(["imprint"]),
            )]
        }
    }

    fn documents(slugs: &[&str]) -> (String, Refinement) {
        (
            "documents".to_owned(),
            Refinement::required_entries(slugs.iter().copied()),
        )
    }

    #[test]
    fn required_documents_publishes_exactly_its_slugs_at_documents() {
        let rule = RequiredDocuments::new(["terms", "privacy", "terms"]);
        assert_eq!(rule.refinements(), [documents(&["privacy", "terms"])]);
    }

    #[test]
    fn required_documents_without_slugs_still_names_its_key() {
        assert_eq!(
            RequiredDocuments::new(Vec::<String>::new()).refinements(),
            [documents(&[])]
        );
    }

    #[test]
    fn a_rule_that_states_nothing_publishes_nothing() {
        assert!(FooterRules.refinements().is_empty());
        assert!(
            CatalogBuilder::new()
                .rule(FooterRules)
                .refinements()
                .is_empty(),
            "neither the rule nor the built-in checks publish anything"
        );
    }

    #[test]
    fn the_builder_publishes_every_rules_refinements_in_registration_order() {
        let builder = Catalog::builder()
            .rule(RequiredDocuments::new(["terms"]))
            .rule(FooterRules)
            .rule(ImprintRequired)
            .rule(RequiredDocuments::new(["privacy", "terms"]));
        assert_eq!(
            builder.refinements(),
            [
                documents(&["terms"]),
                documents(&["imprint"]),
                documents(&["privacy", "terms"]),
            ]
        );
    }

    #[test]
    fn the_builder_carries_limits_and_rules_together() {
        let builder = Catalog::builder()
            .limits(terrace_legal::Limits { max_body_bytes: 2 })
            .rule(FooterRules);
        assert_eq!(builder.limits_in_force().max_body_bytes, 2);
        let config = config(None, vec![("t", hosted(&[("de", "abc")]))]);
        let issues = builder.build(&config).expect_err("over the limit");
        assert_eq!(issues.len(), 1, "the rule found nothing else to say");
    }
}
