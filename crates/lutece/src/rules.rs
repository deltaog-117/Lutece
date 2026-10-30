//! Manage rules: what to do with a window when it first opens, decided from what the
//! backend knows about it. Plain data, so a rule set can be compared, stored and replayed.

mod manage_rule;

pub use manage_rule::{ManageRule, Match, Outcome, Rules};
