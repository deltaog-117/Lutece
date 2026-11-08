//! Things the core asks a backend to do beyond placing windows.
//!
//! Window placement and focus are not effects: they travel in the
//! [`Scene`](super::scene::Scene), so a backend only reconciles what changed.

use crate::shared::error::{Error, Result};
use crate::shared::geometry::Rect;
use crate::shared::ids::{Tag, WinId};

/// An action for the backend to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[non_exhaustive]
pub enum Effect {
    /// Ask a window to close.
    Close(WinId),
    /// Start a program.
    Spawn(SpawnCommand),
    /// Stop managing a window and leave it to its client, which shows it where it asks.
    /// The core never mentions the window again.
    Release(WinId),
    /// Stop the window manager.
    Quit,
    /// Ask whatever provides the external layout where `windows` go on the workspace
    /// `tag`, which shows `area`. The answer is [`Event::LayoutProvided`](crate::Event::LayoutProvided)
    /// carrying the same `serial`; until it comes the workspace uses the default layout,
    /// and a request that is no longer current is not answered.
    LayoutRequested {
        /// The workspace.
        tag: Tag,
        /// The area the windows must fill, with bars left out.
        area: Rect,
        /// The tiled windows in reading order, master first.
        windows: Vec<WinId>,
        /// The focused window of the workspace, if any.
        focus: Option<WinId>,
        /// Names this request in the answer.
        serial: u64,
    },
}

/// A program and its arguments. The program name is never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SpawnCommand {
    program: String,
    args: Vec<String>,
}

impl SpawnCommand {
    /// Builds a command.
    ///
    /// # Errors
    ///
    /// Returns [`Error::EmptyProgram`] if `program` is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use lutece::SpawnCommand;
    ///
    /// let cmd = SpawnCommand::try_new("alacritty", ["-e", "htop"])?;
    /// assert_eq!(cmd.program(), "alacritty");
    /// assert!(SpawnCommand::try_new("", Vec::<String>::new()).is_err());
    /// # Ok::<(), lutece::Error>(())
    /// ```
    pub fn try_new<P, I, S>(program: P, args: I) -> Result<Self>
    where
        P: Into<String>,
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let program = program.into();
        if program.is_empty() {
            return Err(Error::EmptyProgram);
        }
        Ok(Self {
            program,
            args: args.into_iter().map(Into::into).collect(),
        })
    }

    /// The program to run.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// The arguments, in order.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_program_is_rejected() {
        let err = SpawnCommand::try_new("", ["-x"]).expect_err("empty program");
        assert_eq!(err, Error::EmptyProgram);
    }

    #[test]
    fn arguments_keep_their_order() {
        let cmd = SpawnCommand::try_new("sh", ["-c", "true"]).expect("valid command");
        assert_eq!(cmd.args(), ["-c", "true"]);
    }
}
