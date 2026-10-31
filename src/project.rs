//! Projects: a folder with a project-file, `constructo.toml`, and the types files that the file names.
//!
//! The project-file is TOML and everything in it is strict, like the configuration file: an unknown key or a value that
//! is not valid is an error with a line and a column.

use crate::diagnostics::{Diagnostic, Pos};
use glob::{MatchOptions, Pattern};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};
use toml::Spanned;

/// The name of the project-file, in the root of the project folder.
pub const PROJECT_FILE: &str = "constructo.toml";

/// The pattern for the types files of a project when the project-file does not give one.
pub const DEFAULT_INCLUDE: &str = "**/*.types";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    project: ProjectTable,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectTable {
    name: Spanned<String>,
    include: Option<Vec<Spanned<String>>>,
    exclude: Option<Vec<Spanned<String>>>,
}

/// A project that has been loaded: its name, and the types files that it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    /// The folder as it was given, so the paths below are as the user would type them.
    pub folder: PathBuf,
    /// The types files of the project, each as the folder joined to the path within it, sorted by that path.
    pub files: Vec<PathBuf>,
}

/// Why a project could not be loaded. Both kinds are problems with the invocation, not with the inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    /// The folder or the project-file could not be read.
    Cannot(String),
    /// The project-file is not valid.
    Invalid(Diagnostic),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Cannot(message) => write!(f, "ERROR: {}", message),
            ProjectError::Invalid(diagnostic) => write!(f, "{}", diagnostic),
        }
    }
}

impl std::error::Error for ProjectError {}

/// The patterns are matched against a path with `/` as the separator, where a `*` does not cross one.
const MATCH: MatchOptions = MatchOptions {
    case_sensitive: true,
    require_literal_separator: true,
    require_literal_leading_dot: false,
};

impl Project {
    /// Reads the project-file in the folder and finds the types files that it names.
    pub fn load(folder: &Path) -> Result<Project, ProjectError> {
        let path = folder.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| ProjectError::Cannot(format!("Cannot read {}: {}", path.display(), e)))?;
        let (name, include, exclude) = Project::parse_file(&text, &path.display().to_string())?;
        let mut found = Vec::new();
        walk(folder, "", true, &include, &exclude, &mut found)
            .map_err(|e| ProjectError::Cannot(format!("Cannot read {}: {}", folder.display(), e)))?;
        found.sort();
        let files = found.iter().map(|relative| folder.join(relative)).collect();
        Ok(Project {
            name,
            folder: folder.to_path_buf(),
            files,
        })
    }

    /// Reads the text of a project-file, returning the name and the include and exclude patterns. `file` is the name of
    /// the file in diagnostics.
    fn parse_file(text: &str, file: &str) -> Result<(String, Vec<Pattern>, Vec<Pattern>), ProjectError> {
        let at = |offset: usize| Pos::at(text, offset);
        let invalid =
            |offset: usize, message: String| ProjectError::Invalid(Diagnostic::new(file, at(offset), message));
        let parsed: ProjectFile = toml::from_str(text)
            .map_err(|e| invalid(e.span().map_or(0, |span| span.start), e.message().to_string()))?;
        let table = parsed.project;
        if let Err(reason) = check_name(table.name.get_ref()) {
            let message = format!("Invalid project name \"{}\": {}", table.name.get_ref(), reason);
            return Err(invalid(table.name.span().start, message));
        }
        let patterns = |given: Option<Vec<Spanned<String>>>, default: &[&str]| -> Result<Vec<Pattern>, ProjectError> {
            match given {
                Some(list) => list
                    .iter()
                    .map(|item| {
                        Pattern::new(item.get_ref()).map_err(|e| {
                            invalid(
                                item.span().start,
                                format!("Invalid pattern \"{}\": {}", item.get_ref(), e.msg),
                            )
                        })
                    })
                    .collect(),
                None => Ok(default
                    .iter()
                    .map(|pattern| Pattern::new(pattern).expect("the default pattern is valid"))
                    .collect()),
            }
        };
        let include = patterns(table.include, &[DEFAULT_INCLUDE])?;
        let exclude = patterns(table.exclude, &[])?;
        Ok((table.name.into_inner(), include, exclude))
    }
}

/// Checks that the name is one or more segments separated by dots, where a segment starts with a letter and has
/// letters, digits, `_` and `-`. The reason is returned if it is not.
fn check_name(name: &str) -> Result<(), String> {
    for segment in name.split('.') {
        let mut chars = segment.chars();
        match chars.next() {
            None => return Err("each dot-separated part must have a name".to_string()),
            Some(first) if !first.is_ascii_alphabetic() => {
                return Err(format!("the part \"{}\" must start with a letter", segment));
            }
            Some(_) => {}
        }
        if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '-')) {
            return Err(format!(
                "the part \"{}\" has '{}', but only letters, digits, '_' and '-' are allowed",
                segment, bad
            ));
        }
    }
    Ok(())
}

/// Collects the paths, within the project, of the files that match. The path of a file is relative to the project
/// folder, written with `/`.
///
/// A hidden folder is skipped, and so is a subfolder with a project-file of its own, which is another project. The
/// root is never skipped. A symbolic link to a folder is not followed, so that a loop cannot make the walk endless.
fn walk(
    dir: &Path,
    relative: &str,
    root: bool,
    include: &[Pattern],
    exclude: &[Pattern],
    found: &mut Vec<String>,
) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let within = if root {
            name.clone()
        } else {
            format!("{}/{}", relative, name)
        };
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !name.starts_with('.') && !path.join(PROJECT_FILE).is_file() {
                walk(&path, &within, false, include, exclude, found)?;
            }
        } else if (kind.is_file() || (kind.is_symlink() && path.is_file()))
            && include.iter().any(|pattern| pattern.matches_with(&within, MATCH))
            && !exclude.iter().any(|pattern| pattern.matches_with(&within, MATCH))
        {
            found.push(within);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<(String, Vec<Pattern>, Vec<Pattern>), ProjectError> {
        Project::parse_file(text, "constructo.toml")
    }

    fn error(text: &str) -> (u32, u32, String) {
        match parse(text).unwrap_err() {
            ProjectError::Invalid(d) => (d.pos.line, d.pos.col, d.message),
            other => panic!("not a diagnostic: {}", other),
        }
    }

    #[test]
    fn a_name_alone_is_a_project() {
        let (name, include, exclude) = parse("[project]\nname = \"com.example.shapes\"\n").unwrap();
        assert_eq!(name, "com.example.shapes");
        assert_eq!(
            include.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            [DEFAULT_INCLUDE]
        );
        assert!(exclude.is_empty());
    }

    #[test]
    fn the_patterns_can_be_given() {
        let text = "[project]\nname = \"a\"\ninclude = [\"src/*.types\", \"x/**/*.types\"]\nexclude = [\"**/old/*\"]\n";
        let (_, include, exclude) = parse(text).unwrap();
        assert_eq!(include.len(), 2);
        assert_eq!(exclude.len(), 1);
        // An empty include list takes nothing, which is not the same as leaving it out.
        let (_, include, _) = parse("[project]\nname = \"a\"\ninclude = []\n").unwrap();
        assert!(include.is_empty());
    }

    #[test]
    fn the_name_follows_the_rule() {
        for good in ["a", "A1", "com.example", "x_y.z-w.q9"] {
            assert!(check_name(good).is_ok(), "{}", good);
        }
        for bad in [
            "", ".a", "a.", "a..b", "1a", "a.1b", "a b", "a/b", "a:b", "é", "_a", "-a",
        ] {
            assert!(check_name(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn a_bad_name_is_reported_where_it_is_written() {
        let (line, col, message) = error("[project]\nname = \"1st\"\n");
        assert_eq!((line, col), (2, 8));
        assert!(message.contains("Invalid project name \"1st\""), "{}", message);
    }

    #[test]
    fn everything_is_strict() {
        let (line, col, message) = error("[project]\nname = \"a\"\nversion = \"1\"\n");
        assert_eq!((line, col), (3, 1));
        assert!(
            message.contains("unknown field `version`") && message.contains("`name`"),
            "{}",
            message
        );
        assert!(error("[project]\nname = \"a\"\n[extra]\n")
            .2
            .contains("unknown field `extra`"));
        assert!(error("[project]\n").2.contains("missing field `name`"));
        assert!(error("name = \"a\"\n").2.contains("unknown field `name`"));
        assert!(error("").2.contains("missing field `project`"));
        assert!(error("[project]\nname = 1\n").2.contains("invalid type"));
        assert!(error("[project]\nname = \"a\"\ninclude = \"x\"\n")
            .2
            .contains("invalid type"));
        assert_eq!(error("[project\n").0, 1);
    }

    #[test]
    fn a_pattern_that_is_not_valid_is_reported_where_it_is_written() {
        let (line, col, message) = error("[project]\nname = \"a\"\nexclude = [\"fine\", \"[\"]\n");
        assert_eq!((line, col), (3, 20));
        assert!(message.contains("Invalid pattern \"[\""), "{}", message);
    }

    #[test]
    fn the_patterns_match_the_way_the_task_says() {
        let default = Pattern::new(DEFAULT_INCLUDE).unwrap();
        for yes in ["a.types", "x/a.types", "x/y/z/a.types"] {
            assert!(default.matches_with(yes, MATCH), "{}", yes);
        }
        for no in ["a.values", "a.types.bak", "x/a.values"] {
            assert!(!default.matches_with(no, MATCH), "{}", no);
        }
        // A star does not cross a folder.
        assert!(!Pattern::new("*.types").unwrap().matches_with("x/a.types", MATCH));
    }
}
