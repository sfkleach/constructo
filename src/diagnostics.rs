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

/// An error found while checking a source, with the name of the file it was found in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub pos: Pos,
    pub message: String,
    /// Extra information, such as where a duplicated name was first declared.
    pub note: Option<String>,
}

impl Diagnostic {
    pub fn new(file: &str, pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            file: file.to_string(),
            pos,
            message: message.into(),
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ERROR: {} ({}, {})", self.message, self.file, self.pos)?;
        if let Some(note) = &self.note {
            write!(f, "; {}", note)?;
        }
        Ok(())
    }
}
