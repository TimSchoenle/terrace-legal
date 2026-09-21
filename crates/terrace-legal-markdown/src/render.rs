//! Turning the document tree into Dioxus elements.

use dioxus::prelude::*;

use crate::tree::{Kind, Node, parse};

/// Renders Markdown source to elements.
///
/// The result is a fragment, not a wrapper: the caller chooses the container, and with it the
/// typography. Empty or blank source renders nothing.
///
/// # Errors
///
/// Never returns the error case: Dioxus spells a renderable value as a `Result`, and rendering
/// a parsed document has no way to fail.
pub fn markdown(source: &str) -> Element {
    let nodes = parse(source);
    rsx! {
        for node in nodes.iter() {
            {render(node)}
        }
    }
}

fn render_all(nodes: &[Node]) -> Element {
    rsx! {
        for node in nodes.iter() {
            {render(node)}
        }
    }
}

fn render(node: &Node) -> Element {
    match node {
        Node::Text(text) => rsx! { "{text}" },
        Node::Break => rsx! { br {} },
        Node::Rule => rsx! { hr {} },
        Node::Link {
            href,
            external: true,
            children,
        } => rsx! {
            a { href: "{href}", target: "_blank", rel: "noopener noreferrer", {render_all(children)} }
        },
        Node::Link {
            href,
            external: false,
            children,
        } => rsx! {
            a { href: "{href}", {render_all(children)} }
        },
        Node::Element(kind, children) => render_element(*kind, children),
    }
}

fn render_element(kind: Kind, children: &[Node]) -> Element {
    let inner = render_all(children);
    match kind {
        Kind::Heading(1) => rsx! { h1 { {inner} } },
        Kind::Heading(2) => rsx! { h2 { {inner} } },
        Kind::Heading(3) => rsx! { h3 { {inner} } },
        Kind::Heading(4) => rsx! { h4 { {inner} } },
        Kind::Heading(5) => rsx! { h5 { {inner} } },
        Kind::Heading(_) => rsx! { h6 { {inner} } },
        Kind::Paragraph => rsx! { p { {inner} } },
        Kind::BlockQuote => rsx! { blockquote { {inner} } },
        Kind::CodeBlock => rsx! { pre { code { {inner} } } },
        Kind::Code => rsx! { code { {inner} } },
        Kind::Emphasis => rsx! { em { {inner} } },
        Kind::Strong => rsx! { strong { {inner} } },
        Kind::Strikethrough => rsx! { del { {inner} } },
        Kind::OrderedList(1) => rsx! { ol { {inner} } },
        Kind::OrderedList(start) => rsx! { ol { start: "{start}", {inner} } },
        Kind::UnorderedList => rsx! { ul { {inner} } },
        Kind::Item => rsx! { li { {inner} } },
        Kind::Table => rsx! { table { {inner} } },
        Kind::TableHead => rsx! { thead { {inner} } },
        Kind::TableBody => rsx! { tbody { {inner} } },
        Kind::TableRow => rsx! { tr { {inner} } },
        Kind::HeaderCell => rsx! { th { {inner} } },
        Kind::Cell => rsx! { td { {inner} } },
    }
}
