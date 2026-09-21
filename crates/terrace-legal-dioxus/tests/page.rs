//! `LegalDocumentPage` through the public API: loading, failure, success, and the title callback.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use terrace_legal_dioxus::{
    LegalDocumentPage, LegalProvider, LegalRouting, LegalSkin, LegalText, LegalTransport,
    LocalFuture, Part, Shared, SharedTransport, Unstyled,
};
use terrace_legal_model::{LegalDocumentView, LegalIndexEntry, LocaleTag};

#[cfg(feature = "consent")]
use terrace_legal_model::consent::{Acceptance, ConsentStatus};

struct Words;

impl LegalText for Words {
    fn known_title(&self, slug: &str) -> Option<String> {
        (slug == "terms").then(|| "Terms of Service".to_owned())
    }

    fn heading(&self) -> String {
        "Legal".into()
    }

    fn updated(&self, date: &str) -> String {
        format!("Updated {date}")
    }

    fn shown_in(&self, locale: &LocaleTag) -> String {
        format!("Shown in {locale}")
    }

    #[cfg(feature = "consent")]
    fn consent_heading(&self) -> String {
        String::new()
    }

    #[cfg(feature = "consent")]
    fn consent_accept(&self) -> String {
        String::new()
    }

    #[cfg(feature = "consent")]
    fn consent_failed(&self, reason: &str) -> String {
        reason.to_owned()
    }
}

struct Links;

impl LegalRouting for Links {
    fn document_link(&self, slug: &str, label: String, _class: &'static str) -> Element {
        rsx! { a { href: "/legal/{slug}", "{label}" } }
    }
}

/// Answers a document request with `outcome`, and every other request with nothing.
struct Fixed {
    outcome: Result<LegalDocumentView, String>,
}

impl LegalTransport for Fixed {
    type Error = String;

    fn index(&self, _: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn document(&self, _: &str, _: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
        let outcome = self.outcome.clone();
        Box::pin(async move { outcome })
    }

    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>> {
        Box::pin(async { Ok(ConsentStatus::default()) })
    }

    #[cfg(feature = "consent")]
    fn accept(&self, _: Acceptance) -> LocalFuture<Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

fn document(locale: &str) -> LegalDocumentView {
    LegalDocumentView::new(
        "terms",
        locale.parse().expect("valid"),
        "# Terms

Be kind.",
        #[cfg(feature = "consent")]
        terrace_legal_model::Digest::from_sha256([1; 32]),
    )
    .with_updated(Some("2026-08-04".into()))
}

/// A list of titles that a callback appends to, comparable by identity so it can be a property.
#[derive(Clone, Default)]
struct Titles(Rc<RefCell<Vec<String>>>);

impl PartialEq for Titles {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Props, Clone, PartialEq)]
struct HostProps {
    transport: SharedTransport,
    titles: Titles,
}

#[component]
fn Host(props: HostProps) -> Element {
    let language = use_signal(|| "en".to_owned());
    let titles = props.titles.clone();
    rsx! {
        LegalProvider {
            transport: props.transport.clone(),
            text: Shared::<dyn LegalText>::new(Words),
            routing: Shared::<dyn LegalRouting>::new(Links),
            skin: Shared::<dyn LegalSkin>::new(Unstyled),
            language,
            LegalDocumentPage {
                slug: "terms",
                loading: rsx! { p { "loading" } },
                error: Callback::new(|reason: String| rsx! { p { "failed: {reason}" } }),
                on_title: Callback::new(move |title: String| titles.0.borrow_mut().push(title)),
            }
        }
    }
}

async fn run(outcome: Result<LegalDocumentView, String>) -> (String, String, Vec<String>) {
    let titles = Titles::default();
    let mut dom = VirtualDom::new_with_props(
        Host,
        HostProps {
            transport: SharedTransport::new(Fixed { outcome }),
            titles: titles.clone(),
        },
    );
    dom.rebuild_in_place();
    let before = dioxus_ssr::render(&dom);

    for _ in 0..3 {
        let _ = tokio::time::timeout(Duration::from_millis(100), dom.wait_for_work()).await;
        dom.render_immediate(&mut dioxus_core::NoOpMutations);
    }
    let after = dioxus_ssr::render(&dom);
    let collected = titles.0.borrow().clone();
    (before, after, collected)
}

#[tokio::test]
async fn a_document_shows_loading_first_and_then_its_content() {
    let (before, after, titles) = run(Ok(document("en"))).await;
    assert_eq!(before, "<p>loading</p>");
    assert!(after.contains("<h1>Terms of Service</h1>"), "{after}");
    assert!(after.contains("Updated 2026-08-04"), "{after}");
    assert!(after.contains("<p>Be kind.</p>"), "{after}");
    assert!(
        !after.contains("Shown in"),
        "the requested language was served: {after}"
    );
    assert_eq!(
        titles,
        ["Terms of Service"],
        "the title callback fires once the document loads"
    );
}

#[tokio::test]
async fn a_document_in_another_language_says_so() {
    let (_, after, _) = run(Ok(document("de"))).await;
    assert!(after.contains("Shown in de"), "{after}");
}

#[tokio::test]
async fn a_failed_fetch_hands_the_reason_to_the_error_callback() {
    let (before, after, titles) = run(Err("HTTP 500".into())).await;
    assert_eq!(before, "<p>loading</p>");
    assert_eq!(after, "<p>failed: HTTP 500</p>");
    assert!(titles.is_empty(), "no title for a page that failed");
}

#[test]
fn the_skin_can_be_the_unstyled_default() {
    for part in [
        Part::FooterLink,
        Part::SheetLink,
        Part::Link,
        Part::Page,
        Part::Meta,
        Part::LocaleNote,
        Part::Prose,
    ] {
        assert_eq!(Unstyled.class(part), "");
    }
}
