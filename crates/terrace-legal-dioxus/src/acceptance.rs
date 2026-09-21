//! A sentence that links to the documents a reader is agreeing to.

use dioxus::prelude::*;

use crate::context::{LegalContext, entry_link};
use crate::traits::Part;

/// One piece of a template: literal text, or a `{slug}` placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    Text(String),
    Slot(String),
}

/// Splits `template` at its `{slug}` placeholders.
///
/// A `{` that does not open a placeholder, because it has no closing `}` or holds nothing usable
/// between the braces, is literal text.
pub(crate) fn split_template(template: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if is_slot_name(&after[..close]) => {
                if !text.is_empty() {
                    segments.push(Segment::Text(std::mem::take(&mut text)));
                }
                segments.push(Segment::Slot(after[..close].to_owned()));
                rest = &after[close + 1..];
            }
            _ => {
                text.push('{');
                rest = after;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        segments.push(Segment::Text(text));
    }
    segments
}

fn is_slot_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// The properties of [`AcceptanceSentence`].
#[derive(Props, Clone, PartialEq)]
pub struct AcceptanceSentenceProps {
    /// One string from the application's catalogue, with a `{slug}` placeholder where each link
    /// goes, for example `By registering you accept the {terms} and the {privacy}.` A translation
    /// can put the placeholders in any order.
    pub template: String,
    /// The documents the sentence names. The sentence is shown only when every one of them is
    /// published.
    pub slugs: Vec<String>,
}

/// A sentence with a link to each named document in place of its placeholder.
///
/// It renders nothing unless every slug in `slugs` is published, because a sentence that asks a
/// reader to accept a document they cannot open is worse than none. A slug with no placeholder in
/// the template loses its link and the sentence stays. A placeholder that names a slug outside
/// `slugs` is shown as written.
#[component]
pub fn AcceptanceSentence(props: AcceptanceSentenceProps) -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    let index = context.index.read();
    let Some(entries) = index.as_ref() else {
        return VNode::empty();
    };
    let published = |slug: &str| entries.iter().find(|entry| entry.slug == slug);
    if props.slugs.iter().any(|slug| published(slug).is_none()) {
        return VNode::empty();
    }

    let segments = split_template(&props.template);
    let view = |segment: &Segment| -> Element {
        match segment {
            Segment::Slot(slug) if props.slugs.contains(slug) => published(slug)
                .map_or_else(VNode::empty, |entry| {
                    entry_link(&context, entry, Part::Link)
                }),
            Segment::Slot(slug) => rsx! { "{{{slug}}}" },
            Segment::Text(text) => rsx! { "{text}" },
        }
    };
    rsx! {
        for segment in segments.iter() {
            {view(segment)}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(text: &str) -> Segment {
        Segment::Text(text.to_owned())
    }

    fn slot(name: &str) -> Segment {
        Segment::Slot(name.to_owned())
    }

    #[test]
    fn placeholders_split_the_template() {
        assert_eq!(
            split_template("Accept {terms} and {privacy}."),
            [
                text("Accept "),
                slot("terms"),
                text(" and "),
                slot("privacy"),
                text(".")
            ]
        );
    }

    #[test]
    fn a_translation_can_reorder_the_placeholders() {
        assert_eq!(
            split_template("{privacy} und {terms}"),
            [slot("privacy"), text(" und "), slot("terms")]
        );
    }

    #[test]
    fn a_brace_that_opens_nothing_is_literal() {
        for template in [
            "a { b",
            "a {} b",
            "a {Terms} b",
            "a {two words} b",
            "a } b",
            "{",
            "}",
            "{{terms",
        ] {
            let rebuilt: String = split_template(template)
                .iter()
                .map(|segment| match segment {
                    Segment::Text(text) => text.clone(),
                    Segment::Slot(name) => format!("{{{name}}}"),
                })
                .collect();
            assert_eq!(rebuilt, template, "template {template:?} must round-trip");
        }
        assert_eq!(
            split_template("{{terms}}"),
            [text("{"), slot("terms"), text("}")]
        );
    }

    #[test]
    fn edge_templates() {
        assert!(split_template("").is_empty());
        assert_eq!(split_template("{terms}"), [slot("terms")]);
        assert_eq!(split_template("no placeholders"), [text("no placeholders")]);
    }
}
