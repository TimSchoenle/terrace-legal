//! The component that fetches the index and shares the application's handles.

use dioxus::prelude::*;

use crate::context::LegalContext;
use crate::shared::{Shared, SharedTransport};
use crate::traits::{LegalRouting, LegalSkin, LegalText};

/// The properties of [`LegalProvider`].
#[derive(Props, Clone, PartialEq)]
pub struct LegalProviderProps {
    /// Fetches documents from the server.
    pub transport: SharedTransport,
    /// The application's words.
    pub text: Shared<dyn LegalText>,
    /// The application's router.
    pub routing: Shared<dyn LegalRouting>,
    /// The application's classes.
    pub skin: Shared<dyn LegalSkin>,
    /// The language documents are requested in. The index is fetched again when it changes.
    pub language: ReadSignal<String>,
    /// The part of the application that uses the legal components.
    pub children: Element,
}

/// Fetches the index of published documents and shares it with everything below it.
///
/// The index is fetched once per language. Failure is silent: an application whose legal routes
/// are down shows no legal links, which is the right degradation, and the components below render
/// nothing rather than an error.
///
/// The transport, text, routing and skin are read when the provider first renders, so they
/// should be created once and kept. Mount one provider, high in the tree.
#[component]
pub fn LegalProvider(props: LegalProviderProps) -> Element {
    let mut index = use_signal(|| None);
    let language = props.language;

    let context = use_context_provider({
        let props = props.clone();
        move || LegalContext {
            transport: props.transport,
            text: props.text,
            routing: props.routing,
            skin: props.skin,
            language,
            index,
        }
    });

    let transport = context.transport.clone();
    let _fetch = use_resource(move || {
        let lang = language.read().clone();
        let transport = transport.clone();
        async move {
            index.set(transport.index(&lang).await.ok());
        }
    });

    props.children
}
