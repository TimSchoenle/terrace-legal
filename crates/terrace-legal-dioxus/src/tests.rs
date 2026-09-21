//! Server-side renders of each component, against a provider that holds a fixed index.

use std::time::Duration;

use dioxus::prelude::*;
use terrace_legal_model::{LegalDocumentView, LegalIndexEntry, LegalKind, LocaleTag};

use crate::context::LegalContext;
use crate::{
    AcceptanceSentence, LegalDocumentContent, LegalLinks, LegalProvider, LegalRouting, LegalSkin,
    LegalText, LegalTransport, LocalFuture, Part, Shared, SharedTransport, legal_title,
    use_legal_index, use_published,
};

#[cfg(feature = "consent")]
use terrace_legal_model::consent::{Acceptance, ConsentStatus};

// ---------------------------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------------------------

struct Text;

impl LegalText for Text {
    fn known_title(&self, slug: &str) -> Option<String> {
        match slug {
            "terms" => Some("Terms of Service".into()),
            "privacy" => Some("Privacy".into()),
            _ => None,
        }
    }

    fn heading(&self) -> String {
        "Legal".into()
    }

    fn updated(&self, date: &str) -> String {
        format!("Last updated {date}")
    }

    fn shown_in(&self, locale: &LocaleTag) -> String {
        format!("Shown in {locale}")
    }

    #[cfg(feature = "consent")]
    fn consent_heading(&self) -> String {
        "Please review".into()
    }

    #[cfg(feature = "consent")]
    fn consent_accept(&self) -> String {
        "Accept".into()
    }

    #[cfg(feature = "consent")]
    fn consent_failed(&self, reason: &str) -> String {
        format!("Could not save: {reason}")
    }
}

struct Routing;

impl LegalRouting for Routing {
    fn document_link(&self, slug: &str, label: String, class: &'static str) -> Element {
        rsx! { a { class, href: "/legal/{slug}", "{label}" } }
    }
}

struct Skin;

impl LegalSkin for Skin {
    fn class(&self, part: Part) -> &'static str {
        match part {
            Part::FooterLink => "footer-link",
            Part::Link => "inline-link",
            Part::Page => "page",
            Part::Meta => "meta",
            Part::LocaleNote => "locale-note",
            Part::Prose => "prose",
            #[cfg(feature = "consent")]
            Part::ConsentGate => "gate",
            #[cfg(feature = "consent")]
            Part::ConsentItem => "item",
            #[cfg(feature = "consent")]
            Part::ConsentAction => "action",
            _ => "",
        }
    }
}

struct Offline;

impl LegalTransport for Offline {
    type Error = String;

    fn index(&self, _lang: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
        Box::pin(async { Ok(vec![entry("terms", None), entry("privacy", None)]) })
    }

    fn document(&self, _slug: &str, _lang: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
        Box::pin(async { Err("offline".to_owned()) })
    }

    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>> {
        Box::pin(async { Ok(ConsentStatus::default()) })
    }

    #[cfg(feature = "consent")]
    fn accept(&self, _acceptance: Acceptance) -> LocalFuture<Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

fn entry(slug: &str, title: Option<&str>) -> LegalIndexEntry {
    LegalIndexEntry::new(slug, LegalKind::Inline).with_title(title.map(Into::into))
}

fn external(slug: &str, url: &str, title: &str) -> LegalIndexEntry {
    LegalIndexEntry::new(slug, LegalKind::External)
        .with_title(Some(title.into()))
        .with_url(Some(url.into()))
}

fn view(slug: &str, locale: &str, updated: Option<&str>, body: &str) -> LegalDocumentView {
    LegalDocumentView::new(
        slug,
        locale.parse().expect("valid"),
        body,
        #[cfg(feature = "consent")]
        terrace_legal_model::Digest::from_sha256([3; 32]),
    )
    .with_updated(updated.map(Into::into))
}

// ---------------------------------------------------------------------------------------------
// A harness that provides a fixed index, so no fetch is needed
// ---------------------------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Scene {
    Links(Part),
    Sentence(String, Vec<String>),
    Content(Box<LegalDocumentView>, Option<LocaleTag>),
    Probe,
    #[cfg(feature = "consent")]
    Consent(
        Vec<terrace_legal_model::consent::DocumentConsent>,
        Option<String>,
    ),
}

#[derive(Props, Clone, PartialEq)]
struct HarnessProps {
    entries: Option<Vec<LegalIndexEntry>>,
    provided: bool,
    scene: Scene,
}

#[component]
fn Harness(props: HarnessProps) -> Element {
    let index = use_signal(|| props.entries.clone());
    let language = use_signal(|| "en".to_owned());
    if props.provided {
        use_context_provider(|| LegalContext {
            transport: SharedTransport::new(Offline),
            text: Shared::<dyn LegalText>::new(Text),
            routing: Shared::<dyn LegalRouting>::new(Routing),
            skin: Shared::<dyn LegalSkin>::new(Skin),
            language: language.into(),
            index,
        });
    }
    match props.scene.clone() {
        Scene::Links(part) => rsx! { LegalLinks { part } },
        Scene::Sentence(template, slugs) => rsx! { AcceptanceSentence { template, slugs } },
        Scene::Content(view, requested) => {
            rsx! { LegalDocumentContent { view: *view, requested } }
        }
        Scene::Probe => {
            let all = use_legal_index().map(|entries| entries.len());
            let terms = use_published("terms").map(|entry| entry.slug);
            let nope = use_published("nope").map(|entry| entry.slug);
            rsx! { "{all:?}|{terms:?}|{nope:?}" }
        }
        #[cfg(feature = "consent")]
        Scene::Consent(documents, failure) => rsx! {
            crate::ConsentList {
                documents,
                on_accept: Callback::new(|_: String| {}),
                failure,
            }
        },
    }
}

fn render(entries: Option<Vec<LegalIndexEntry>>, scene: Scene) -> String {
    render_with(entries, true, scene)
}

fn render_with(entries: Option<Vec<LegalIndexEntry>>, provided: bool, scene: Scene) -> String {
    let mut dom = VirtualDom::new_with_props(
        Harness,
        HarnessProps {
            entries,
            provided,
            scene,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|&item| item.to_owned()).collect()
}

// ---------------------------------------------------------------------------------------------
// LegalLinks
// ---------------------------------------------------------------------------------------------

#[test]
fn links_render_nothing_for_an_empty_or_missing_index_or_provider() {
    assert_eq!(render(Some(vec![]), Scene::Links(Part::FooterLink)), "");
    assert_eq!(render(None, Scene::Links(Part::FooterLink)), "");
    assert_eq!(
        render_with(
            Some(vec![entry("terms", None)]),
            false,
            Scene::Links(Part::FooterLink)
        ),
        ""
    );
}

#[test]
fn links_follow_catalog_order_and_take_the_skins_class() {
    let html = render(
        Some(vec![
            entry("privacy", Some("Datenschutz")),
            entry("terms", None),
            entry("zzz", None),
        ]),
        Scene::Links(Part::FooterLink),
    );
    let privacy = html.find("/legal/privacy").expect("privacy link");
    let terms = html.find("/legal/terms").expect("terms link");
    let zzz = html.find("/legal/zzz").expect("zzz link");
    assert!(privacy < terms && terms < zzz, "{html}");
    assert!(html.contains(r#"class="footer-link""#), "{html}");
    assert!(
        html.contains(">Datenschutz<"),
        "operator title first: {html}"
    );
    assert!(
        html.contains(">Terms of Service<"),
        "then the application's name: {html}"
    );
    assert!(html.contains(">zzz<"), "then the slug: {html}");
}

#[test]
fn an_external_link_is_an_anchor_that_opens_safely() {
    let html = render(
        Some(vec![external(
            "imprint",
            "https://example.org/impressum",
            "Imprint",
        )]),
        Scene::Links(Part::FooterLink),
    );
    assert!(
        html.contains(r#"href="https://example.org/impressum""#),
        "{html}"
    );
    assert!(html.contains(r#"rel="noopener noreferrer""#), "{html}");
    assert!(html.contains(r#"target="_blank""#), "{html}");
    assert!(html.contains(r#"class="footer-link""#), "{html}");
    assert!(
        !html.contains("/legal/imprint"),
        "an external link does not use the router: {html}"
    );
}

#[test]
fn a_part_the_skin_does_not_style_writes_no_class_attribute() {
    let html = render(
        Some(vec![external("imprint", "https://example.org/", "Imprint")]),
        Scene::Links(Part::SheetLink),
    );
    assert!(!html.contains("class"), "{html}");
}

#[test]
fn the_title_falls_back_from_operator_to_application_to_slug() {
    let text = Text;
    assert_eq!(
        legal_title(&text, &entry("terms", Some("Nutzungsbedingungen"))),
        "Nutzungsbedingungen"
    );
    assert_eq!(
        legal_title(&text, &entry("terms", Some("  "))),
        "Terms of Service"
    );
    assert_eq!(
        legal_title(&text, &entry("terms", None)),
        "Terms of Service"
    );
    assert_eq!(legal_title(&text, &entry("cookies", None)), "cookies");
}

// ---------------------------------------------------------------------------------------------
// Hooks
// ---------------------------------------------------------------------------------------------

#[test]
fn the_index_hooks_read_the_provided_index() {
    let html = render(
        Some(vec![entry("terms", None), entry("privacy", None)]),
        Scene::Probe,
    );
    assert_eq!(html, "Some(2)|Some(&#34;terms&#34;)|None");
}

#[test]
fn the_index_hooks_answer_none_before_the_index_arrives_and_without_a_provider() {
    assert_eq!(render(None, Scene::Probe), "None|None|None");
    assert_eq!(
        render_with(Some(vec![entry("terms", None)]), false, Scene::Probe),
        "None|None|None"
    );
}

// ---------------------------------------------------------------------------------------------
// LegalDocumentContent
// ---------------------------------------------------------------------------------------------

fn content(view: LegalDocumentView, requested: Option<&str>) -> String {
    render(
        Some(vec![]),
        Scene::Content(
            Box::new(view),
            requested.map(|text| text.parse().expect("valid")),
        ),
    )
}

#[test]
fn a_document_shows_its_title_updated_line_and_rendered_body() {
    let html = content(
        view("terms", "en", Some("2026-08-04"), "# Terms\n\nBe **kind**."),
        Some("en"),
    );
    assert!(html.contains(r#"<article class="page">"#), "{html}");
    assert!(
        html.contains("<h1>Terms of Service</h1>"),
        "the known title heads the page: {html}"
    );
    assert!(
        html.contains(r#"<p class="meta">Last updated 2026-08-04</p>"#),
        "{html}"
    );
    assert!(html.contains(r#"<div class="prose">"#), "{html}");
    assert!(
        html.contains("<strong>kind</strong>"),
        "the body is rendered as Markdown: {html}"
    );
}

#[test]
fn the_operators_title_beats_the_applications() {
    let mut document = view("terms", "en", None, "x");
    document.title = Some("Nutzungsbedingungen".into());
    assert!(content(document, Some("en")).contains("<h1>Nutzungsbedingungen</h1>"));
}

#[test]
fn a_blank_updated_line_is_absent() {
    for updated in [None, Some(""), Some("   ")] {
        let html = content(view("terms", "en", updated, "x"), Some("en"));
        assert!(!html.contains("meta"), "updated {updated:?}: {html}");
        assert!(!html.contains("Last updated"), "{html}");
    }
}

#[test]
fn the_language_note_appears_only_when_the_language_differs() {
    let shown = |locale: &str, requested: Option<&str>| {
        content(view("terms", locale, None, "x"), requested).contains("locale-note")
    };
    assert!(shown("de", Some("en")), "asked for English, got German");
    assert!(shown("en", Some("fr")));
    assert!(!shown("en", Some("en")));
    assert!(
        !shown("de", Some("de-AT")),
        "a regional request answered by its language is not a substitution"
    );
    assert!(!shown("de-AT", Some("de")));
    assert!(!shown("de", None), "no known request, no claim");
}

#[test]
fn the_note_names_the_language_that_is_shown() {
    let html = content(view("terms", "de", None, "x"), Some("en"));
    assert!(
        html.contains(r#"<p class="locale-note">Shown in de</p>"#),
        "{html}"
    );
}

#[test]
fn a_document_page_body_cannot_inject_markup() {
    let html = content(
        view(
            "terms",
            "en",
            None,
            "<script>alert(1)</script>\n\n[x](javascript:alert(1))",
        ),
        Some("en"),
    );
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("javascript:alert(1)\""), "{html}");
}

// ---------------------------------------------------------------------------------------------
// AcceptanceSentence
// ---------------------------------------------------------------------------------------------

fn index() -> Option<Vec<LegalIndexEntry>> {
    Some(vec![
        entry("terms", None),
        entry("privacy", Some("Privacy Notice")),
    ])
}

fn sentence(template: &str, slugs: &[&str]) -> String {
    render(index(), Scene::Sentence(template.into(), strings(slugs)))
}

#[test]
fn a_sentence_links_each_named_document_in_place() {
    let html = sentence(
        "By joining you accept the {terms} and the {privacy}.",
        &["terms", "privacy"],
    );
    assert_eq!(
        html,
        concat!(
            "By joining you accept the ",
            r#"<a class="inline-link" href="/legal/terms">Terms of Service</a>"#,
            " and the ",
            r#"<a class="inline-link" href="/legal/privacy">Privacy Notice</a>"#,
            "."
        )
    );
}

#[test]
fn a_sentence_renders_nothing_unless_every_named_document_is_published() {
    assert_eq!(
        sentence("Accept {terms} and {cookies}.", &["terms", "cookies"]),
        ""
    );
    assert_eq!(
        render(
            None,
            Scene::Sentence("Accept {terms}".into(), strings(&["terms"]))
        ),
        ""
    );
    assert_eq!(
        render_with(
            index(),
            false,
            Scene::Sentence("Accept {terms}".into(), strings(&["terms"]))
        ),
        ""
    );
}

#[test]
fn a_translation_that_reorders_the_placeholders_keeps_its_links() {
    let html = sentence("{privacy} und {terms} akzeptieren", &["terms", "privacy"]);
    let privacy = html.find("/legal/privacy").expect("privacy");
    let terms = html.find("/legal/terms").expect("terms");
    assert!(privacy < terms, "{html}");
    assert!(html.ends_with(" akzeptieren"), "{html}");
}

#[test]
fn a_missing_placeholder_loses_a_link_but_not_the_sentence() {
    let html = sentence("Accept the {terms}.", &["terms", "privacy"]);
    assert_eq!(
        html,
        r#"Accept the <a class="inline-link" href="/legal/terms">Terms of Service</a>."#
    );
}

#[test]
fn a_placeholder_for_a_document_the_sentence_does_not_name_is_shown_as_written() {
    let html = sentence("Accept {terms} and {privacy}.", &["terms"]);
    assert!(html.contains("{privacy}"), "{html}");
    assert!(html.contains("/legal/terms"));
    assert!(!html.contains("/legal/privacy"));
}

#[test]
fn a_sentence_with_no_named_documents_is_its_text() {
    assert_eq!(sentence("Welcome.", &[]), "Welcome.");
}

// ---------------------------------------------------------------------------------------------
// LegalProvider: the index is fetched, and refetched
// ---------------------------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ProvidedProps {
    transport: SharedTransport,
}

#[component]
fn Provided(props: ProvidedProps) -> Element {
    let language = use_signal(|| "en".to_owned());
    rsx! {
        LegalProvider {
            transport: props.transport.clone(),
            text: Shared::<dyn LegalText>::new(Text),
            routing: Shared::<dyn LegalRouting>::new(Routing),
            skin: Shared::<dyn LegalSkin>::new(Skin),
            language,
            LegalLinks { part: Part::FooterLink }
        }
    }
}

async fn settled(dom: &mut VirtualDom) {
    let _ = tokio::time::timeout(Duration::from_millis(200), dom.wait_for_work()).await;
    dom.render_immediate(&mut dioxus_core::NoOpMutations);
}

#[tokio::test]
async fn the_provider_fetches_the_index_and_the_links_appear() {
    let mut dom = VirtualDom::new_with_props(
        Provided,
        ProvidedProps {
            transport: SharedTransport::new(Offline),
        },
    );
    dom.rebuild_in_place();
    assert_eq!(
        dioxus_ssr::render(&dom),
        "",
        "nothing until the index arrives"
    );

    settled(&mut dom).await;
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("/legal/terms") && html.contains("/legal/privacy"),
        "{html}"
    );
}

struct Failing;

impl LegalTransport for Failing {
    type Error = String;

    fn index(&self, _lang: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
        Box::pin(async { Err("connection refused".to_owned()) })
    }

    fn document(&self, _slug: &str, _lang: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
        Box::pin(async { Err("connection refused".to_owned()) })
    }

    #[cfg(feature = "consent")]
    fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>> {
        Box::pin(async { Err("connection refused".to_owned()) })
    }

    #[cfg(feature = "consent")]
    fn accept(&self, _acceptance: Acceptance) -> LocalFuture<Result<(), String>> {
        Box::pin(async { Err("connection refused".to_owned()) })
    }
}

/// An application whose legal routes are down shows no legal links, and no error.
#[tokio::test]
async fn a_failing_index_fetch_is_silent() {
    let mut dom = VirtualDom::new_with_props(
        Provided,
        ProvidedProps {
            transport: SharedTransport::new(Failing),
        },
    );
    dom.rebuild_in_place();
    settled(&mut dom).await;
    assert_eq!(dioxus_ssr::render(&dom), "");
}

// ---------------------------------------------------------------------------------------------
// Consent
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "consent")]
mod consent {
    use std::cell::RefCell;
    use std::rc::Rc;

    use terrace_legal_model::consent::{Acceptance, DocumentConsent, Requirement};
    use terrace_legal_model::{ConsentSummary, Digest};

    use super::*;
    use crate::{ErasedTransport, accept_document, acceptance_for};

    fn owed(slug: &str) -> DocumentConsent {
        DocumentConsent {
            slug: slug.into(),
            requirement: Requirement::Accept,
            version: "2026-08".into(),
            outstanding: true,
            blocking: false,
            content_changed_since: false,
        }
    }

    fn consenting(slug: &str, version: &str, seed: u8) -> LegalDocumentView {
        LegalDocumentView::new(
            slug,
            "de".parse().expect("valid"),
            "text",
            Digest::from_sha256([seed; 32]),
        )
        .with_consent(Some(ConsentSummary {
            requirement: Requirement::Accept,
            version: version.into(),
        }))
    }

    /// Serves one document and records what is accepted.
    struct Recording {
        document: LegalDocumentView,
        accepted: Rc<RefCell<Vec<Acceptance>>>,
    }

    impl LegalTransport for Recording {
        type Error = String;

        fn index(&self, _: &str) -> LocalFuture<Result<Vec<LegalIndexEntry>, String>> {
            Box::pin(async { Ok(vec![]) })
        }

        fn document(&self, _: &str, _: &str) -> LocalFuture<Result<LegalDocumentView, String>> {
            let document = self.document.clone();
            Box::pin(async move { Ok(document) })
        }

        fn consent_status(&self) -> LocalFuture<Result<ConsentStatus, String>> {
            Box::pin(async { Ok(ConsentStatus::default()) })
        }

        fn accept(&self, acceptance: Acceptance) -> LocalFuture<Result<(), String>> {
            self.accepted.borrow_mut().push(acceptance);
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn an_acceptance_quotes_the_version_locale_and_digest_of_the_text_shown() {
        let acceptance = acceptance_for(&consenting("terms", "2026-08", 9)).expect("has a policy");
        assert_eq!(acceptance.slug, "terms");
        assert_eq!(acceptance.version, "2026-08");
        assert_eq!(acceptance.locale.as_str(), "de");
        assert_eq!(acceptance.digest, Digest::from_sha256([9; 32]));
    }

    #[test]
    fn a_document_without_a_policy_cannot_be_accepted() {
        assert_eq!(acceptance_for(&view("terms", "en", None, "x")), None);
    }

    #[tokio::test]
    async fn accepting_sends_what_was_fetched_and_nothing_else() {
        let accepted = Rc::new(RefCell::new(Vec::new()));
        let transport = SharedTransport::new(Recording {
            document: consenting("terms", "2026-08", 4),
            accepted: Rc::clone(&accepted),
        });
        let transport: &dyn ErasedTransport = &*transport;

        accept_document(transport, "terms", "de")
            .await
            .expect("recorded");

        let sent = accepted.borrow();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].version, "2026-08");
        assert_eq!(sent[0].digest, Digest::from_sha256([4; 32]));
        assert_eq!(sent[0].locale.as_str(), "de");
    }

    #[tokio::test]
    async fn a_document_without_a_policy_sends_nothing() {
        let accepted = Rc::new(RefCell::new(Vec::new()));
        let transport = SharedTransport::new(Recording {
            document: view("terms", "en", None, "x"),
            accepted: Rc::clone(&accepted),
        });
        let transport: &dyn ErasedTransport = &*transport;
        let error = accept_document(transport, "terms", "en")
            .await
            .expect_err("no policy");
        assert!(error.contains("no consent policy"), "{error}");
        assert!(accepted.borrow().is_empty());
    }

    #[test]
    fn the_list_shows_each_owed_document_with_a_link_and_a_button() {
        let html = render(
            index(),
            Scene::Consent(vec![owed("terms"), owed("privacy")], None),
        );
        assert!(html.contains(r#"<section class="gate">"#), "{html}");
        assert!(html.contains("<h2>Please review</h2>"), "{html}");
        assert_eq!(html.matches(r#"<li class="item">"#).count(), 2, "{html}");
        assert_eq!(html.matches(r#"class="action""#).count(), 2, "{html}");
        assert_eq!(html.matches(">Accept</button>").count(), 2, "{html}");
        assert!(
            html.contains("/legal/terms") && html.contains("/legal/privacy"),
            "{html}"
        );
        assert!(!html.contains("role=\"alert\""), "{html}");
    }

    #[test]
    fn a_document_missing_from_the_index_is_named_without_a_link() {
        let html = render(index(), Scene::Consent(vec![owed("cookies")], None));
        assert!(html.contains("cookies"), "{html}");
        assert!(!html.contains("/legal/cookies"), "{html}");
    }

    #[test]
    fn the_list_shows_a_failure_and_nothing_when_nothing_is_owed() {
        let html = render(
            index(),
            Scene::Consent(vec![owed("terms")], Some("Could not save: boom".into())),
        );
        assert!(
            html.contains(r#"role="alert""#) && html.contains("Could not save: boom"),
            "{html}"
        );
        assert_eq!(
            render(index(), Scene::Consent(vec![], Some("x".into()))),
            ""
        );
        assert_eq!(
            render_with(index(), false, Scene::Consent(vec![owed("terms")], None)),
            ""
        );
    }
}
