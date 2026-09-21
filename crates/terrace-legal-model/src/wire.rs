//! The types that cross the HTTP boundary between a server and a client.
//!
//! The shapes are a contract with generated clients, so they change only by addition. The structs
//! are `#[non_exhaustive]` and are built with a constructor and setters, which is what lets a
//! field be added without breaking a caller.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::locale::LocaleTag;

#[cfg(feature = "consent")]
use crate::consent::Requirement;

/// Whether a document is served from here or hosted elsewhere.
// The variants carry no `///`: utoipa would publish each one as a described `oneOf` branch instead
// of a plain enum, and the published schema is a contract that has to stay a plain string enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[non_exhaustive]
pub enum LegalKind {
    #[expect(missing_docs, reason = "see the comment above the enum")]
    Inline,
    #[expect(missing_docs, reason = "see the comment above the enum")]
    External,
}

/// Which locale the caller wants.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::IntoParams))]
#[cfg_attr(feature = "utoipa", into_params(parameter_in = Query))]
pub struct LegalParams {
    /// Language code (`en`, `de`). Falls back to `Accept-Language`, then to the first locale
    /// the operator configured.
    #[serde(default)]
    pub lang: Option<String>,
}

/// One entry in the index the footer renders.
// utoipa publishes every `///` line above as the schema's description, so only the summary is `///`.
// Every member is always present; an absent value is `null`. The type is `#[non_exhaustive]`, so
// it is built with `LegalIndexEntry::new` and the setters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[non_exhaustive]
pub struct LegalIndexEntry {
    /// The URL slug, and the key an operator configured this document under.
    pub slug: String,
    /// The operator's title for the requested locale, when they set one.
    pub title: Option<String>,
    /// The "last updated" line, verbatim as configured.
    pub updated: Option<String>,
    /// `inline` for a document this API serves, `external` for one hosted elsewhere.
    pub kind: LegalKind,
    /// Where to send the reader, for an `external` document.
    pub url: Option<String>,
    /// The locales an `inline` document is published in.
    #[cfg_attr(feature = "utoipa", schema(value_type = Vec<String>))]
    pub locales: Vec<LocaleTag>,
    /// The consent policy of the document. Absent when it has none.
    #[cfg(feature = "consent")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consent: Option<ConsentSummary>,
}

impl LegalIndexEntry {
    /// Creates an entry with no title, date, URL or locales.
    ///
    /// The type is `#[non_exhaustive]`, so this and the `with_*` setters are the only way to build
    /// one. Every member is always serialised; an absent value is `null`.
    #[must_use]
    pub fn new(slug: impl Into<String>, kind: LegalKind) -> Self {
        Self {
            slug: slug.into(),
            title: None,
            updated: None,
            kind,
            url: None,
            locales: Vec::new(),
            #[cfg(feature = "consent")]
            consent: None,
        }
    }

    /// Sets the title.
    #[must_use]
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// Sets the "last updated" line.
    #[must_use]
    pub fn with_updated(mut self, updated: Option<String>) -> Self {
        self.updated = updated;
        self
    }

    /// Sets the URL of an external document.
    #[must_use]
    pub fn with_url(mut self, url: Option<String>) -> Self {
        self.url = url;
        self
    }

    /// Sets the locales an inline document is published in.
    #[must_use]
    pub fn with_locales(mut self, locales: Vec<LocaleTag>) -> Self {
        self.locales = locales;
        self
    }

    /// Sets the consent policy.
    #[cfg(feature = "consent")]
    #[must_use]
    pub fn with_consent(mut self, consent: Option<ConsentSummary>) -> Self {
        self.consent = consent;
        self
    }
}

/// The format of a document's body.
///
/// A value rather than an enum, so a deployment or a later release can publish another format
/// without this crate having to know it. Clients render the formats they understand and show the
/// rest as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentFormat(String);

impl DocumentFormat {
    /// The only format published today.
    pub const MARKDOWN: &'static str = "markdown";

    /// Returns [`Self::MARKDOWN`].
    #[must_use]
    pub fn markdown() -> Self {
        Self(Self::MARKDOWN.to_owned())
    }

    /// Wraps another format name.
    #[must_use]
    pub fn other(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the format name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns whether the body is Markdown.
    #[must_use]
    pub fn is_markdown(&self) -> bool {
        self.0 == Self::MARKDOWN
    }
}

impl Default for DocumentFormat {
    fn default() -> Self {
        Self::markdown()
    }
}

/// One document, in the locale that was actually served.
// utoipa publishes every `///` line above as the schema's description, so only the summary is `///`.
// Every member is always present; an absent value is `null`. The type is `#[non_exhaustive]`, so
// it is built with `LegalDocumentView::new` and the setters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[non_exhaustive]
pub struct LegalDocumentView {
    // The published schema has no description for `slug`, `title`, `updated` and `body`, and a
    // `///` here would add one, so those four members are documented with `//` and exempted.
    #[expect(missing_docs, reason = "see the comment above the struct members")]
    pub slug: String,
    /// The locale served, which is **not** necessarily the one requested — a reader asking for
    /// German and receiving the only available English text has to be told, or they conclude
    /// the operator writes German like that.
    #[cfg_attr(feature = "utoipa", schema(value_type = String, example = "de-AT"))]
    pub locale: LocaleTag,
    #[expect(missing_docs, reason = "see the comment above the struct members")]
    pub title: Option<String>,
    #[expect(missing_docs, reason = "see the comment above the struct members")]
    pub updated: Option<String>,
    /// Always `markdown`. Present so a future format is a value rather than a new endpoint.
    #[cfg_attr(feature = "utoipa", schema(value_type = String, example = "markdown"))]
    pub format: DocumentFormat,
    #[expect(missing_docs, reason = "see the comment above the struct members")]
    pub body: String,
    /// The digest of `body`, which an acceptance quotes so it names the text that was shown.
    #[cfg(feature = "consent")]
    #[cfg_attr(feature = "utoipa", schema(value_type = String, example = "sha256:0f4c"))]
    pub digest: Digest,
    /// The consent policy of the document. Absent when it has none.
    #[cfg(feature = "consent")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consent: Option<ConsentSummary>,
}

impl LegalDocumentView {
    /// Creates a Markdown view with no title or date.
    ///
    /// The type is `#[non_exhaustive]`, so this and the `with_*` setters are the only way to build
    /// one. Every member is always serialised; an absent value is `null`.
    ///
    /// With the `consent` feature the digest of `body` is required too, because an acceptance
    /// quotes it.
    #[must_use]
    pub fn new(
        slug: impl Into<String>,
        locale: LocaleTag,
        body: impl Into<String>,
        #[cfg(feature = "consent")] digest: Digest,
    ) -> Self {
        Self {
            slug: slug.into(),
            locale,
            title: None,
            updated: None,
            format: DocumentFormat::markdown(),
            body: body.into(),
            #[cfg(feature = "consent")]
            digest,
            #[cfg(feature = "consent")]
            consent: None,
        }
    }

    /// Sets the title.
    #[must_use]
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// Sets the "last updated" line.
    #[must_use]
    pub fn with_updated(mut self, updated: Option<String>) -> Self {
        self.updated = updated;
        self
    }

    /// Sets the body format.
    #[must_use]
    pub fn with_format(mut self, format: DocumentFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets the consent policy.
    #[cfg(feature = "consent")]
    #[must_use]
    pub fn with_consent(mut self, consent: Option<ConsentSummary>) -> Self {
        self.consent = consent;
        self
    }
}

/// What a client needs to know about a document's consent policy.
#[cfg(feature = "consent")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ConsentSummary {
    /// What the reader has to do with the document.
    pub requirement: Requirement,
    /// The operator's revision, which an acceptance quotes.
    pub version: String,
}

/// A SHA-256 digest written as `sha256:` and 64 lowercase hexadecimal digits.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest(String);

/// Why a string is not a [`Digest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a digest is `sha256:` followed by 64 lowercase hexadecimal digits")]
pub struct ParseDigestError;

const PREFIX: &str = "sha256:";

impl Digest {
    /// Formats raw SHA-256 output.
    #[must_use]
    pub fn from_sha256(bytes: [u8; 32]) -> Self {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(PREFIX.len() + 64);
        text.push_str(PREFIX);
        for byte in bytes {
            text.push(char::from(HEX[usize::from(byte >> 4)]));
            text.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        Self(text)
    }

    /// Returns the `sha256:<hex>` text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns only the hexadecimal digits.
    #[must_use]
    pub fn hex(&self) -> &str {
        &self.0[PREFIX.len()..]
    }
}

impl FromStr for Digest {
    type Err = ParseDigestError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let hex = input.strip_prefix(PREFIX).ok_or(ParseDigestError)?;
        let valid = hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if valid {
            Ok(Self(input.to_owned()))
        } else {
            Err(ParseDigestError)
        }
    }
}

impl TryFrom<String> for Digest {
    type Error = ParseDigestError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> Self {
        digest.0
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_view(slug: &str, locale: &str, body: &str) -> LegalDocumentView {
        LegalDocumentView::new(
            slug,
            locale.parse().expect("valid"),
            body,
            #[cfg(feature = "consent")]
            Digest::from_sha256([1; 32]),
        )
    }

    #[test]
    fn a_digest_formats_and_parses_back() {
        let digest = Digest::from_sha256([0xab; 32]);
        assert_eq!(digest.as_str(), format!("sha256:{}", "ab".repeat(32)));
        assert_eq!(digest.hex().len(), 64);
        assert_eq!(digest.as_str().parse::<Digest>(), Ok(digest));
    }

    #[test]
    fn malformed_digests_are_refused() {
        for input in [
            "",
            "sha256:",
            "sha256:ab",
            "md5:abababababababababababababababababababababababababababababababab",
            &format!("sha256:{}", "AB".repeat(32)),
            &format!("sha256:{}", "zz".repeat(32)),
            &format!("sha256:{}", "a".repeat(65)),
        ] {
            assert!(input.parse::<Digest>().is_err(), "input {input:?}");
        }
    }

    /// The published shape of the two payloads is a contract with a generated client, so it is
    /// pinned member by member: every nullable member is present as `null`, and `kind` is
    /// lowercase.
    #[test]
    fn the_wire_shapes_match_the_published_contract() {
        let entry = LegalIndexEntry::new("terms", LegalKind::Inline).with_locales(vec![
            "en".parse().expect("valid"),
            "de-AT".parse().expect("valid"),
        ]);
        assert_eq!(
            serde_json::to_value(&entry).expect("serialises"),
            serde_json::json!({
                "slug": "terms",
                "title": null,
                "updated": null,
                "kind": "inline",
                "url": null,
                "locales": ["en", "de-AT"]
            })
        );

        let external = LegalIndexEntry::new("imprint", LegalKind::External)
            .with_title(Some("Imprint".into()))
            .with_updated(Some("2026-08-04".into()))
            .with_url(Some("https://example.org/impressum".into()));
        let json = serde_json::to_value(&external).expect("serialises");
        assert_eq!(
            json,
            serde_json::json!({
                "slug": "imprint",
                "title": "Imprint",
                "updated": "2026-08-04",
                "kind": "external",
                "url": "https://example.org/impressum",
                "locales": []
            })
        );
        assert_eq!(
            serde_json::from_value::<LegalIndexEntry>(json).expect("deserialises"),
            external
        );

        let view = new_view("terms", "de-AT", "# Terms");
        let json = serde_json::to_value(&view).expect("serialises");
        assert_eq!(json["slug"], "terms");
        assert_eq!(json["locale"], "de-AT");
        assert_eq!(json["title"], serde_json::Value::Null);
        assert_eq!(json["updated"], serde_json::Value::Null);
        assert_eq!(json["format"], "markdown");
        assert_eq!(json["body"], "# Terms");
        #[cfg(not(feature = "consent"))]
        assert_eq!(
            json.as_object().expect("object").len(),
            6,
            "no member beyond the published six"
        );
        assert_eq!(
            serde_json::from_value::<LegalDocumentView>(json).expect("deserialises"),
            view
        );
    }

    #[test]
    fn a_format_is_a_value_so_a_new_one_needs_no_new_type() {
        assert!(DocumentFormat::default().is_markdown());
        let other = DocumentFormat::other("asciidoc");
        assert!(!other.is_markdown());
        assert_eq!(other.as_str(), "asciidoc");
        let view = new_view("terms", "en", "x").with_format(other.clone());
        let json = serde_json::to_value(&view).expect("serialises");
        assert_eq!(json["format"], "asciidoc");
        assert_eq!(
            serde_json::from_value::<LegalDocumentView>(json)
                .expect("deserialises")
                .format,
            other
        );
    }

    /// Text in a description that only means something to a Rust reader: intra-doc links, paths,
    /// attributes and the `#[non_exhaustive]` builder guidance.
    #[cfg(feature = "utoipa")]
    const RUST_ONLY_MARKERS: [&str; 5] = ["[`", "Self::", "#[", "non_exhaustive", "crate::"];

    /// Collects every `description` in `value`, with the JSON path it was found at.
    #[cfg(feature = "utoipa")]
    fn descriptions<'a>(
        value: &'a serde_json::Value,
        path: &str,
        out: &mut Vec<(String, &'a str)>,
    ) {
        match value {
            serde_json::Value::Object(members) => {
                for (key, member) in members {
                    let at = format!("{path}/{key}");
                    match (key.as_str(), member.as_str()) {
                        ("description", Some(text)) => out.push((at, text)),
                        _ => descriptions(member, &at, out),
                    }
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    descriptions(item, &format!("{path}/{index}"), out);
                }
            }
            _ => {}
        }
    }

    /// The published schema carries the doc comments as descriptions, so they are part of the
    /// contract a generated client sees.
    ///
    /// It pins the bug where text meant for Rust callers was published into the schema: a `///`
    /// paragraph on a struct told HTTP clients the type is `#[non_exhaustive]` and carried an
    /// intra-doc link to `Self::new`, and generated clients copied both into their own doc
    /// comments. The exact struct descriptions are asserted, and every description in the
    /// document is swept for Rust syntax.
    #[cfg(feature = "utoipa")]
    #[test]
    fn the_wire_types_publish_the_expected_schemas() {
        use utoipa::OpenApi;

        #[cfg(not(feature = "consent"))]
        #[derive(OpenApi)]
        #[openapi(components(schemas(LegalIndexEntry, LegalDocumentView, LegalKind)))]
        struct Api;

        #[cfg(feature = "consent")]
        #[derive(OpenApi)]
        #[openapi(components(schemas(
            LegalIndexEntry,
            LegalDocumentView,
            LegalKind,
            ConsentSummary,
            crate::consent::Requirement,
            crate::consent::DocumentConsent,
            crate::consent::ConsentStatus,
            crate::consent::Acceptance,
            crate::consent::Withdrawal,
        )))]
        struct Api;

        let json = serde_json::to_value(Api::openapi()).expect("serialises");
        let schemas = &json["components"]["schemas"];

        assert_eq!(
            schemas["LegalIndexEntry"]["description"],
            "One entry in the index the footer renders."
        );
        assert_eq!(
            schemas["LegalDocumentView"]["description"],
            "One document, in the locale that was actually served."
        );

        let params =
            serde_json::to_value(<LegalParams as utoipa::IntoParams>::into_params(|| None))
                .expect("serialises");
        let mut found = Vec::new();
        descriptions(&json, "", &mut found);
        descriptions(&params, "/LegalParams", &mut found);
        assert!(
            found.len() > 10,
            "the sweep reached the descriptions: {found:?}"
        );
        for (path, text) in &found {
            for marker in RUST_ONLY_MARKERS {
                assert!(
                    !text.contains(marker),
                    "{path} publishes Rust-only text ({marker:?}): {text:?}"
                );
            }
        }

        let kind = &schemas["LegalKind"];
        assert_eq!(kind["type"], "string");
        assert_eq!(kind["enum"], serde_json::json!(["inline", "external"]));
        assert!(
            kind.get("oneOf").is_none(),
            "a plain enum, not a described oneOf: {kind}"
        );

        let entry = &schemas["LegalIndexEntry"];
        assert_eq!(
            entry["properties"]["slug"]["description"],
            "The URL slug, and the key an operator configured this document under."
        );
        assert_eq!(entry["properties"]["locales"]["type"], "array");
        assert_eq!(entry["properties"]["locales"]["items"]["type"], "string");
        let required: Vec<&str> = entry["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|name| name.as_str())
            .collect();
        assert!(
            required.contains(&"slug")
                && required.contains(&"kind")
                && required.contains(&"locales")
        );
        assert!(
            !required.contains(&"title") && !required.contains(&"url"),
            "nullable, not required"
        );

        let view = &schemas["LegalDocumentView"];
        assert_eq!(view["properties"]["locale"]["type"], "string");
        assert_eq!(view["properties"]["format"]["type"], "string");
        assert!(view["properties"]["slug"].get("description").is_none());
    }
}
