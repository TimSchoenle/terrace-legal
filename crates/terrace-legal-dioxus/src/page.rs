//! The page of one hosted document.

use dioxus::prelude::*;
use terrace_legal_markdown::markdown;
use terrace_legal_model::{LegalDocumentView, LocaleTag};

use crate::context::{LegalContext, view_title};

/// The properties of [`LegalDocumentContent`].
#[derive(Props, Clone, PartialEq)]
pub struct LegalDocumentContentProps {
    /// The document as the server returned it.
    pub view: LegalDocumentView,
    /// The locale the reader asked for. The "shown in" note appears only when the document is in
    /// another language, so it is not shown for a request answered by a regional variant.
    #[props(default)]
    pub requested: Option<LocaleTag>,
}

/// A loaded document: its title, the "last updated" line, a note when it is not in the reader's
/// language, and the rendered Markdown.
///
/// [`LegalDocumentPage`] renders it once the document has arrived. It is public so that an
/// application that fetches documents itself, for a server-side render or a test, can show one.
///
/// It renders nothing without a [`crate::LegalProvider`] above it.
#[component]
pub fn LegalDocumentContent(props: LegalDocumentContentProps) -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    let view = &props.view;

    let title = view_title(&*context.text, view);
    let updated = view
        .updated
        .as_deref()
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .map(|date| context.text.updated(date));
    let locale_note = props
        .requested
        .as_ref()
        .filter(|requested| !requested.same_language(&view.locale))
        .map(|_| context.text.shown_in(&view.locale));

    let page = context.class(crate::traits::Part::Page);
    let meta = context.class(crate::traits::Part::Meta);
    let note = context.class(crate::traits::Part::LocaleNote);
    let prose = context.class(crate::traits::Part::Prose);

    rsx! {
        article { class: page,
            h1 { "{title}" }
            if let Some(updated) = updated {
                p { class: meta, "{updated}" }
            }
            if let Some(locale_note) = locale_note {
                p { class: note, "{locale_note}" }
            }
            div { class: prose, {markdown(&view.body)} }
        }
    }
}

/// The properties of [`LegalDocumentPage`].
#[derive(Props, Clone, PartialEq)]
pub struct LegalDocumentPageProps {
    /// The document to show.
    pub slug: String,
    /// Shown while the document is being fetched.
    #[props(default = VNode::empty())]
    pub loading: Element,
    /// Renders the failure, given the transport's error text.
    pub error: Callback<String, Element>,
    /// Called with the title to show, when a document has loaded, so the application can set its
    /// window or tab title.
    #[props(default)]
    pub on_title: Option<Callback<String>>,
}

/// Fetches one document in the provider's language and shows it.
///
/// While it loads, `loading` is shown. If the fetch fails, `error` renders the transport's error
/// text. The document is fetched again when `slug` or the provider's language changes.
///
/// It renders nothing without a [`crate::LegalProvider`] above it.
#[component]
pub fn LegalDocumentPage(props: LegalDocumentPageProps) -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    let language = context.language;
    let transport = context.transport.clone();

    let slug = props.slug.clone();
    let document = use_resource(use_reactive!(|(slug,)| {
        let lang = language.read().clone();
        let transport = transport.clone();
        async move { transport.document(&slug, &lang).await }
    }));

    let on_title = props.on_title;
    let text = context.text.clone();
    use_effect(move || {
        if let (Some(Ok(view)), Some(on_title)) = (&*document.read(), on_title) {
            on_title.call(view_title(&*text, view));
        }
    });

    let state = document.read();
    match &*state {
        None => props.loading.clone(),
        Some(Err(reason)) => props.error.call(reason.clone()),
        Some(Ok(view)) => {
            let requested = language.read().parse::<LocaleTag>().ok();
            rsx! {
                LegalDocumentContent { view: view.clone(), requested }
            }
        }
    }
}
