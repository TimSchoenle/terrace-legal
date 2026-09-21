//! No component contains a class name.
//!
//! A class literal in this crate would tie it to one stylesheet, and a utility-class framework
//! would have to be told to scan the library's source. Class names belong to the application's
//! skin, which its own build already scans.

use std::fs;
use std::path::{Path, PathBuf};

fn sources(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("the source directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

/// Returns each place in `text` where a `class` attribute is given something other than a value the
/// skin computed: a string literal, a raw string or a formatted string.
fn class_literals(text: &str) -> Vec<usize> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let code = line.split("//").next().unwrap_or_default();
        for marker in ["class:", "class ="] {
            let mut rest = code;
            while let Some(at) = rest.find(marker) {
                // The marker has to be a whole word, not the end of `subclass:` or `.class =`.
                let before = rest[..at].chars().next_back();
                let word_start =
                    before.is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'));
                let value = rest[at + marker.len()..].trim_start();
                if word_start
                    && (value.starts_with('"')
                        || value.starts_with("r#")
                        || value.starts_with("r\"")
                        || value.starts_with("format!"))
                {
                    found.push(index + 1);
                }
                rest = &rest[at + marker.len()..];
            }
        }
    }
    found
}

#[test]
fn no_component_names_a_class() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(
        files.len() >= 8,
        "the scan found the crate's sources: {files:?}"
    );

    for file in files {
        // The crate's own tests use class literals on purpose, in a fake skin.
        if file.file_name().is_some_and(|name| name == "tests.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).expect("readable");
        assert_eq!(
            class_literals(&text),
            Vec::<usize>::new(),
            "{} contains a class literal; ask the skin for it instead",
            file.display()
        );
    }
}

#[test]
fn the_scan_recognises_what_it_forbids() {
    assert_eq!(class_literals("a { class: \"x\" }"), [1]);
    assert_eq!(
        class_literals("let a = rsx! {\n  div { class: r#\"x\"# }\n};"),
        [2]
    );
    assert_eq!(class_literals("div { class: format!(\"a {b}\") }"), [1]);
    assert_eq!(class_literals("div { class = \"x\" }"), [1]);
}

#[test]
fn the_scan_allows_what_the_skin_computes() {
    assert!(class_literals("a { class, href: \"x\" }").is_empty());
    assert!(class_literals("a { class: page, href }").is_empty());
    assert!(class_literals("a { class: context.class(Part::Page) }").is_empty());
    assert!(class_literals("// class: \"x\" is what a component must not write").is_empty());
    assert!(class_literals("let subclass: &str = \"x\";").is_empty());
}
