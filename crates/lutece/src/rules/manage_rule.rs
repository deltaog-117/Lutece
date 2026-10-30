//! Rules that decide how a newly opened window is managed.

use crate::shared::contracts::event::WindowInfo;
use crate::shared::ids::Tag;

/// What a rule looks at. One criterion each, so a rule always tests something; to need
/// two things, write two rules and order them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Match {
    /// The application class (X11 `WM_CLASS`) or app id (Wayland) is exactly this.
    Class(String),
    /// The window title is exactly this.
    Title(String),
    /// The window is a dialog or popup for another window.
    Dialog,
}

impl Match {
    fn matches(&self, info: &WindowInfo) -> bool {
        match self {
            Self::Class(class) => info.class == *class,
            Self::Title(title) => info.title == *title,
            Self::Dialog => info.transient_for.is_some(),
        }
    }
}

/// What happens to a window a rule matched.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Outcome {
    /// Leave the window alone: it is never tiled, focused or hidden by the core.
    Ignore,
    /// Open the window on the workspace with this tag, without switching to it.
    SendTo(Tag),
}

/// One condition and what to do with a window that meets it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ManageRule {
    when: Match,
    outcome: Outcome,
}

impl ManageRule {
    /// A rule that applies `outcome` to windows that meet `when`.
    #[must_use]
    pub const fn new(when: Match, outcome: Outcome) -> Self {
        Self { when, outcome }
    }

    /// The outcome, or `None` if this rule does not match `info`.
    #[must_use]
    pub fn outcome_for(&self, info: &WindowInfo) -> Option<&Outcome> {
        self.when.matches(info).then_some(&self.outcome)
    }
}

/// An ordered list of rules. The first rule that matches a window decides; a window no
/// rule matches is managed as usual.
///
/// # Examples
///
/// ```
/// use lutece::{ManageRule, Match, Outcome, Rules, WindowInfo};
///
/// let mut rules = Rules::new();
/// rules.push(ManageRule::new(Match::Class("Xmessage".into()), Outcome::Ignore));
///
/// let xmessage = WindowInfo { class: "Xmessage".into(), ..WindowInfo::default() };
/// assert_eq!(rules.outcome_for(&xmessage), Some(&Outcome::Ignore));
/// assert_eq!(rules.outcome_for(&WindowInfo::default()), None);
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Rules {
    rules: Vec<ManageRule>,
}

impl Rules {
    /// No rules: every window is managed as usual.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `rule` after the existing ones, so it only decides for windows they leave
    /// alone.
    pub fn push(&mut self, rule: ManageRule) {
        self.rules.push(rule);
    }

    /// The outcome of the first rule that matches `info`, if any.
    #[must_use]
    pub fn outcome_for(&self, info: &WindowInfo) -> Option<&Outcome> {
        self.rules.iter().find_map(|rule| rule.outcome_for(info))
    }

    /// The tags the rules send windows to, in rule order.
    pub fn tags(&self) -> impl Iterator<Item = &Tag> {
        self.rules.iter().filter_map(|rule| match &rule.outcome {
            Outcome::SendTo(tag) => Some(tag),
            Outcome::Ignore => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::WinId;
    use proptest::prelude::*;

    fn tag(name: &str) -> Tag {
        Tag::try_new(name).expect("test tags have text")
    }

    fn window(class: &str, title: &str) -> WindowInfo {
        WindowInfo {
            class: class.into(),
            title: title.into(),
            transient_for: None,
        }
    }

    #[test]
    fn each_criterion_tests_only_its_own_field() {
        let info = window("Firefox", "Mozilla");
        assert!(Match::Class("Firefox".into()).matches(&info));
        assert!(!Match::Class("Mozilla".into()).matches(&info));
        assert!(Match::Title("Mozilla".into()).matches(&info));
        assert!(!Match::Title("Firefox".into()).matches(&info));
    }

    #[test]
    fn a_dialog_is_a_window_with_a_parent() {
        let mut info = window("", "");
        assert!(!Match::Dialog.matches(&info));
        info.transient_for = Some(WinId::new(1));
        assert!(Match::Dialog.matches(&info));
    }

    #[test]
    fn the_first_matching_rule_decides() {
        let mut rules = Rules::new();
        rules.push(ManageRule::new(
            Match::Class("Firefox".into()),
            Outcome::SendTo(tag("3")),
        ));
        rules.push(ManageRule::new(
            Match::Class("Firefox".into()),
            Outcome::Ignore,
        ));
        assert_eq!(
            rules.outcome_for(&window("Firefox", "")),
            Some(&Outcome::SendTo(tag("3")))
        );
    }

    #[test]
    fn only_send_rules_name_a_tag() {
        let mut rules = Rules::new();
        rules.push(ManageRule::new(Match::Dialog, Outcome::Ignore));
        rules.push(ManageRule::new(
            Match::Class("Firefox".into()),
            Outcome::SendTo(tag("3")),
        ));
        assert_eq!(rules.tags().collect::<Vec<_>>(), [&tag("3")]);
    }

    proptest! {
        #[test]
        fn no_rules_means_no_outcome(class in ".*", title in ".*") {
            let rules = Rules::new();
            prop_assert_eq!(rules.outcome_for(&window(&class, &title)), None);
        }

        #[test]
        fn a_class_rule_matches_exactly_that_class(class in "[a-z]{0,6}", other in "[a-z]{0,6}") {
            let rule = ManageRule::new(Match::Class(class.clone()), Outcome::Ignore);
            prop_assert_eq!(
                rule.outcome_for(&window(&other, "")).is_some(),
                class == other
            );
        }
    }
}
