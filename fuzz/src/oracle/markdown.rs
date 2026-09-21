//! Oracle for rendering untrusted Markdown.

use dioxus::prelude::*;
use terrace_legal_markdown::{is_safe_href, markdown};

/// Elements and attributes that would let a document act instead of read.
const FORBIDDEN_ELEMENTS: [&str; 12] = [
    "script", "iframe", "object", "embed", "img", "style", "link", "meta", "form", "svg", "base",
    "template",
];
const FORBIDDEN_ATTRIBUTES: [&str; 6] =
    ["src", "srcset", "style", "action", "formaction", "srcdoc"];

/// Renders arbitrary text as Markdown and checks that nothing in the output can execute.
///
/// # Panics
///
/// When rendering panics, or when the output holds a forbidden element, an event-handler or
/// forbidden attribute, an unsafe `href`, or an external link without `rel="noopener noreferrer"`.
pub fn check(data: &[u8]) {
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };
    let source = source.to_owned();
    let html = dioxus_ssr::render_element(rsx! { div { {markdown(&source)} } });

    // Text is escaped by the renderer, so a `<` that is not a tag never appears in the output.
    let mut rest = html.as_str();
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let end = after.find('>').expect("every tag is closed");
        let inner = after[..end].trim_end_matches('/').trim();
        rest = &after[end + 1..];
        if inner.starts_with('/') {
            continue;
        }
        let (name, attributes) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        assert!(
            !FORBIDDEN_ELEMENTS.contains(&name.to_ascii_lowercase().as_str()),
            "forbidden element <{name}> in {html}"
        );
        check_attributes(name, attributes, &html);
    }
}

fn check_attributes(name: &str, mut text: &str, html: &str) {
    let mut external = false;
    let mut noopener = false;
    loop {
        text = text.trim_start();
        if text.is_empty() {
            break;
        }
        let Some((attribute, after)) = text.split_once('=') else {
            break;
        };
        let attribute = attribute.trim().to_ascii_lowercase();
        let after = after
            .strip_prefix('"')
            .expect("attribute values are quoted");
        let end = after.find('"').expect("a quote closes the value");
        let value = &after[..end];
        text = &after[end + 1..];

        assert!(
            !attribute.starts_with("on"),
            "event handler {attribute} in {html}"
        );
        assert!(
            !FORBIDDEN_ATTRIBUTES.contains(&attribute.as_str()),
            "forbidden attribute {attribute} on <{name}> in {html}"
        );
        if attribute == "href" {
            assert!(is_safe_href(value), "unsafe href {value:?} in {html}");
            let lower = value.trim_start().to_ascii_lowercase();
            external = lower.starts_with("http:") || lower.starts_with("https:");
        }
        if attribute == "rel" && value == "noopener noreferrer" {
            noopener = true;
        }
    }
    if name == "a" && external {
        assert!(noopener, "an external link without noopener in {html}");
    }
}
