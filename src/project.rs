//! Projects: a folder with a project-file, `constructo.toml`, and the types files that the file names.
//!
//! The project-file is TOML and everything in it is strict, like the configuration file: an unknown key or a value that
//! is not valid is an error with a line and a column.

use crate::diagnostics::{Diagnostic, Pos};
use glob::{MatchOptions, Pattern};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};
use toml::Spanned;

/// The name of the project-file, in the root of the project folder.
pub const PROJECT_FILE: &str = "constructo.toml";

/// The pattern for the types files of a project when the project-file does not give one.
pub const DEFAULT_INCLUDE: &str = "**/*.types";

/// The name of the standard prelude, which is a project that is built in.
pub const PRELUDE_PROJECT: &str = "constructo.prelude";

/// The start of the source of a built-in project, and of the name of one. The `constructo` prefix is reserved for them.
const BUILTIN_SCHEME: &str = "constructo:";
const BUILTIN_PREFIX: &str = "constructo.";

/// A project that is held inside the binary. Its source is a URI that names the project as a folder would.
#[derive(Debug, PartialEq, Eq)]
pub struct Builtin {
    pub name: &'static str,
    pub uri: &'static str,
    /// The types files of the project, each with its name within the project and its text.
    pub files: &'static [(&'static str, &'static str)],
}

/// The projects that are built in.
pub const BUILTINS: &[Builtin] = &[Builtin {
    name: PRELUDE_PROJECT,
    uri: "constructo:/prelude",
    files: &[("prelude.types", crate::PRELUDE)],
}];

fn builtin_for_uri(uri: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.uri == uri)
}

/// The built-in projects as they are listed in a message.
fn builtin_list() -> String {
    BUILTINS
        .iter()
        .map(|builtin| builtin.uri)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Whether a source names a built-in project and not a folder.
fn is_builtin_source(source: &str) -> bool {
    source.starts_with(BUILTIN_SCHEME)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectFile {
    project: ProjectTable,
    #[serde(default)]
    import: Vec<ImportTable>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectTable {
    name: Spanned<String>,
    include: Option<Vec<Spanned<String>>>,
    exclude: Option<Vec<Spanned<String>>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ImportTable {
    project_name: Spanned<String>,
    source: Spanned<String>,
    group: Option<Spanned<String>>,
}

/// An `[[import]]` of a project-file: a project that is brought in, and the group that it is put into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The full name that the project-file says the imported project has.
    pub project_name: String,
    /// The group the project is put into. Without one the project can only be named by its full name.
    pub group: Option<String>,
    /// The folder of the imported project, as the importing folder joined to the `source`.
    pub folder: PathBuf,
    /// The index of the imported project in the `ProjectSet`. This is `None` until the set has been loaded.
    pub target: Option<usize>,
    name_pos: Pos,
    source_pos: Pos,
}

/// A types file of a project. A file of a built-in project has its text with it, since it is not on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    pub path: PathBuf,
    pub embedded: Option<&'static str>,
}

/// A project that has been loaded: its name, its imports and the types files that it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    /// The folder as it was given, so the paths below are as the user would type them.
    pub folder: PathBuf,
    /// The projects that this one imports, in the order in the project-file.
    pub imports: Vec<Import>,
    /// The types files of the project, each as the folder joined to the path within it, sorted by that path.
    pub files: Vec<InputFile>,
    /// True for a project that is built in, which is not reported as an input.
    pub builtin: bool,
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

/// What a project-file says, with the patterns compiled.
struct ParsedFile {
    name: String,
    include: Vec<Pattern>,
    exclude: Vec<Pattern>,
    imports: Vec<Import>,
}

impl Project {
    /// A project that is built in.
    fn from_builtin(builtin: &'static Builtin) -> Project {
        let folder = PathBuf::from(builtin.uri);
        let files = builtin
            .files
            .iter()
            .map(|(name, text)| InputFile {
                path: folder.join(name),
                embedded: Some(text),
            })
            .collect();
        Project {
            name: builtin.name.to_string(),
            folder,
            imports: Vec::new(),
            files,
            builtin: true,
        }
    }

    /// Reads the project-file in the folder and finds the types files that it names. The imports are read but the
    /// projects that they name are not loaded, which `ProjectSet::load` does. A folder that is the URI of a built-in
    /// project gives that project.
    pub fn load(folder: &Path) -> Result<Project, ProjectError> {
        if let Some(uri) = folder.to_str().filter(|uri| is_builtin_source(uri)) {
            return builtin_for_uri(uri).map(Project::from_builtin).ok_or_else(|| {
                ProjectError::Cannot(format!(
                    "There is no built-in project {}; the built-in projects are {}",
                    uri,
                    builtin_list()
                ))
            });
        }
        let path = folder.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| ProjectError::Cannot(format!("Cannot read {}: {}", path.display(), e)))?;
        let parsed = Project::parse_file(&text, &path.display().to_string(), folder)?;
        let mut found = Vec::new();
        walk(folder, "", true, &parsed.include, &parsed.exclude, &mut found)
            .map_err(|e| ProjectError::Cannot(format!("Cannot read {}: {}", folder.display(), e)))?;
        found.sort();
        let files = found
            .iter()
            .map(|relative| InputFile {
                path: folder.join(relative),
                embedded: None,
            })
            .collect();
        Ok(Project {
            name: parsed.name,
            folder: folder.to_path_buf(),
            imports: parsed.imports,
            files,
            builtin: false,
        })
    }

    /// The name of the project-file in diagnostics.
    fn project_file(&self) -> String {
        self.folder.join(PROJECT_FILE).display().to_string()
    }

    /// Reads the text of a project-file. `file` is the name of the file in diagnostics, and a relative `source` of an
    /// import is relative to `folder`.
    fn parse_file(text: &str, file: &str, folder: &Path) -> Result<ParsedFile, ProjectError> {
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
        if table.name.get_ref().starts_with(BUILTIN_PREFIX) {
            let message = format!(
                "Invalid project name \"{}\": names that start with {} are for built-in projects",
                table.name.get_ref(),
                BUILTIN_PREFIX
            );
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
        let mut imports = Vec::new();
        for entry in parsed.import {
            let name = entry.project_name.get_ref();
            if let Err(reason) = check_name(name) {
                let message = format!("Invalid project name \"{}\": {}", name, reason);
                return Err(invalid(entry.project_name.span().start, message));
            }
            if let Some(group) = &entry.group {
                if let Err(reason) = check_group(group.get_ref()) {
                    let message = format!("Invalid group name \"{}\": {}", group.get_ref(), reason);
                    return Err(invalid(group.span().start, message));
                }
            }
            let source = entry.source.get_ref();
            if source.is_empty() {
                return Err(invalid(
                    entry.source.span().start,
                    "The source of an import is empty".to_string(),
                ));
            }
            // A built-in project has to be named by both its name and its source, so that neither can be mistaken.
            let (named_builtin, source_builtin) = (name.starts_with(BUILTIN_PREFIX), is_builtin_source(source));
            if named_builtin != source_builtin {
                let message = if named_builtin {
                    format!(
                        "{} is a built-in project, so its source has to be one of {}",
                        name,
                        builtin_list()
                    )
                } else {
                    format!(
                        "The source {} is a built-in project, so the project name has to be a built-in one",
                        source
                    )
                };
                return Err(invalid(entry.source.span().start, message));
            }
            if source_builtin && builtin_for_uri(source).is_none() {
                let message = format!(
                    "There is no built-in project {}; the built-in projects are {}",
                    source,
                    builtin_list()
                );
                return Err(invalid(entry.source.span().start, message));
            }
            imports.push(Import {
                project_name: name.clone(),
                group: entry.group.map(Spanned::into_inner),
                folder: if source_builtin {
                    PathBuf::from(source)
                } else {
                    folder.join(source)
                },
                target: None,
                name_pos: at(entry.project_name.span().start),
                source_pos: at(entry.source.span().start),
            });
        }
        Ok(ParsedFile {
            name: table.name.into_inner(),
            include,
            exclude,
            imports,
        })
    }
}

/// The projects that a run uses: the one that was named, and every project that it imports, directly or not.
///
/// The projects are in dependency order, so a project comes after every project that it imports and the one that was
/// named is last. Each project is in the set once, however many times it is imported. Imports are not transitive for
/// the names that a project can use, but all of them are loaded and checked, because a project's types can name those
/// of the projects that it imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSet {
    pub projects: Vec<Project>,
}

impl ProjectSet {
    /// Loads the project in the folder and, following the imports to any depth, the projects that it imports.
    ///
    /// Unless `prelude` is false, every project that does not import the standard prelude itself is given an import of
    /// it into the group `local`, as its first import. A project that does import it has said what it wants.
    pub fn load(folder: &Path, prelude: bool) -> Result<ProjectSet, ProjectError> {
        let mut loader = Loader {
            implicit_prelude: prelude,
            ..Loader::default()
        };
        let root = Project::load(folder)?;
        loader.visit(root)?;
        Ok(ProjectSet {
            projects: loader.projects,
        })
    }

    /// The set for a run with no project named: a project with no name, no files and nothing but the implicit import
    /// of the prelude, unless `prelude` is false. The files that are given are written in it as if they were its own,
    /// which is how they are treated when a project is named too.
    pub fn unnamed(prelude: bool) -> ProjectSet {
        let mut loader = Loader {
            implicit_prelude: prelude,
            ..Loader::default()
        };
        let root = Project {
            name: String::new(),
            folder: PathBuf::new(),
            imports: Vec::new(),
            files: Vec::new(),
            builtin: false,
        };
        loader
            .visit(root)
            .expect("a project with no files and only the prelude can always be loaded");
        ProjectSet {
            projects: loader.projects,
        }
    }

    /// The project that was named, which is the last.
    pub fn root(&self) -> &Project {
        self.projects
            .last()
            .expect("a set holds at least the project that was named")
    }
}

#[derive(Default)]
struct Loader {
    /// The projects that are finished, in dependency order.
    projects: Vec<Project>,
    /// The index in `projects` of each finished project, by its folder once it has been made absolute.
    done: HashMap<PathBuf, usize>,
    /// The projects that are being loaded now, outermost first, with the name of each, to find a cycle.
    stack: Vec<(PathBuf, String)>,
    /// The folder of each project name that has been seen, so that one name is not two projects.
    names: HashMap<String, PathBuf>,
    /// Whether a project that does not import the prelude is given the import of it.
    implicit_prelude: bool,
}

impl Loader {
    /// Loads the imports of a project and then adds the project to the set, returning its index.
    fn visit(&mut self, mut project: Project) -> Result<usize, ProjectError> {
        if self.implicit_prelude
            && !project.builtin
            && !project
                .imports
                .iter()
                .any(|import| import.project_name == PRELUDE_PROJECT)
        {
            let prelude = BUILTINS
                .iter()
                .find(|builtin| builtin.name == PRELUDE_PROJECT)
                .expect("the prelude is built in");
            let start = Pos { line: 1, col: 1 };
            project.imports.insert(
                0,
                Import {
                    project_name: PRELUDE_PROJECT.to_string(),
                    group: Some("local".to_string()),
                    folder: PathBuf::from(prelude.uri),
                    target: None,
                    name_pos: start,
                    source_pos: start,
                },
            );
        }
        let canonical = absolute(&project.folder)
            .map_err(|e| ProjectError::Cannot(format!("Cannot read {}: {}", project.folder.display(), e)))?;
        self.names.insert(project.name.clone(), canonical.clone());
        self.stack.push((canonical, project.name.clone()));
        let file = project.project_file();
        let mut seen: HashSet<(PathBuf, Option<String>)> = HashSet::new();
        for index in 0..project.imports.len() {
            let import = &project.imports[index];
            let (name_pos, source_pos) = (import.name_pos, import.source_pos);
            let problem = |pos: Pos, message: String| ProjectError::Invalid(Diagnostic::new(&file, pos, message));
            let child_canonical = absolute(&import.folder).map_err(|e| {
                problem(
                    source_pos,
                    format!("Cannot read the project at {}: {}", import.folder.display(), e),
                )
            })?;
            if !seen.insert((child_canonical.clone(), import.group.clone())) {
                let message = match &import.group {
                    Some(group) => format!(
                        "{} is imported into the group {} more than once",
                        import.project_name, group
                    ),
                    None => format!("{} is imported with no group more than once", import.project_name),
                };
                return Err(problem(source_pos, message));
            }
            let target = if let Some(&done) = self.done.get(&child_canonical) {
                self.check_name_of(&file, import, name_pos, &self.projects[done].name)?;
                done
            } else if let Some(at) = self.stack.iter().position(|(path, _)| *path == child_canonical) {
                let mut names: Vec<&str> = self.stack[at..].iter().map(|(_, name)| name.as_str()).collect();
                names.push(&self.stack[at].1);
                return Err(problem(
                    source_pos,
                    format!("The imports form a cycle: {}", names.join(" -> ")),
                ));
            } else {
                let child = Project::load(&import.folder)?;
                self.check_name_of(&file, import, name_pos, &child.name)?;
                if let Some(other) = self.names.get(&child.name) {
                    if *other != child_canonical {
                        let message = format!(
                            "The project {} is in two places: {} and {}",
                            child.name,
                            other.display(),
                            child_canonical.display()
                        );
                        return Err(problem(source_pos, message));
                    }
                }
                self.visit(child)?
            };
            project.imports[index].target = Some(target);
        }
        let (canonical, _) = self.stack.pop().expect("this project was pushed above");
        self.projects.push(project);
        self.done.insert(canonical, self.projects.len() - 1);
        Ok(self.projects.len() - 1)
    }

    /// Checks that the project that an import found has the name that the import says.
    fn check_name_of(&self, file: &str, import: &Import, pos: Pos, found: &str) -> Result<(), ProjectError> {
        if import.project_name == found {
            return Ok(());
        }
        let message = format!(
            "The import says the project is named {} but the project at {} is named {}",
            import.project_name,
            import.folder.display(),
            found
        );
        Err(ProjectError::Invalid(Diagnostic::new(file, pos, message)))
    }
}

/// The folder as an absolute path with no links or `..`, which is the identity of a project. A built-in project is
/// identified by its URI.
fn absolute(folder: &Path) -> std::io::Result<PathBuf> {
    // The project with no name has no folder either.
    if folder.as_os_str().is_empty() || folder.to_str().is_some_and(is_builtin_source) {
        return Ok(folder.to_path_buf());
    }
    std::fs::canonicalize(folder)
}

/// Checks that the group name is a single part, which starts with a letter and has letters, digits and `_`. The
/// reason is returned if it is not valid.
fn check_group(group: &str) -> Result<(), String> {
    if group.contains('.') {
        return Err("a group name has no dot, because a dot makes it the full name of a project".to_string());
    }
    let mut chars = group.chars();
    match chars.next() {
        None => return Err("a group name cannot be empty".to_string()),
        Some(first) if !first.is_ascii_alphabetic() => return Err("it must start with a letter".to_string()),
        Some(_) => {}
    }
    match chars.find(|c| !(c.is_ascii_alphanumeric() || *c == '_')) {
        Some(bad) => Err(format!(
            "it has '{}', but only letters, digits and '_' are allowed",
            bad
        )),
        None => Ok(()),
    }
}

/// Checks that the name is two or more segments separated by dots, where a segment starts with a letter and has
/// letters, digits and `_`, of either case. A name has to be written in source text, so `-` is not allowed, and the
/// dot tells a project name from a group name, which has none. The reason is returned if it is not valid.
fn check_name(name: &str) -> Result<(), String> {
    if !name.contains('.') {
        return Err("a project name needs a dot, as in \"com.example\", to tell it from a group name".to_string());
    }
    for segment in name.split('.') {
        let mut chars = segment.chars();
        match chars.next() {
            None => return Err("each dot-separated part must have a name".to_string()),
            Some(first) if !first.is_ascii_alphabetic() => {
                return Err(format!("the part \"{}\" must start with a letter", segment));
            }
            Some(_) => {}
        }
        if let Some(bad) = chars.find(|c| !(c.is_ascii_alphanumeric() || *c == '_')) {
            return Err(format!(
                "the part \"{}\" has '{}', but only letters, digits and '_' are allowed",
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
        let parsed = Project::parse_file(text, "constructo.toml", Path::new("here"))?;
        Ok((parsed.name, parsed.include, parsed.exclude))
    }

    fn imports(text: &str) -> Vec<Import> {
        Project::parse_file(text, "constructo.toml", Path::new("here"))
            .unwrap()
            .imports
    }

    fn error(text: &str) -> (u32, u32, String) {
        match parse(text).unwrap_err() {
            ProjectError::Invalid(d) => (d.pos.line, d.pos.col, d.message),
            other => panic!("not a diagnostic: {}", other),
        }
    }

    const ONE: &str = "[project]\nname = \"a.b\"\n";

    #[test]
    fn an_import_has_a_name_a_source_and_perhaps_a_group() {
        let text = [
            ONE,
            "\n[[import]]\nproject-name = \"P.Q.R\"\ngroup = \"pqrxyz\"\nsource = \"../sister\"\n",
            "\n[[import]]\nproject-name = \"x.y\"\nsource = \"/abs/x\"\n",
        ]
        .concat();
        let imports = imports(&text);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0].project_name, "P.Q.R");
        assert_eq!(imports[0].group.as_deref(), Some("pqrxyz"));
        // A relative source is relative to the importing folder, and an absolute one is not.
        assert_eq!(imports[0].folder, Path::new("here/../sister"));
        assert_eq!(imports[1].group, None);
        assert_eq!(imports[1].folder, Path::new("/abs/x"));
        assert_eq!(imports[0].target, None);
        assert_eq!((imports[0].name_pos.line, imports[0].source_pos.line), (5, 7));
    }

    #[test]
    fn an_import_is_strict() {
        let bad = |entry: &str| error(&format!("{}\n[[import]]\n{}", ONE, entry));
        assert!(bad("source = \"x\"\n").2.contains("missing field `project-name`"));
        assert!(bad("project-name = \"x.y\"\n").2.contains("missing field `source`"));
        let (line, _, message) = bad("project-name = \"x.y\"\nsource = \"s\"\nversion = 1\n");
        assert_eq!(line, 7);
        assert!(message.contains("unknown field `version`"), "{}", message);
        assert!(bad("project-name = 1\nsource = \"s\"\n").2.contains("invalid type"));
        assert!(bad("project-name = \"xy\"\nsource = \"s\"\n")
            .2
            .contains("Invalid project name \"xy\""));
        assert!(bad("project-name = \"x.y\"\nsource = \"\"\n")
            .2
            .contains("source of an import is empty"));
        let (line, col, message) = bad("project-name = \"x.y\"\nsource = \"s\"\ngroup = \"a.b\"\n");
        assert_eq!((line, col), (7, 9));
        assert!(message.contains("a group name has no dot"), "{}", message);
        // An `import` that is not an array of tables is not accepted either.
        assert!(!error("[project]\nname = \"a.b\"\nimport = 1\n").2.is_empty());
    }

    #[test]
    fn a_group_name_is_a_single_part() {
        for good in ["a", "local", "pqrxyz", "G1", "x_y", "A"] {
            assert!(check_group(good).is_ok(), "{}", good);
        }
        for bad in ["", "a.b", "1a", "_a", "a-b", "a b", "a:", "é"] {
            assert!(check_group(bad).is_err(), "{}", bad);
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
        let text =
            "[project]\nname = \"a.b\"\ninclude = [\"src/*.types\", \"x/**/*.types\"]\nexclude = [\"**/old/*\"]\n";
        let (_, include, exclude) = parse(text).unwrap();
        assert_eq!(include.len(), 2);
        assert_eq!(exclude.len(), 1);
        // An empty include list takes nothing, which is not the same as leaving it out.
        let (_, include, _) = parse("[project]\nname = \"a.b\"\ninclude = []\n").unwrap();
        assert!(include.is_empty());
    }

    #[test]
    fn the_name_follows_the_rule() {
        for good in ["a.b", "A1.B", "com.example", "x_y.z_w.q9", "P.Q.R"] {
            assert!(check_name(good).is_ok(), "{}", good);
        }
        for bad in [
            "", "a", "shapes", ".a", "a.", "a..b", "1a.b", "a.1b", "a b.c", "a.b/c", "a:b.c", "é.a", "_a.b", "-a.b",
            "a.my-lib",
        ] {
            assert!(check_name(bad).is_err(), "{}", bad);
        }
    }

    #[test]
    fn a_bad_name_is_reported_where_it_is_written() {
        let (line, col, message) = error("[project]\nname = \"1st.x\"\n");
        assert_eq!((line, col), (2, 8));
        assert!(message.contains("Invalid project name \"1st.x\""), "{}", message);
    }

    #[test]
    fn everything_is_strict() {
        let (line, col, message) = error("[project]\nname = \"a.b\"\nversion = \"1\"\n");
        assert_eq!((line, col), (3, 1));
        assert!(
            message.contains("unknown field `version`") && message.contains("`name`"),
            "{}",
            message
        );
        assert!(error("[project]\nname = \"a.b\"\n[extra]\n")
            .2
            .contains("unknown field `extra`"));
        assert!(error("[project]\n").2.contains("missing field `name`"));
        assert!(error("name = \"a.b\"\n").2.contains("unknown field `name`"));
        assert!(error("").2.contains("missing field `project`"));
        assert!(error("[project]\nname = 1\n").2.contains("invalid type"));
        assert!(error("[project]\nname = \"a.b\"\ninclude = \"x\"\n")
            .2
            .contains("invalid type"));
        assert_eq!(error("[project\n").0, 1);
    }

    #[test]
    fn a_pattern_that_is_not_valid_is_reported_where_it_is_written() {
        let (line, col, message) = error("[project]\nname = \"a.b\"\nexclude = [\"fine\", \"[\"]\n");
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
