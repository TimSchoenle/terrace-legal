//! One oracle per fuzz target.
//!
//! An oracle panics when the code under test breaks a rule. It has to model the rule correctly:
//! one that reports a crash which is not a bug wastes the campaign, and the replay sweep in
//! `tests/replay.rs` is how that gets found out.

pub mod accept_language;
pub mod config_load;
pub mod locale_tag;
pub mod markdown;
