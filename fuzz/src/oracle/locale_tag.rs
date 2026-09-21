//! Oracle for the locale grammar.

use terrace_legal_model::LocaleTag;

/// Parses arbitrary text as a locale and checks what a successful parse promises.
///
/// # Panics
///
/// When parsing panics, or when an accepted tag is not in canonical form, does not survive its own
/// text, `_` and serde, or has a truncation chain that does not shrink to its language.
pub fn check(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(tag) = text.parse::<LocaleTag>() else {
        return;
    };

    let language = tag.language();
    assert!(
        (2..=3).contains(&language.len()) && language.bytes().all(|b| b.is_ascii_lowercase()),
        "language {language:?} of {tag} is not two or three lowercase letters"
    );
    assert!(tag.as_str().starts_with(language));

    assert_eq!(tag.as_str().parse::<LocaleTag>().as_ref(), Ok(&tag));
    assert_eq!(tag.to_string(), tag.as_str());
    assert_eq!(
        tag.as_str().replace('-', "_").parse::<LocaleTag>().as_ref(),
        Ok(&tag),
        "`_` is accepted for `-`"
    );

    let json = serde_json::to_string(&tag).expect("a tag serialises");
    let decoded: LocaleTag = serde_json::from_str(&json).expect("a serialised tag deserialises");
    assert_eq!(decoded, tag);

    let mut length = tag.as_str().len();
    let mut current = tag.clone();
    let mut steps = 0;
    while let Some(shorter) = current.truncated() {
        assert!(shorter.as_str().len() < length, "truncation has to shrink");
        assert!(tag.as_str().starts_with(shorter.as_str()));
        assert!(shorter.same_language(&tag));
        length = shorter.as_str().len();
        current = shorter;
        steps += 1;
        assert!(steps <= 2, "a tag has at most three subtags");
    }
    assert_eq!(
        current.as_str(),
        language,
        "truncation ends at the language"
    );
}
