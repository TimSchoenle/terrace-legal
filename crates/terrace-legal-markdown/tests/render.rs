//! What the rendered output contains, checked on the serialised HTML.

use dioxus::prelude::*;
use terrace_legal_markdown::{is_safe_href, markdown};

fn render(source: &str) -> String {
    let source = source.to_owned();
    dioxus_ssr::render_element(rsx! { div { {markdown(&source)} } })
}

type Attributes = Vec<(String, String)>;

/// Every tag in `html` as `(name, attributes)`. Text is escaped by the renderer, so a `<` that is
/// not a tag never appears.
fn tags(html: &str) -> Vec<(String, Attributes)> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        let after = &rest[start + 1..];
        let end = after.find('>').expect("a tag is closed");
        let inner = after[..end].trim_end_matches('/').trim();
        rest = &after[end + 1..];
        if inner.starts_with('/') || inner.starts_with('!') {
            continue;
        }
        let (name, attributes) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        out.push((name.to_owned(), parse_attributes(attributes)));
    }
    out
}

fn parse_attributes(mut text: &str) -> Attributes {
    let mut out = Vec::new();
    loop {
        text = text.trim_start();
        if text.is_empty() {
            return out;
        }
        let Some((name, rest)) = text.split_once('=') else {
            out.push((text.to_owned(), String::new()));
            return out;
        };
        let rest = rest.strip_prefix('"').expect("attribute values are quoted");
        let end = rest.find('"').expect("a quote closes the value");
        out.push((name.trim().to_owned(), rest[..end].to_owned()));
        text = &rest[end + 1..];
    }
}

fn names(html: &str) -> Vec<String> {
    tags(html).into_iter().map(|(name, _)| name).collect()
}

/// No script element, no event-handler attribute, and only allowed `href` targets.
fn assert_inert(html: &str) {
    for (name, attributes) in tags(html) {
        assert!(
            !matches!(
                name.as_str(),
                "script"
                    | "iframe"
                    | "object"
                    | "embed"
                    | "img"
                    | "style"
                    | "link"
                    | "meta"
                    | "form"
                    | "svg"
            ),
            "element <{name}> in {html}"
        );
        for (attribute, value) in attributes {
            assert!(
                !attribute.to_ascii_lowercase().starts_with("on"),
                "attribute {attribute} in {html}"
            );
            assert!(
                !matches!(
                    attribute.as_str(),
                    "src" | "srcset" | "style" | "action" | "formaction" | "srcdoc"
                ),
                "attribute {attribute} in {html}"
            );
            if attribute == "href" {
                assert!(is_safe_href(&value), "unsafe href {value:?} in {html}");
            }
        }
    }
}

#[test]
fn semantic_elements_are_produced_and_no_class_is_written() {
    let html = render(
        "# One\n\n## Two\n\n###### Six\n\ntext with *em*, **strong**, ~~gone~~ and `code`\n\n\
         - a\n- b\n\n5. c\n6. d\n\n> quoted\n\n---\n\n```\nlet x = 1;\n```\n",
    );
    let found = names(&html);
    for expected in [
        "h1",
        "h2",
        "h6",
        "p",
        "em",
        "strong",
        "del",
        "code",
        "ul",
        "ol",
        "li",
        "blockquote",
        "hr",
        "pre",
    ] {
        assert!(
            found.iter().any(|n| n == expected),
            "missing <{expected}> in {html}"
        );
    }
    assert!(
        !html.contains("class"),
        "the host styles the output: {html}"
    );
    assert!(html.contains(r#"<ol start="5">"#), "{html}");
    assert!(
        !render("1. a\n2. b").contains("start"),
        "a list that starts at one needs no attribute"
    );
}

#[test]
fn a_table_renders_a_head_and_a_body() {
    let html = render("| a | b |\n|---|---|\n| 1 | 2 |\n");
    let found = names(&html);
    for expected in ["table", "thead", "tr", "th", "tbody", "td"] {
        assert!(
            found.iter().any(|n| n == expected),
            "missing <{expected}> in {html}"
        );
    }
    assert_eq!(found.iter().filter(|n| *n == "th").count(), 2);
    assert_eq!(found.iter().filter(|n| *n == "td").count(), 2);
}

#[test]
fn text_is_escaped() {
    let html = render("a < b & c > d \"quoted\"");
    assert!(
        html.contains("a &#60; b &#38; c &#62; d &#34;quoted&#34;"),
        "{html}"
    );
}

#[test]
fn raw_html_is_shown_as_literal_text() {
    let html = render("<script>alert(1)</script>\n\nhello <b onclick=\"x()\">bold</b>");
    assert_inert(&html);
    assert!(
        html.contains("&#60;script&#62;alert(1)&#60;/script&#62;"),
        "{html}"
    );
    assert!(html.contains("&#60;b onclick="), "{html}");
}

#[test]
fn an_image_is_its_alt_text_and_fetches_nothing() {
    let html = render("![a tracking pixel](https://tracker.example/p.png)");
    assert_inert(&html);
    assert!(html.contains("a tracking pixel"));
    assert!(!html.contains("tracker.example"), "{html}");
}

#[test]
fn an_external_link_opens_safely_and_an_internal_one_does_not_open_a_tab() {
    let html =
        render("[out](https://example.org/a) [in](/privacy) [mail](mailto:legal@example.org)");
    let anchors: Vec<_> = tags(&html).into_iter().filter(|(n, _)| n == "a").collect();
    assert_eq!(anchors.len(), 3);
    let attribute = |index: usize, name: &str| {
        anchors[index]
            .1
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(attribute(0, "href"), Some("https://example.org/a"));
    assert_eq!(attribute(0, "target"), Some("_blank"));
    assert_eq!(attribute(0, "rel"), Some("noopener noreferrer"));
    assert_eq!(attribute(1, "href"), Some("/privacy"));
    assert_eq!(attribute(1, "target"), None);
    assert_eq!(attribute(2, "href"), Some("mailto:legal@example.org"));
    assert_eq!(attribute(2, "target"), None);
}

/// A corpus of the payloads that turn a Markdown renderer into a script-injection hole.
#[test]
fn the_xss_corpus_renders_nothing_executable() {
    let corpus = [
        "<script>alert(1)</script>",
        "<img src=x onerror=alert(1)>",
        "<svg onload=alert(1)>",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<iframe src=\"javascript:alert(1)\"></iframe>",
        "<style>*{background:url(javascript:alert(1))}</style>",
        "[x](javascript:alert(1))",
        "[x](\tjavascript:alert(1))",
        "[x](\njavascript:alert(1))",
        "[x](JaVaScRiPt:alert(1))",
        "[x](java&#x09;script:alert(1))",
        "[x](&#106;avascript:alert(1))",
        "[x](&#x6A;&#x61;&#x76;&#x61;script:alert(1))",
        "[x](data:text/html,<script>alert(1)</script>)",
        "[x](data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==)",
        "[x](vbscript:msgbox(1))",
        "[x][ref]\n\n[ref]: javascript:alert(1)",
        "[ref]\n\n[ref]: JAVASCRIPT:alert(1)",
        "<javascript:alert(1)>",
        "<data:text/html,x>",
        "![x](javascript:alert(1))",
        "![x](x\" onerror=\"alert(1))",
        "[x](https://example.org \"title\" onmouseover=\"alert(1)\")",
        "[x](<javascript:alert(1)>)",
        "<div onmouseover=\"alert(1)\">hover</div>",
        "`<script>alert(1)</script>`",
        "```\n<script>alert(1)</script>\n```",
        "| <script>alert(1)</script> |\n|---|\n| <img src=x onerror=alert(1)> |",
    ];
    for source in corpus {
        let html = render(source);
        assert_inert(&html);
        assert!(!html.contains("<script"), "source {source:?} gave {html}");
    }
}

#[test]
fn an_unsafe_link_keeps_its_text_but_loses_its_anchor() {
    let html = render("[click me](javascript:alert(1))");
    assert!(html.contains("click me"));
    assert!(!names(&html).iter().any(|n| n == "a"), "{html}");
}

#[test]
fn an_autolink_to_a_safe_target_is_an_anchor() {
    let html = render("<https://example.org/x>");
    let anchors: Vec<_> = tags(&html).into_iter().filter(|(n, _)| n == "a").collect();
    assert_eq!(anchors.len(), 1);
}

#[test]
fn empty_and_blank_sources_render_an_empty_container() {
    assert_eq!(render(""), "<div></div>");
    assert_eq!(render("  \n\n "), "<div></div>");
}

#[test]
fn a_large_document_renders() {
    let source =
        "# Heading\n\ntext with **bold** and [a link](https://example.org)\n\n".repeat(2000);
    let html = render(&source);
    assert_eq!(names(&html).iter().filter(|n| *n == "h1").count(), 2000);
}
