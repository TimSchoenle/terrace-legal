//! The list of documents a subject has to accept, and the acceptance it records.

use dioxus::prelude::*;
use terrace_legal_model::LegalDocumentView;
use terrace_legal_model::consent::{Acceptance, DocumentConsent};

use crate::context::{LegalContext, entry_link};
use crate::shared::ErasedTransport;
use crate::traits::Part;

/// Builds the acceptance of the text `view` shows.
///
/// The acceptance quotes the version, locale and digest of `view` itself, so it names exactly the
/// text that was fetched. Returns `None` when the document has no consent policy.
#[must_use]
pub fn acceptance_for(view: &LegalDocumentView) -> Option<Acceptance> {
    let consent = view.consent.as_ref()?;
    Some(Acceptance {
        slug: view.slug.clone(),
        version: consent.version.clone(),
        locale: view.locale.clone(),
        digest: view.digest.clone(),
    })
}

/// Fetches the current text of `slug` in `lang` and records an acceptance of exactly that text.
///
/// The server refuses the acceptance if the text changes between the fetch and the record, so a
/// subject never accepts text that was not the text fetched.
///
/// # Errors
///
/// The transport's error text when either call fails, or a message when the document has no
/// consent policy.
pub async fn accept_document(
    transport: &dyn ErasedTransport,
    slug: &str,
    lang: &str,
) -> Result<(), String> {
    let view = transport.document(slug, lang).await?;
    let acceptance = acceptance_for(&view)
        .ok_or_else(|| format!("the document `{slug}` has no consent policy"))?;
    transport.accept(acceptance).await
}

/// The properties of [`ConsentList`].
#[derive(Props, Clone, PartialEq)]
pub struct ConsentListProps {
    /// The documents the subject still owes, in the order to show them.
    pub documents: Vec<DocumentConsent>,
    /// Called with the slug when the subject accepts one.
    pub on_accept: Callback<String>,
    /// A message to show under the list, such as a failed acceptance.
    #[props(default)]
    pub failure: Option<String>,
}

/// The documents a subject owes, each with a link and an accept button.
///
/// It renders nothing when `documents` is empty or there is no provider above it. It is public
/// so an application can show the same list around its own state.
#[component]
pub fn ConsentList(props: ConsentListProps) -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    if props.documents.is_empty() {
        return VNode::empty();
    }
    let index = context.index.read();
    let heading = context.text.consent_heading();
    let accept_label = context.text.consent_accept();
    let gate = context.class(Part::ConsentGate);
    let item = context.class(Part::ConsentItem);
    let action = context.class(Part::ConsentAction);

    rsx! {
        section { class: gate,
            h2 { "{heading}" }
            ul {
                for document in props.documents.iter() {
                    li { class: item,
                        {
                            let entry = index
                                .as_ref()
                                .and_then(|entries| entries.iter().find(|entry| entry.slug == document.slug));
                            match entry {
                                Some(entry) => entry_link(&context, entry, Part::Link),
                                None => {
                                    let label = context
                                        .text
                                        .known_title(&document.slug)
                                        .unwrap_or_else(|| document.slug.clone());
                                    rsx! { "{label}" }
                                }
                            }
                        }
                        button {
                            class: action,
                            r#type: "button",
                            onclick: {
                                let slug = document.slug.clone();
                                let on_accept = props.on_accept;
                                move |_| on_accept.call(slug.clone())
                            },
                            "{accept_label}"
                        }
                    }
                }
            }
            if let Some(failure) = &props.failure {
                p { role: "alert", "{failure}" }
            }
        }
    }
}

/// Lists the documents the signed-in subject still owes and records each acceptance.
///
/// It fetches the subject's status once, shows [`ConsentList`], and on acceptance fetches the
/// current text of that document and records an acceptance quoting its version and digest. It
/// fetches the status again afterwards, so an accepted document leaves the list. It renders
/// nothing while the status loads, when it fails to load, and when nothing is owed.
#[component]
pub fn ConsentGate() -> Element {
    let Some(context) = try_consume_context::<LegalContext>() else {
        return VNode::empty();
    };
    let transport = context.transport.clone();
    let mut status = use_resource({
        let transport = transport.clone();
        move || {
            let transport = transport.clone();
            async move { transport.consent_status().await }
        }
    });
    let mut failure = use_signal(|| None::<String>);

    let on_accept = Callback::new({
        let text = context.text.clone();
        let language = context.language;
        move |slug: String| {
            let transport = transport.clone();
            let text = text.clone();
            let lang = language.read().clone();
            spawn(async move {
                match accept_document(&*transport, &slug, &lang).await {
                    Ok(()) => {
                        failure.set(None);
                        status.restart();
                    }
                    Err(reason) => failure.set(Some(text.consent_failed(&reason))),
                }
            });
        }
    });

    let outstanding: Vec<DocumentConsent> = match &*status.read() {
        Some(Ok(status)) => status.outstanding().cloned().collect(),
        _ => Vec::new(),
    };
    rsx! {
        ConsentList { documents: outstanding, on_accept, failure: failure() }
    }
}
