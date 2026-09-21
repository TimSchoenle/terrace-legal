//! Shim over [`terrace_legal_fuzz::oracle::locale_tag`], where the oracle and its documentation live.
//!
//! Gated on `cfg(fuzzing)`, which `cargo fuzz` sets and nothing else does. Without it this is a
//! binary with no `main`, and a plain `cargo test` in this directory would fail to link it.

#![cfg_attr(fuzzing, no_main)]

#[cfg(fuzzing)]
libfuzzer_sys::fuzz_target!(|data: &[u8]| terrace_legal_fuzz::oracle::locale_tag::check(data));

#[cfg(not(fuzzing))]
fn main() {
    eprintln!(
        "built without --cfg fuzzing; run this through `cargo +nightly fuzz run locale_tag`          or replay the corpus with `cargo test`"
    );
}
