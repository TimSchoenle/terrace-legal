//! The list of links to every published document.

use dioxus::prelude::*;

use crate::context::{LegalContext, entry_link};
use crate::traits::Part;

/// The properties of [`LegalLinks`].
#[derive(Props, Clone, PartialEq)]
pub struct LegalLinksProps {
    /// Which skin part styles each link, so a footer and a menu sheet can differ.
    pub part: Part,
}

/// One link per published document, in the order the operator chose.
///
/// A hosted document links through the application's router and an external one is an anchor that
/// opens in a new tab with `rel="noopener noreferrer"`. It renders nothing when there is no
/// provider, when the index has not arrived, and when nothing is published.
#[component]
pub fn LegalLinks(props: LegalLinksProps) -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    let index = context.index.read();
    let Some(entries) = index.as_ref().filter(|entries| !entries.is_empty()) else {
        return VNode::empty();
    };
    rsx! {
        for entry in entries.iter() {
            {entry_link(&context, entry, props.part)}
        }
    }
}
