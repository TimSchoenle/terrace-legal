//! What a host's published schema says once it is refined with the catalog builder.
//!
//! The rule and the schema come from one builder value; these tests hold the schema to the rule's
//! runtime verdict, through the JSON Schema a deployment tool actually validates against.
#![cfg(feature = "testing")]

mod common;

use std::collections::BTreeSet;

use common::json_schema::accepts;
use proptest::prelude::*;
use serde_json::{Value, json};
use terrace_config::Dialect;
use terrace_config::schema::Schema;
use terrace_legal::testing::{HostConfig, LegalFixture};
use terrace_legal::{CatalogBuilder, LegalConfig, RequiredDocuments, Rule};

/// The host's schema as it publishes it: defaults observed, then refined under `legal`.
fn published(builder: &CatalogBuilder) -> Schema {
    Schema::describe::<HostConfig>(&Dialect::new(LegalFixture::PREFIX))
        .with_defaults_from(&HostConfig::default())
        .expect("defaults serialise")
        .refine_with("legal", builder)
        .expect("the builder's refinements apply")
}

fn json_schema(schema: &Schema) -> Value {
    serde_json::from_str(&schema.to_json_schema().expect("renders")).expect("valid JSON")
}

fn host(documents: &Value) -> Value {
    json!({ "legal": { "documents": documents } })
}

mod end_to_end {
    use super::*;

    fn builder() -> CatalogBuilder {
        CatalogBuilder::new().rule(RequiredDocuments::new(["terms", "privacy"]))
    }

    #[test]
    fn the_rendered_schema_refuses_a_section_without_the_required_documents() {
        let schema = json_schema(&published(&builder()));
        assert!(!accepts(&schema, &host(&json!({}))));
        assert!(!accepts(
            &schema,
            &host(&json!({ "terms": { "body": { "en": "x" } } }))
        ));
        assert!(!accepts(&schema, &json!({ "legal": {} })));
        assert!(!accepts(&schema, &json!({})));
    }

    #[test]
    fn the_rendered_schema_accepts_a_section_with_the_required_documents() {
        let schema = json_schema(&published(&builder()));
        let documents = json!({
            "terms": { "body": { "en": "# Terms" } },
            "privacy": { "url": "https://example.org/privacy" },
            "imprint": { "body": { "de": "# Impressum" } },
        });
        assert!(accepts(&schema, &host(&documents)));
    }

    #[test]
    fn the_required_documents_are_published_in_the_documents_constraint() {
        let schema = json_schema(&published(&builder()));
        let documents = &schema["properties"]["legal"]["properties"]["documents"];
        assert_eq!(documents["required"], json!(["privacy", "terms"]));
        assert_eq!(
            documents["additionalProperties"]["type"], "object",
            "the map stays open to documents nobody requires"
        );
    }

    #[test]
    fn several_rules_publish_the_union_of_their_entries() {
        let builder = CatalogBuilder::new()
            .rule(RequiredDocuments::new(["terms"]))
            .rule(RequiredDocuments::new(["privacy", "terms"]));
        let schema = json_schema(&published(&builder));
        assert_eq!(
            schema["properties"]["legal"]["properties"]["documents"]["required"],
            json!(["privacy", "terms"])
        );
    }

    #[test]
    fn a_refined_documents_key_is_required_and_has_no_default() {
        let schema = published(&builder());
        let key = schema
            .keys
            .iter()
            .find(|key| key.path == "legal.documents")
            .expect("the documents key");
        assert!(key.required, "an empty map would be refused at boot");
        assert_eq!(key.default, None);
        assert_eq!(key.default_value, None);
    }

    #[test]
    fn a_builder_without_refinements_publishes_the_derived_schema_unchanged() {
        let dialect = Dialect::new(LegalFixture::PREFIX);
        let derived = Schema::describe::<HostConfig>(&dialect)
            .with_defaults_from(&HostConfig::default())
            .expect("defaults serialise");
        let refined = published(&CatalogBuilder::new());
        assert_eq!(
            refined.to_json().expect("renders"),
            derived.to_json().expect("renders")
        );
        assert!(accepts(&json_schema(&refined), &host(&json!({}))));
    }
}

/// The schema's verdict on `documents` is the rule's verdict, for every map.
mod parity {
    use super::*;

    /// Valid slugs, including both separators a slug may contain, so the check and the schema are
    /// compared on names the environment layer spells differently from the key.
    const SLUGS: &[&str] = &[
        "terms",
        "privacy",
        "imprint",
        "cookie-policy",
        "data_use",
        "0",
    ];

    /// One document of either kind: the rule counts an external document as published.
    fn document() -> impl Strategy<Value = Value> {
        prop_oneof![
            Just(json!({ "body": { "en": "# Text" } })),
            Just(json!({ "url": "https://example.org/elsewhere" })),
        ]
    }

    fn documents() -> impl Strategy<Value = Value> {
        prop::collection::btree_map(prop::sample::select(SLUGS), document(), 0..=SLUGS.len())
            .prop_map(|map| {
                Value::Object(
                    map.into_iter()
                        .map(|(slug, document)| (slug.to_owned(), document))
                        .collect(),
                )
            })
    }

    fn required() -> impl Strategy<Value = BTreeSet<&'static str>> {
        prop::collection::btree_set(prop::sample::select(SLUGS), 0..=SLUGS.len())
    }

    /// The schema accepts and the rule passes, or the schema rejects and the rule refuses.
    fn assert_parity(required: &BTreeSet<&str>, documents: &Value) {
        let rule = RequiredDocuments::new(required.iter().copied());
        let schema = json_schema(&published(&CatalogBuilder::new().rule(rule.clone())));
        let instance = host(documents);
        let config: LegalConfig =
            serde_json::from_value(json!({ "documents": documents })).expect("a valid section");

        // Every generated map is valid for the unrefined schema, so a rejection below can only
        // come from the refinement.
        assert!(accepts(
            &json_schema(&published(&CatalogBuilder::new())),
            &instance
        ));
        assert_eq!(
            accepts(&schema, &instance),
            rule.check_config(&config).is_empty(),
            "required {required:?}, documents {documents}"
        );
    }

    #[test]
    fn the_boundary_cases_agree() {
        let hosted = json!({ "body": { "en": "x" } });
        let external = json!({ "url": "https://example.org/x" });
        let cases: &[(&[&str], Value)] = &[
            (&[], json!({})),
            (&[], json!({ "terms": hosted })),
            (&["terms"], json!({})),
            (&["terms"], json!({ "terms": hosted })),
            (&["terms"], json!({ "terms": external })),
            (&["terms"], json!({ "privacy": hosted })),
            (&["terms", "privacy"], json!({ "terms": hosted })),
            (
                &["terms", "privacy"],
                json!({ "terms": hosted, "privacy": external }),
            ),
            (&["cookie-policy"], json!({ "cookie_policy": hosted })),
            (&["cookie-policy"], json!({ "cookie-policy": hosted })),
        ];
        for (required, documents) in cases {
            assert_parity(&required.iter().copied().collect(), documents);
        }
    }

    proptest! {
        #[test]
        fn schema_acceptance_matches_the_rule(required in required(), documents in documents()) {
            assert_parity(&required, &documents);
        }
    }
}

/// The evaluator the tests above rely on, held to the keywords it claims.
mod evaluator {
    use super::*;

    #[test]
    fn it_applies_required_properties_and_closed_objects() {
        let schema = json!({
            "type": "object",
            "properties": { "a": { "type": "integer", "minimum": 0, "maximum": 2 } },
            "additionalProperties": false,
            "required": ["a"],
        });
        assert!(accepts(&schema, &json!({ "a": 1 })));
        assert!(!accepts(&schema, &json!({})));
        assert!(!accepts(&schema, &json!({ "a": 3 })));
        assert!(!accepts(&schema, &json!({ "a": 1, "b": 1 })));
        assert!(!accepts(&schema, &json!([])));
    }

    #[test]
    fn it_applies_an_element_schema_and_enum() {
        let schema = json!({
            "type": "object",
            "additionalProperties": { "enum": ["x", "y"] },
        });
        assert!(accepts(&schema, &json!({ "k": "x" })));
        assert!(!accepts(&schema, &json!({ "k": "z" })));
    }

    #[test]
    #[should_panic(expected = "does not implement `pattern`")]
    fn it_refuses_to_guess_at_a_keyword_it_does_not_implement() {
        accepts(&json!({ "pattern": "^a$" }), &json!("a"));
    }
}
