//! Which link targets a document is allowed to carry.

/// The URL schemes a rendered link may use. Everything else is rendered as its text.
const ALLOWED_SCHEMES: [&str; 4] = ["http", "https", "mailto", "tel"];

/// Returns whether `href` is safe to put in an anchor.
///
/// A reference without a scheme, such as `/privacy`, `#section` or `../terms`, is safe. A
/// reference with a scheme is safe only when the scheme is `http`, `https`, `mailto` or `tel`,
/// compared without regard to case.
///
/// The check reads the reference the way a browser does. A browser drops leading control
/// characters and spaces and removes every tab and line break before it looks for a scheme, so
/// `\tjavascript:alert(1)` and `java\nscript:alert(1)` are `javascript:` links. Anything before
/// the first `:` that is not a valid scheme, such as `java script:`, is refused rather than
/// guessed at.
#[must_use]
pub fn is_safe_href(href: &str) -> bool {
    let cleaned: String = href
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();

    let Some(colon) = cleaned.find(':') else {
        return true;
    };
    let head = &cleaned[..colon];
    if head.contains(['/', '?', '#']) {
        // The colon belongs to a path, a query or a fragment, so this is a relative reference.
        return true;
    }
    let mut chars = head.chars();
    let valid_scheme = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid_scheme
        && ALLOWED_SCHEMES
            .iter()
            .any(|allowed| head.eq_ignore_ascii_case(allowed))
}

/// Returns whether a safe `href` leaves the site over `http` or `https`, so that it opens in a new
/// tab with `rel="noopener noreferrer"`.
pub(crate) fn is_external(href: &str) -> bool {
    let cleaned: String = href
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let lower = cleaned.to_ascii_lowercase();
    lower.starts_with("http:") || lower.starts_with("https:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_navigable_schemes_are_rendered_as_links() {
        for safe in [
            "https://example.org/a?b=c#d",
            "http://example.org",
            "HTTPS://EXAMPLE.ORG",
            "mailto:legal@example.org",
            "tel:+491234567",
            "/privacy",
            "privacy",
            "../terms",
            "#section",
            "?lang=de",
            "//example.org/x",
            "",
            "/path:with:colons",
            "page?next=javascript:alert(1)",
            "#javascript:alert(1)",
            "  https://example.org",
        ] {
            assert!(is_safe_href(safe), "href {safe:?}");
        }
        for unsafe_href in [
            "javascript:alert(1)",
            "JaVaScRiPt:alert(1)",
            "\tjavascript:alert(1)",
            "\njavascript:alert(1)",
            " javascript:alert(1)",
            "\u{1}javascript:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "java\r\nscript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "DATA:text/html;base64,PHNjcmlwdD4=",
            "vbscript:msgbox(1)",
            "file:///etc/passwd",
            "ftp://example.org",
            "blob:https://example.org/uuid",
            "java script:alert(1)",
            "1http:foo",
            "ht!tp:foo",
            ":alert(1)",
        ] {
            assert!(!is_safe_href(unsafe_href), "href {unsafe_href:?}");
        }
    }

    #[test]
    fn external_means_http_or_https_after_the_same_cleaning() {
        assert!(is_external("https://example.org"));
        assert!(is_external("HTTP://example.org"));
        assert!(is_external("\thttps://example.org"));
        assert!(is_external("ht\ttps://example.org"));
        assert!(!is_external("mailto:a@b.c"));
        assert!(!is_external("/privacy"));
        assert!(!is_external("//example.org"));
    }
}
