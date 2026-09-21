//! A small document tree built from Markdown events.
//!
//! The tree exists so that everything that decides what may appear in the output, which is the
//! security-relevant part, is plain data that a test can inspect without rendering anything.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

use crate::href::{is_external, is_safe_href};

/// A block or inline element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Heading(u8),
    Paragraph,
    BlockQuote,
    CodeBlock,
    Code,
    Emphasis,
    Strong,
    Strikethrough,
    OrderedList(u64),
    UnorderedList,
    Item,
    Table,
    TableHead,
    TableBody,
    TableRow,
    HeaderCell,
    Cell,
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    /// Literal text, escaped by the renderer.
    Text(String),
    /// A hard line break.
    Break,
    /// A thematic break.
    Rule,
    /// An element with children.
    Element(Kind, Vec<Node>),
    /// An anchor whose `href` passed [`is_safe_href`].
    Link {
        href: String,
        external: bool,
        children: Vec<Node>,
    },
}

/// What an open Markdown container is collecting.
enum Frame {
    Element(Kind),
    Link(String),
    Image,
    /// Raw HTML, which is rendered as a paragraph of literal text.
    HtmlBlock,
    /// A container that renders as its children only: an unsafe link, or a footnote body.
    Transparent,
}

/// Parses `source` into a tree.
///
/// Raw HTML never becomes markup: it is text. An image becomes its alt text, and a link with an
/// unsafe target becomes its own text.
pub(crate) fn parse(source: &str) -> Vec<Node> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;

    let mut stack: Vec<(Frame, Vec<Node>)> = Vec::new();
    let mut root: Vec<Node> = Vec::new();

    fn sink<'a>(stack: &'a mut [(Frame, Vec<Node>)], root: &'a mut Vec<Node>) -> &'a mut Vec<Node> {
        match stack.last_mut() {
            Some((_, children)) => children,
            None => root,
        }
    }

    for event in Parser::new_ext(source, options) {
        match event {
            Event::Start(tag) => stack.push((frame_for(&tag), Vec::new())),
            Event::End(_) => {
                let Some((frame, children)) = stack.pop() else {
                    continue;
                };
                let parent = sink(&mut stack, &mut root);
                close(frame, children, parent);
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                push_text(sink(&mut stack, &mut root), text.into_string());
            }
            Event::Code(text) => sink(&mut stack, &mut root).push(Node::Element(
                Kind::Code,
                vec![Node::Text(text.into_string())],
            )),
            Event::SoftBreak => push_text(sink(&mut stack, &mut root), " ".to_owned()),
            Event::HardBreak => sink(&mut stack, &mut root).push(Node::Break),
            Event::Rule => sink(&mut stack, &mut root).push(Node::Rule),
            Event::TaskListMarker(checked) => push_text(
                sink(&mut stack, &mut root),
                if checked { "[x] " } else { "[ ] " }.to_owned(),
            ),
            // Not enabled by the options above, so the parser does not emit them; an arm is
            // still needed and text is the safe reading of anything unforeseen.
            Event::FootnoteReference(label) => {
                push_text(sink(&mut stack, &mut root), format!("[^{label}]"));
            }
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                push_text(sink(&mut stack, &mut root), text.into_string());
            }
        }
    }
    // An unbalanced stream cannot come from the parser, but if it did the open containers are
    // folded into the root rather than dropped.
    while let Some((_, children)) = stack.pop() {
        sink(&mut stack, &mut root).extend(children);
    }
    root
}

fn frame_for(tag: &Tag<'_>) -> Frame {
    match tag {
        Tag::Paragraph => Frame::Element(Kind::Paragraph),
        Tag::Heading { level, .. } => Frame::Element(Kind::Heading(heading_level(*level))),
        Tag::BlockQuote(_) => Frame::Element(Kind::BlockQuote),
        Tag::CodeBlock(_) => Frame::Element(Kind::CodeBlock),
        Tag::List(Some(start)) => Frame::Element(Kind::OrderedList(*start)),
        Tag::List(None) => Frame::Element(Kind::UnorderedList),
        Tag::Item => Frame::Element(Kind::Item),
        Tag::Table(_) => Frame::Element(Kind::Table),
        Tag::TableHead => Frame::Element(Kind::TableHead),
        Tag::TableRow => Frame::Element(Kind::TableRow),
        // The cell kind depends on whether the head or the body holds it, so it is settled when the
        // cell closes.
        Tag::TableCell => Frame::Element(Kind::Cell),
        Tag::Emphasis => Frame::Element(Kind::Emphasis),
        Tag::Strong => Frame::Element(Kind::Strong),
        Tag::Strikethrough => Frame::Element(Kind::Strikethrough),
        Tag::Link { dest_url, .. } => Frame::Link(dest_url.to_string()),
        Tag::Image { .. } => Frame::Image,
        Tag::HtmlBlock => Frame::HtmlBlock,
        _ => Frame::Transparent,
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn close(frame: Frame, children: Vec<Node>, parent: &mut Vec<Node>) {
    match frame {
        Frame::Element(Kind::TableHead) => {
            // The parser puts the header cells directly in the head, without a row.
            let cells = children
                .into_iter()
                .map(|node| match node {
                    Node::Element(Kind::Cell, cell) => Node::Element(Kind::HeaderCell, cell),
                    other => other,
                })
                .collect();
            parent.push(Node::Element(
                Kind::TableHead,
                vec![Node::Element(Kind::TableRow, cells)],
            ));
        }
        Frame::Element(Kind::Table) => {
            let mut head = Vec::new();
            let mut body = Vec::new();
            for child in children {
                match child {
                    Node::Element(Kind::TableHead, _) => head.push(child),
                    other => body.push(other),
                }
            }
            let mut table = head;
            if !body.is_empty() {
                table.push(Node::Element(Kind::TableBody, body));
            }
            parent.push(Node::Element(Kind::Table, table));
        }
        Frame::Element(kind) => parent.push(Node::Element(kind, children)),
        Frame::Link(href) => {
            if is_safe_href(&href) {
                let external = is_external(&href);
                parent.push(Node::Link {
                    href,
                    external,
                    children,
                });
            } else {
                parent.extend(children);
            }
        }
        Frame::Image => {
            // No remote fetch from an operator's document: privacy, and the page's CSP.
            let alt = plain_text(&children);
            if !alt.is_empty() {
                push_text(parent, alt);
            }
        }
        Frame::HtmlBlock => parent.push(Node::Element(Kind::Paragraph, children)),
        Frame::Transparent => parent.extend(children),
    }
}

/// Appends text, merging it into a preceding text node so the output has fewer nodes.
fn push_text(children: &mut Vec<Node>, text: String) {
    if let Some(Node::Text(previous)) = children.last_mut() {
        previous.push_str(&text);
    } else {
        children.push(Node::Text(text));
    }
}

/// Flattens nodes to their text, for an image's alt text.
fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Break => out.push(' '),
            Node::Rule => {}
            Node::Element(_, children) | Node::Link { children, .. } => {
                out.push_str(&plain_text(children));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(node: &Node) -> String {
        plain_text(std::slice::from_ref(node))
    }

    #[test]
    fn raw_html_is_text_never_markup() {
        let tree = parse("<script>alert(1)</script>\n\nhello <b>bold</b> world");
        let joined: String = tree.iter().map(text).collect();
        assert!(joined.contains("<script>alert(1)</script>"));
        assert!(joined.contains("<b>bold</b>"));
        fn only_text_and_paragraphs(nodes: &[Node]) -> bool {
            nodes.iter().all(|node| match node {
                Node::Text(_) => true,
                Node::Element(Kind::Paragraph, children) => only_text_and_paragraphs(children),
                _ => false,
            })
        }
        assert!(only_text_and_paragraphs(&tree), "{tree:?}");
    }

    #[test]
    fn an_image_is_its_alt_text() {
        let tree = parse("![a tracking pixel](https://tracker.example/p.png)");
        assert_eq!(
            tree,
            vec![Node::Element(
                Kind::Paragraph,
                vec![Node::Text("a tracking pixel".into())]
            )]
        );
        let empty = parse("![](https://tracker.example/p.png)");
        assert_eq!(empty, vec![Node::Element(Kind::Paragraph, vec![])]);
    }

    #[test]
    fn a_link_with_an_unsafe_target_is_its_text() {
        let tree = parse("[click](javascript:alert(1))");
        assert_eq!(
            tree,
            vec![Node::Element(
                Kind::Paragraph,
                vec![Node::Text("click".into())]
            )]
        );
    }

    #[test]
    fn a_safe_link_keeps_its_target_and_knows_whether_it_is_external() {
        let tree = parse("[a](https://example.org) [b](/privacy) [c](mailto:x@y.z)");
        let Node::Element(Kind::Paragraph, children) = &tree[0] else {
            panic!("a paragraph: {tree:?}");
        };
        let links: Vec<(&str, bool)> = children
            .iter()
            .filter_map(|node| match node {
                Node::Link { href, external, .. } => Some((href.as_str(), *external)),
                _ => None,
            })
            .collect();
        assert_eq!(
            links,
            [
                ("https://example.org", true),
                ("/privacy", false),
                ("mailto:x@y.z", false)
            ]
        );
    }

    #[test]
    fn a_table_has_a_head_row_of_header_cells_and_a_body() {
        let tree = parse("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        let Node::Element(Kind::Table, parts) = &tree[0] else {
            panic!("a table: {tree:?}");
        };
        assert!(matches!(&parts[0], Node::Element(Kind::TableHead, rows)
            if matches!(&rows[0], Node::Element(Kind::TableRow, cells)
                if cells.iter().all(|c| matches!(c, Node::Element(Kind::HeaderCell, _))))));
        let Node::Element(Kind::TableBody, rows) = &parts[1] else {
            panic!("a body: {parts:?}");
        };
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_table_without_rows_has_no_body() {
        let tree = parse("| a |\n|---|\n");
        let Node::Element(Kind::Table, parts) = &tree[0] else {
            panic!("a table");
        };
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn lists_headings_and_quotes_keep_their_structure() {
        let tree = parse("# T\n\n3. a\n4. b\n\n- x\n\n> q\n\n---\n");
        assert!(matches!(&tree[0], Node::Element(Kind::Heading(1), _)));
        assert!(matches!(&tree[1], Node::Element(Kind::OrderedList(3), items) if items.len() == 2));
        assert!(matches!(&tree[2], Node::Element(Kind::UnorderedList, _)));
        assert!(matches!(&tree[3], Node::Element(Kind::BlockQuote, _)));
        assert_eq!(tree[4], Node::Rule);
    }

    #[test]
    fn breaks_and_task_markers_are_preserved() {
        let tree = parse("a\\\nb\n\n- [x] done\n- [ ] todo\n");
        let Node::Element(Kind::Paragraph, children) = &tree[0] else {
            panic!("a paragraph");
        };
        assert!(children.contains(&Node::Break));
        let joined: String = tree.iter().map(text).collect();
        assert!(joined.contains("[x] done") && joined.contains("[ ] todo"));
    }

    #[test]
    fn a_code_block_keeps_its_text_verbatim() {
        let tree = parse("```\n<b>&amp;</b>\n```\n");
        assert_eq!(
            tree,
            vec![Node::Element(
                Kind::CodeBlock,
                vec![Node::Text("<b>&amp;</b>\n".into())]
            )]
        );
    }

    #[test]
    fn empty_input_is_an_empty_tree() {
        assert!(parse("").is_empty());
        assert!(parse("   \n\n  ").is_empty());
    }
}
