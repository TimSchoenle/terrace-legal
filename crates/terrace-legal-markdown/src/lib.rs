//! Renders untrusted Markdown to Dioxus elements, without an HTML string at any point.
//!
//! [`markdown`] takes the text an operator wrote and returns an [`Element`](dioxus::prelude::Element).
//! Nothing in the output can execute: raw HTML is shown as literal text, a link whose scheme is
//! not `http`, `https`, `mailto` or `tel` is shown as its text, and an image is shown as its alt
//! text. The output uses semantic elements only (`h1` to `h6`, `p`, `ul`, `ol`, `li`, `table`,
//! `blockquote`, `pre`, `code`, `em`, `strong`, `del`, `a`, `br`, `hr`) and no classes, so the
//! container the host wraps it in decides how it looks.
//!
//! ```
//! use dioxus::prelude::*;
//! use terrace_legal_markdown::markdown;
//!
//! fn page() -> Element {
//!     rsx! { article { {markdown("# Terms\n\nSee the [privacy notice](/privacy).")} } }
//! }
//! # let mut dom = VirtualDom::new(page);
//! # dom.rebuild_in_place();
//! ```
//!
//! # Design record
//!
//! **Why there is no HTML string.** The usual route, rendering Markdown to HTML and injecting it,
//! makes every gap in a sanitiser a script-injection hole. Here the parser's events become
//! elements one by one, so text is escaped by the renderer and there is nothing to sanitise. Raw
//! HTML arrives as an event and is rendered as text.
//!
//! **Why links go through an allowlist.** Escaping does not help with a `javascript:` link, whose
//! danger is in its target. [`is_safe_href`] reads a target the way a browser does, so a tab
//! inside the scheme or a leading control character does not slip through.
//!
//! **Why images render as alt text.** A document an operator wrote would otherwise make every
//! reader's browser fetch a remote resource: a tracking pixel, and a hole in the page's content
//! security policy.
//!
//! **Why this is its own crate.** Nothing here is about legal documents. A host's release notes
//! use it too.

mod href;
mod render;
mod tree;

pub use href::is_safe_href;
pub use render::markdown;
