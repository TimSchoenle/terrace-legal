//! A JSON Schema evaluator for exactly the keywords the rendered configuration schema uses.
//!
//! No validator is a dependency of this workspace, and the tests need to know what a published
//! schema accepts rather than what it spells. An evaluator that skipped a keyword it did not know
//! would accept too much and let a test pass on a schema it never checked, so any keyword outside
//! the implemented set panics, naming it. A new keyword in the rendered schema therefore fails the
//! tests until it is implemented here.

use serde_json::{Map, Value};

/// Annotations: they describe a value and never decide whether it is accepted.
const ANNOTATIONS: &[&str] = &["$schema", "description", "title", "default", "examples"];

/// Whether `instance` is valid against `schema`.
///
/// # Panics
///
/// On a schema that is neither a boolean nor an object, and on any assertion keyword this
/// evaluator does not implement.
pub(crate) fn accepts(schema: &Value, instance: &Value) -> bool {
    let keywords = match schema {
        Value::Bool(accepted) => return *accepted,
        Value::Object(keywords) => keywords,
        other => panic!("{other} is not a schema"),
    };
    keywords
        .iter()
        .all(|(keyword, argument)| keyword_accepts(keywords, keyword, argument, instance))
}

fn keyword_accepts(
    keywords: &Map<String, Value>,
    keyword: &str,
    argument: &Value,
    instance: &Value,
) -> bool {
    match keyword {
        _ if ANNOTATIONS.contains(&keyword) => true,
        "type" => has_type(argument, instance),
        "enum" => argument
            .as_array()
            .expect("`enum` is an array")
            .contains(instance),
        "minimum" => instance
            .as_f64()
            .is_none_or(|number| number >= argument.as_f64().expect("`minimum` is a number")),
        "maximum" => instance
            .as_f64()
            .is_none_or(|number| number <= argument.as_f64().expect("`maximum` is a number")),
        "required" => instance.as_object().is_none_or(|object| {
            argument
                .as_array()
                .expect("`required` is an array")
                .iter()
                .all(|name| object.contains_key(name.as_str().expect("a required name")))
        }),
        "properties" => instance.as_object().is_none_or(|object| {
            let properties = argument.as_object().expect("`properties` is an object");
            object.iter().all(|(name, value)| {
                properties
                    .get(name)
                    .is_none_or(|property| accepts(property, value))
            })
        }),
        "additionalProperties" => instance.as_object().is_none_or(|object| {
            let declared = keywords.get("properties").and_then(Value::as_object);
            object
                .iter()
                .filter(|(name, _)| declared.is_none_or(|declared| !declared.contains_key(*name)))
                .all(|(_, value)| accepts(argument, value))
        }),
        other => panic!("the evaluator does not implement `{other}`; add it before relying on it"),
    }
}

fn has_type(argument: &Value, instance: &Value) -> bool {
    let name = argument.as_str().expect("`type` is a single name here");
    match name {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "boolean" => instance.is_boolean(),
        "null" => instance.is_null(),
        "number" => instance.is_number(),
        "integer" => instance.is_i64() || instance.is_u64(),
        other => panic!("`{other}` is not a JSON Schema type"),
    }
}
