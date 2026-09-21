//! Replays the committed corpus through the oracles, without libFuzzer.
//!
//! Two jobs. Regression: every seed under `seeds/`, and every input a campaign promoted into
//! `corpus/`, runs through its oracle on a plain `cargo test`, so a reproducer keeps being checked
//! long after whoever found it moved on. Validation: [`sweep`] runs each oracle over inputs built
//! to hit its edges from a fixed seed, so an oracle that models a rule wrongly fails here, by test
//! name, instead of as a blob in a corpus.

use std::path::{Path, PathBuf};

use terrace_legal_fuzz::oracle;

type Oracle = fn(&[u8]);

fn directory(kind: &str, target: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(kind)
        .join(target)
}

/// Runs every file in `dir` through `oracle`, and returns how many there were.
///
/// A missing directory is not a failure: `corpus/` is where a campaign writes, and a fresh clone
/// holds only its `.gitkeep`.
fn replay(dir: &Path, oracle: Oracle) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut replayed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.file_name().is_some_and(|name| name == ".gitkeep") {
            continue;
        }
        let data = std::fs::read(&path).expect("a corpus file is readable");
        // No `catch_unwind`: a panic is the finding, and the harness names the test.
        oracle(&data);
        replayed += 1;
    }
    replayed
}

fn replay_target(target: &str, oracle: Oracle) {
    let seeds = replay(&directory("seeds", target), oracle);
    assert!(
        seeds > 0,
        "no seeds found for `{target}`: the corpus is what makes this test mean anything"
    );
    replay(&directory("corpus", target), oracle);
}

/// A small deterministic generator, so a failure reproduces from the test name alone.
struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % (bound as u64)).expect("fits")
    }
}

fn iterations() -> usize {
    std::env::var("LEGAL_FUZZ_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(500)
}

/// Feeds `oracle` inputs assembled from `tokens`, and from raw bytes.
fn sweep(seed: u64, tokens: &[&str], oracle: Oracle) {
    let mut random = Xorshift(seed);
    for _ in 0..iterations() {
        let pieces = 1 + random.below(24);
        let mut input = Vec::new();
        for _ in 0..pieces {
            input.extend_from_slice(tokens[random.below(tokens.len())].as_bytes());
        }
        oracle(&input);

        let length = random.below(64);
        let raw: Vec<u8> = (0..length)
            .map(|_| u8::try_from(random.next() & 0xff).expect("a byte"))
            .collect();
        oracle(&raw);
    }
}

#[test]
fn accept_language_seeds() {
    replay_target("accept_language", oracle::accept_language::check);
}

#[test]
fn locale_tag_seeds() {
    replay_target("locale_tag", oracle::locale_tag::check);
}

#[test]
fn config_load_seeds() {
    replay_target("config_load", oracle::config_load::check);
}

#[test]
fn markdown_seeds() {
    replay_target("markdown", oracle::markdown::check);
}

#[test]
fn accept_language_sweep() {
    sweep(
        0x01,
        &[
            "en",
            "de",
            "de-AT",
            "zh-Hant-TW",
            "fr-CA",
            "*",
            ",",
            ", ",
            ";",
            ";q=",
            "q=0",
            "0.5",
            "1",
            "0",
            ".",
            "1.000",
            "0.001",
            "x-klingon",
            "de-CH-1996",
            "\u{ff}",
            "\t",
            " ",
            "=",
            "-",
            "_",
        ],
        oracle::accept_language::check,
    );
}

#[test]
fn locale_tag_sweep() {
    sweep(
        0x02,
        &[
            "de", "EN", "zh", "Hant", "hant", "AT", "at", "419", "-", "_", "--", "x", "1",
            "\u{e9}", "", " ",
        ],
        oracle::locale_tag::check,
    );
}

#[test]
fn config_load_sweep() {
    sweep(
        0x03,
        &[
            "{",
            "}",
            "\"documents\"",
            ":",
            ",",
            "\"terms\"",
            "\"body\"",
            "\"en\"",
            "\"de_at\"",
            "\"# T\"",
            "\"url\"",
            "\"https://example.org/\"",
            "\"http://\"",
            "\"order\"",
            "3",
            "-1",
            "\"title\"",
            "\"updated\"",
            "\"default_locale\"",
            "\"consent\"",
            "\"requirement\"",
            "\"accept\"",
            "\"version\"",
            "\"1\"",
            "\"effective\"",
            "\"2026-09-01\"",
            "\"grace_days\"",
            "14",
            "null",
            "[",
            "]",
            "\"\"",
            "\" \"",
        ],
        oracle::config_load::check,
    );
}

#[test]
fn markdown_sweep() {
    sweep(
        0x04,
        &[
            "# ",
            "## ",
            "* ",
            "- ",
            "1. ",
            "> ",
            "```",
            "`",
            "**",
            "_",
            "~~",
            "[",
            "]",
            "(",
            ")",
            "![",
            "<",
            ">",
            "&",
            "\n",
            "\n\n",
            " ",
            "javascript:",
            "java\tscript:",
            "data:",
            "https://x.y/",
            "mailto:a@b.c",
            "<script>",
            "</script>",
            "onerror=",
            "|",
            "---",
            ":",
            "\\",
            "&#106;",
            "&#x6A;",
            "text",
            "[ref]: ",
        ],
        oracle::markdown::check,
    );
}
