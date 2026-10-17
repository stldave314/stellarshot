// SPDX-License-Identifier: GPL-3.0-only

//! [`Redacted`]: typed text that must never be printed.

/// Text the user typed that must never reach a log, a panic message or
/// debug output: a password being chosen, a client secret. Its `Debug` says
/// only that something is there; the text itself is reached with
/// [`Redacted::as_str`], where it is actually needed.
///
/// For text on its way to a repository, [`crate::engine::Secret`] also wipes
/// it from memory when dropped; this is for what a form holds while it is
/// being typed, compared and shown masked.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Redacted(String);

impl Redacted {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

impl From<String> for Redacted {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl From<&str> for Redacted {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

impl From<Redacted> for String {
    fn from(text: Redacted) -> Self {
        text.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_shows_the_text() {
        let typed = Redacted::from("hunter2");
        assert!(!format!("{typed:?}").contains("hunter2"));
        assert!(!format!("{:?}", Some((1, typed.clone()))).contains("hunter2"));
        assert_eq!(typed.as_str(), "hunter2");
    }
}
