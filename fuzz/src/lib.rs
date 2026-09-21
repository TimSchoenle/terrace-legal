//! The fuzz oracles, as an ordinary library.
//!
//! Each `fuzz_targets/*.rs` binary is a shim over one function in [`oracle`]. The bodies live here
//! so they can be replayed without libFuzzer: `tests/replay.rs` runs every committed seed and a
//! deterministic sweep through the matching oracle on a plain `cargo test`.
//!
//! The split is not only a convenience. `cargo fuzz` needs a nightly-only sanitizer, and an oracle
//! that can only run under it is an oracle nobody checks. Here the seeds are a regression suite that
//! runs anywhere, and the fuzzer discovers new inputs to add to it.

pub mod oracle;
