//! Source positions and diagnostics.

use std::fmt;

/// A position in a source text. Both fields are 1-based and the column counts characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl Pos {
    /// The position of a byte offset in a text, counting characters. An offset past the end is the end.
    pub fn at(text: &str, offset: usize) -> Pos {
        let before = &text[..offset.min(text.len())];
        let line = before.matches('\n').count() as u32 + 1;
        let col = before.rsplit('\n').next().map_or(0, |last| last.chars().count()) as u32 + 1;
        Pos { line, col }
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}", self.line, self.col)
    }
}

/// How serious a diagnostic is. An error means that the inputs are wrong, and a warning that they were used but
/// something was left out or may not be what was meant. Only the `convert` subcommand reports warnings at present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

impl Severity {
    /// The word that starts a line of this severity.
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "ERROR",
            Severity::Warning => "WARNING",
        }
    }
}

/// A problem found in a source, with the name of the file it was found in. It is an error unless it is made as a
/// warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub file: String,
    pub pos: Pos,
    pub message: String,
    /// Extra information, such as where a duplicated name was first declared.
    pub note: Option<String>,
}

impl Diagnostic {
    pub fn new(file: &str, pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            file: file.to_string(),
            pos,
            message: message.into(),
            note: None,
        }
    }

    /// A diagnostic that is a warning, which does not make a run fail.
    pub fn warning(file: &str, pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::new(file, pos, message)
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} ({}, {})",
            self.severity.label(),
            self.message,
            self.file,
            self.pos
        )?;
        if let Some(note) = &self.note {
            write!(f, "; {}", note)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Pos = Pos { line: 3, col: 9 };

    #[test]
    fn a_diagnostic_is_an_error_unless_it_is_made_as_a_warning() {
        assert_eq!(Diagnostic::new("f", AT, "m").severity, Severity::Error);
        assert_eq!(Diagnostic::warning("f", AT, "m").severity, Severity::Warning);
    }

    #[test]
    fn an_error_starts_with_error() {
        assert_eq!(
            Diagnostic::new("a.values", AT, "Bad value").to_string(),
            "ERROR: Bad value (a.values, line 3, col 9)"
        );
    }

    #[test]
    fn a_warning_has_the_same_form_and_starts_with_warning() {
        assert_eq!(
            Diagnostic::warning("a.values", AT, "More values follow").to_string(),
            "WARNING: More values follow (a.values, line 3, col 9)"
        );
        assert_eq!(
            Diagnostic::warning("a.values", AT, "More values follow")
                .with_note("see --to-array")
                .to_string(),
            "WARNING: More values follow (a.values, line 3, col 9); see --to-array"
        );
    }

    #[test]
    fn the_labels_are_the_words_that_start_a_line() {
        assert_eq!(Severity::Error.label(), "ERROR");
        assert_eq!(Severity::Warning.label(), "WARNING");
        assert_eq!(Severity::default(), Severity::Error);
    }
}
