//! Resolution of names between projects.
//!
//! Every declaration has a canonical name, which is the local name followed by `@` and the full name of its project,
//! such as `Shape@com.example.shapes`. The prelude, and the files that were given without a project, are the
//! exceptions: their names are bare, which is what the rest of the checker has always seen. A reference is rewritten to
//! the canonical name of the declaration it refers to, so that after this pass a name identifies one declaration
//! wherever it is written, and the validity checks and the type-check need not know about projects at all.
//!
//! A name is resolved in the scope of the project whose file it is written in. A bare `Foo` is `local::Foo`, and the
//! group `local` holds the project itself, the prelude, and any project that was imported into `local`. A qualified
//! `group::Foo` or `full.name::Foo` is looked up in the projects of that group, or in the project of that name, which
//! must be the project itself or one that it imports. Imports are not transitive, so those are the only projects that a
//! project can name.

use crate::ast::{ConstDecl, EnumDecl, Type, TypesFile, Value, ValueKind, VariantBody};
use crate::diagnostics::{Diagnostic, Pos};
use crate::parser::Parsed;
use crate::validate::BASIC_TYPES;
use crate::Source;
use std::collections::{HashMap, HashSet};

/// The project that a source belongs to, which decides where its declarations are and how its names are resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The standard prelude.
    Prelude,
    /// A project, by its index in the `Layout`.
    Project(usize),
}

/// A project as the resolution sees it: its name, and what it imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub name: String,
    /// Each import as the group it is put into, if any, and the index of the project that it brings in.
    pub imports: Vec<(Option<String>, usize)>,
}

/// How the sources are divided into projects. Without one, all of the sources are in a single project that has no name
/// and no imports, apart from the prelude.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub projects: Vec<ProjectInfo>,
    /// The owner of each source. The values files are checked in the context of the project that `root` names.
    pub owners: Vec<Owner>,
    pub root: usize,
}

/// The place where a declaration is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Home {
    Prelude,
    /// The files that were given with no project.
    Plain,
    Named(usize),
}

/// The two namespaces: types, and constructors with consts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Ns {
    Type,
    Value,
}

/// A declared name, and where it was first written.
struct Declared {
    source: usize,
    pos: Pos,
}

/// What the resolution found.
pub struct Resolution {
    /// The errors, each with the index of its source.
    pub diagnostics: Vec<(usize, Diagnostic)>,
    /// For each source, the local names of the types that can be written bare in it.
    pub visible_types: Vec<HashSet<String>>,
    names: Names,
}

pub struct Resolver<'a> {
    layout: Option<&'a Layout>,
    homes: Vec<Home>,
    declared: HashMap<(Home, Ns), HashMap<String, Declared>>,
    prelude: bool,
}

/// What is needed to show a canonical name as it would be written in a file.
struct Names {
    layout: Option<Layout>,
    homes: Vec<Home>,
    declared: HashMap<(Home, Ns), HashSet<String>>,
    prelude: bool,
}

/// The canonical name of a declaration.
fn key(layout: Option<&Layout>, home: Home, name: &str) -> String {
    match (home, layout) {
        (Home::Named(index), Some(layout)) => format!("{}@{}", name, layout.projects[index].name),
        _ => name.to_string(),
    }
}

/// Resolves the names in the parsed sources, in place, and reports the names that cannot be resolved.
pub fn resolve(sources: &[Source], parsed: &mut [Parsed], layout: Option<&Layout>) -> Resolution {
    let prelude_name = crate::PRELUDE_NAME;
    let homes: Vec<Home> = match layout {
        // A source with no owner is taken to be written in the root project. A caller should give every source an
        // owner, so this is only a defensive check that stops the index from running off the end of the list.
        Some(layout) => (0..sources.len())
            .map(|index| match layout.owners.get(index) {
                Some(Owner::Prelude) => Home::Prelude,
                Some(Owner::Project(project)) => Home::Named(*project),
                None => Home::Named(layout.root),
            })
            .collect(),
        None => sources
            .iter()
            .map(|source| {
                if source.name == prelude_name {
                    Home::Prelude
                } else {
                    Home::Plain
                }
            })
            .collect(),
    };
    let prelude = homes.contains(&Home::Prelude);
    let mut resolver = Resolver {
        layout,
        homes,
        declared: HashMap::new(),
        prelude,
    };
    resolver.collect(parsed);
    let mut diagnostics = Vec::new();
    let mut visible_types = vec![HashSet::new(); sources.len()];
    resolver.clash_with_prelude(sources, parsed, &mut diagnostics);
    for (index, file) in parsed.iter_mut().enumerate() {
        let home = resolver.homes[index];
        let mut context = Context {
            file: index,
            name: &sources[index].name,
            home,
            diagnostics: &mut diagnostics,
        };
        match file {
            Parsed::Types(file) => {
                visible_types[index] = resolver.visible_types(home);
                resolver.types_file(&mut context, file);
            }
            Parsed::Values(values) => {
                for value in values {
                    resolver.value(&mut context, value);
                }
            }
        }
    }
    let names = Names {
        layout: layout.cloned(),
        homes: resolver.homes.clone(),
        declared: resolver
            .declared
            .iter()
            .map(|(k, v)| (*k, v.keys().cloned().collect()))
            .collect(),
        prelude: resolver.prelude,
    };
    Resolution {
        diagnostics,
        visible_types,
        names,
    }
}

struct Context<'a> {
    file: usize,
    name: &'a str,
    home: Home,
    diagnostics: &'a mut Vec<(usize, Diagnostic)>,
}

impl Context<'_> {
    fn error(&mut self, pos: Pos, message: String) {
        self.diagnostics
            .push((self.file, Diagnostic::new(self.name, pos, message)));
    }
}

impl<'a> Resolver<'a> {
    /// Records the names that each home declares.
    fn collect(&mut self, parsed: &[Parsed]) {
        for (index, file) in parsed.iter().enumerate() {
            let Parsed::Types(file) = file else { continue };
            let home = self.homes[index];
            for decl in &file.enums {
                self.add(home, Ns::Type, &decl.name, index, decl.pos);
                for variant in &decl.variants {
                    self.add(home, Ns::Value, &variant.name, index, variant.pos);
                }
            }
            for constant in &file.consts {
                self.add(home, Ns::Value, &constant.name, index, constant.pos);
            }
        }
    }

    fn add(&mut self, home: Home, ns: Ns, name: &str, source: usize, pos: Pos) {
        self.declared
            .entry((home, ns))
            .or_default()
            .entry(name.to_string())
            .or_insert(Declared { source, pos });
    }

    /// A name that a project declares and the prelude does too is a duplicate, as it has always been. The prelude is
    /// to become a project that can be overridden, but until then this keeps the behaviour of a project the same as
    /// that of a list of files. Only a named project needs this, since in the other files the two share a name.
    fn clash_with_prelude(&self, sources: &[Source], parsed: &[Parsed], diagnostics: &mut Vec<(usize, Diagnostic)>) {
        if !self.prelude {
            return;
        }
        for (index, file) in parsed.iter().enumerate() {
            let Parsed::Types(file) = file else { continue };
            if !matches!(self.homes[index], Home::Named(_)) {
                continue;
            }
            let mut check = |ns: Ns, name: &str, label: &str, pos: Pos| {
                if let Some(first) = self
                    .declared
                    .get(&(Home::Prelude, ns))
                    .and_then(|names| names.get(name))
                {
                    let note = format!("first declared at {}, {}", sources[first.source].name, first.pos);
                    let message = format!("Duplicate {} {}", label, name);
                    diagnostics.push((
                        index,
                        Diagnostic::new(&sources[index].name, pos, message).with_note(note),
                    ));
                }
            };
            for decl in &file.enums {
                check(Ns::Type, &decl.name, "type", decl.pos);
                for variant in &decl.variants {
                    check(Ns::Value, &variant.name, "constructor", variant.pos);
                }
            }
            for constant in &file.consts {
                check(Ns::Value, &constant.name, "const", constant.pos);
            }
        }
    }

    /// The homes in the group `local` of a home: the project itself, then the projects imported into `local`, then the
    /// prelude.
    fn local_homes(&self, home: Home) -> Vec<Home> {
        let mut homes = vec![home];
        if let (Home::Named(index), Some(layout)) = (home, self.layout) {
            for (group, target) in &layout.projects[index].imports {
                if group.as_deref() == Some("local") && !homes.contains(&Home::Named(*target)) {
                    homes.push(Home::Named(*target));
                }
            }
        }
        if self.prelude && home != Home::Prelude {
            homes.push(Home::Prelude);
        }
        homes
    }

    /// The projects in a group of a home, or `None` if the project has no such group.
    fn group_homes(&self, home: Home, group: &str) -> Option<Vec<Home>> {
        if group == "local" {
            return Some(self.local_homes(home));
        }
        let (Home::Named(index), Some(layout)) = (home, self.layout) else {
            return None;
        };
        let mut homes = Vec::new();
        for (name, target) in &layout.projects[index].imports {
            if name.as_deref() == Some(group) && !homes.contains(&Home::Named(*target)) {
                homes.push(Home::Named(*target));
            }
        }
        (!homes.is_empty()).then_some(homes)
    }

    /// The project that a full name refers to: the project itself, or one that it imports.
    fn full_home(&self, home: Home, full: &str) -> Option<Home> {
        let (Home::Named(index), Some(layout)) = (home, self.layout) else {
            return None;
        };
        if layout.projects[index].name == full {
            return Some(home);
        }
        layout.projects[index]
            .imports
            .iter()
            .find(|(_, target)| layout.projects[*target].name == full)
            .map(|(_, target)| Home::Named(*target))
    }

    fn declares(&self, home: Home, ns: Ns, name: &str) -> bool {
        self.declared
            .get(&(home, ns))
            .is_some_and(|names| names.contains_key(name))
    }

    fn visible_types(&self, home: Home) -> HashSet<String> {
        self.local_homes(home)
            .into_iter()
            .flat_map(|h| {
                self.declared
                    .get(&(h, Ns::Type))
                    .into_iter()
                    .flat_map(|names| names.keys().cloned())
            })
            .collect()
    }

    fn candidate(&self, home: Home, name: &str) -> String {
        match (home, self.layout) {
            (Home::Named(index), Some(layout)) => format!("{}::{}", layout.projects[index].name, name),
            _ => name.to_string(),
        }
    }

    /// Resolves a name written in the scope of `home`. Returns the canonical name, and an error if there is one. A bare
    /// name that nothing declares is left for the later checks to report as unknown.
    fn name(&self, home: Home, ns: Ns, written: &str, params: &[String]) -> (String, Option<String>) {
        let (qualifier, name) = match written.rsplit_once("::") {
            Some((qualifier, name)) => (Some(qualifier), name),
            None => (None, written),
        };
        if qualifier.is_none() && ns == Ns::Type && (params.iter().any(|p| p == name) || BASIC_TYPES.contains(&name)) {
            return (written.to_string(), None);
        }
        let homes = match qualifier {
            None => self.local_homes(home),
            Some(qualifier) if qualifier.contains('.') => match self.full_home(home, qualifier) {
                Some(found) => vec![found],
                None => {
                    let message = format!(
                        "Unknown project {}: a project can name itself and the projects it imports",
                        qualifier
                    );
                    return (written.to_string(), Some(message));
                }
            },
            Some(qualifier) => match self.group_homes(home, qualifier) {
                Some(homes) => homes,
                None => return (written.to_string(), Some(format!("Unknown group {}", qualifier))),
            },
        };
        let mut found: Vec<Home> = homes.into_iter().filter(|h| self.declares(*h, ns, name)).collect();
        // A name that the project declares and the prelude declares too has been reported as a duplicate already.
        if qualifier.is_none() && found.len() == 2 && found[0] == home && found[1] == Home::Prelude {
            found.pop();
        }
        match found.as_slice() {
            [] => match qualifier {
                None => (written.to_string(), None),
                Some(qualifier) => {
                    let what = match ns {
                        Ns::Type => "type",
                        Ns::Value if name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') => "const",
                        Ns::Value => "constructor",
                    };
                    (
                        written.to_string(),
                        Some(format!("Unknown {} {} in {}", what, name, qualifier)),
                    )
                }
            },
            [one] => (key(self.layout, *one, name), None),
            many => {
                let candidates: Vec<String> = many.iter().map(|h| self.candidate(*h, name)).collect();
                let message = format!(
                    "The name {} is ambiguous: it could be {}",
                    written,
                    candidates.join(" or ")
                );
                (key(self.layout, many[0], name), Some(message))
            }
        }
    }

    fn types_file(&self, context: &mut Context, file: &mut TypesFile) {
        for decl in &mut file.enums {
            self.enum_decl(context, decl);
        }
        for constant in &mut file.consts {
            self.const_decl(context, constant);
        }
    }

    fn enum_decl(&self, context: &mut Context, decl: &mut EnumDecl) {
        let params: Vec<String> = decl.params.iter().map(|p| p.name.clone()).collect();
        decl.name = key(self.layout, context.home, &decl.name);
        for variant in &mut decl.variants {
            variant.name = key(self.layout, context.home, &variant.name);
            match &mut variant.body {
                VariantBody::Unit => {}
                VariantBody::Record(fields) => fields.iter_mut().for_each(|f| self.ty(context, &params, &mut f.ty)),
                VariantBody::Tuple(parameters) => {
                    parameters.iter_mut().for_each(|p| self.ty(context, &params, &mut p.ty))
                }
                VariantBody::Array(element) => self.ty(context, &params, element),
            }
        }
    }

    fn const_decl(&self, context: &mut Context, constant: &mut ConstDecl) {
        let params: Vec<String> = constant.params.iter().map(|p| p.name.clone()).collect();
        constant.name = key(self.layout, context.home, &constant.name);
        if let Some(ty) = &mut constant.ty {
            self.ty(context, &params, ty);
        }
        self.value(context, &mut constant.value);
    }

    fn ty(&self, context: &mut Context, params: &[String], ty: &mut Type) {
        match ty {
            Type::Ref { name, pos, args } => {
                let (resolved, error) = self.name(context.home, Ns::Type, name, params);
                if let Some(message) = error {
                    context.error(*pos, message);
                }
                *name = resolved;
                args.iter_mut().for_each(|arg| self.ty(context, params, arg));
            }
            Type::Array(element) => self.ty(context, params, element),
            Type::Record(fields) => fields.iter_mut().for_each(|f| self.ty(context, params, &mut f.ty)),
        }
    }

    fn value(&self, context: &mut Context, value: &mut Value) {
        match &mut value.kind {
            ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_) | ValueKind::Str(_) => {}
            ValueKind::Array(items) => items.iter_mut().for_each(|item| self.value(context, item)),
            ValueKind::Record(members) => members.iter_mut().for_each(|m| self.value(context, &mut m.value)),
            ValueKind::Tagged { name, body } => {
                let (resolved, error) = self.name(context.home, Ns::Value, name, &[]);
                if let Some(message) = error {
                    context.error(value.pos, message);
                }
                *name = resolved;
                use crate::ast::TaggedBody;
                match body {
                    Some(TaggedBody::Record(members)) => {
                        members.iter_mut().for_each(|m| self.value(context, &mut m.value))
                    }
                    Some(TaggedBody::Tuple(arguments)) => {
                        arguments.iter_mut().for_each(|a| self.value(context, &mut a.value))
                    }
                    Some(TaggedBody::Array(items)) => items.iter_mut().for_each(|item| self.value(context, item)),
                    None => {}
                }
            }
        }
    }
}

impl Resolution {
    /// Rewrites the canonical names in a message as they would be written in the file with the index. A name that can
    /// be written bare there is shown bare, and any other is shown in full, as `com.example.shapes::Shape`.
    pub fn show(&self, file: usize, text: &str) -> String {
        if !text.contains('@') {
            return text.to_string();
        }
        let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        while i < chars.len() {
            // A canonical name is a word, an `@`, and a full name, which is words separated by dots.
            if is_word(chars[i]) && (i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.')) {
                let mut end = i;
                while end < chars.len() && is_word(chars[end]) {
                    end += 1;
                }
                if end < chars.len() && chars[end] == '@' && chars.get(end + 1).is_some_and(|c| c.is_ascii_alphabetic())
                {
                    let mut stop = end + 1;
                    while stop < chars.len()
                        && (is_word(chars[stop])
                            || (chars[stop] == '.' && stop + 1 < chars.len() && is_word(chars[stop + 1])))
                    {
                        stop += 1;
                    }
                    let local: String = chars[i..end].iter().collect();
                    let project: String = chars[end + 1..stop].iter().collect();
                    out.push_str(&self.names.show(file, &local, &project));
                    i = stop;
                    continue;
                }
                out.extend(&chars[i..end]);
                i = end;
                continue;
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }
}

impl Names {
    /// The way to write a name in a file: bare if it resolves to the same declaration there, and in full if not.
    fn show(&self, file: usize, local: &str, project: &str) -> String {
        let full = format!("{}::{}", project, local);
        let (Some(layout), Some(home)) = (&self.layout, self.homes.get(file)) else {
            return full;
        };
        let Some(target) = layout.projects.iter().position(|p| p.name == project) else {
            return full;
        };
        let resolver = Resolver {
            layout: Some(layout),
            homes: Vec::new(),
            declared: HashMap::new(),
            prelude: self.prelude,
        };
        // The name is bare if, in the namespace that declares it, nothing but the target has the name in `local`.
        for ns in [Ns::Type, Ns::Value] {
            let declares = |h: Home| self.declared.get(&(h, ns)).is_some_and(|names| names.contains(local));
            if !declares(Home::Named(target)) {
                continue;
            }
            let found: Vec<Home> = resolver
                .local_homes(*home)
                .into_iter()
                .filter(|h| declares(*h))
                .collect();
            if found == [Home::Named(target)] {
                return local.to_string();
            }
        }
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{check_in_projects, check_sources, CheckOptions, Report};

    /// Projects `a` and `b` and an `app` that imports both in the groups that are given. The sources are the prelude,
    /// the text of `a`, the text of `b`, and then the text of `app`, followed by values in the context of `app`.
    fn run(groups: (Option<&str>, Option<&str>), a: &str, b: &str, app: &str, values: Option<&str>) -> Report {
        let info = |name: &str, imports: Vec<(Option<String>, usize)>| ProjectInfo {
            name: name.to_string(),
            imports,
        };
        let layout_of = Layout {
            projects: vec![
                info("example.a", vec![]),
                info("example.b", vec![]),
                info(
                    "example.app",
                    vec![(groups.0.map(String::from), 0), (groups.1.map(String::from), 1)],
                ),
            ],
            owners: [Owner::Prelude, Owner::Project(0), Owner::Project(1), Owner::Project(2)]
                .into_iter()
                .chain(values.map(|_| Owner::Project(2)))
                .collect(),
            root: 2,
        };
        let mut sources = vec![crate::prelude_source()];
        for (name, text) in [("a.types", a), ("b.types", b), ("app.types", app)] {
            sources.push(Source {
                name: name.to_string(),
                text: text.to_string(),
            });
        }
        if let Some(text) = values {
            sources.push(Source {
                name: "app.values".to_string(),
                text: text.to_string(),
            });
        }
        check_in_projects(&sources, &layout_of, &CheckOptions::default())
    }

    fn messages(report: &Report) -> Vec<String> {
        report.diagnostics.iter().map(|d| d.to_string()).collect()
    }

    #[test]
    fn two_projects_can_declare_the_same_names() {
        let a = "enum Shape = Circle, Middle;";
        let b = "enum Shape = Square, Middle;";
        let app = "const x: a::Shape = a::Middle; const y: b::Shape = b::Middle;";
        let report = run(
            (Some("a"), Some("b")),
            a,
            b,
            app,
            Some("a::Circle; b::Square; [a::Middle]; [b::Middle]"),
        );
        assert_eq!(messages(&report), Vec::<String>::new());
        assert!(report.type_checked);
    }

    #[test]
    fn the_types_of_two_projects_are_different_types() {
        let a = "enum Shape = Middle;";
        let b = "enum Shape = Middle;";
        let report = run((Some("a"), Some("b")), a, b, "const x: a::Shape = b::Middle;", None);
        assert_eq!(
            messages(&report),
            vec![concat!(
                "ERROR: Expected example.a::Shape but found example.b::Middle of type example.b::Shape ",
                "(app.types, line 1, col 21)"
            )]
        );
    }

    #[test]
    fn a_name_in_two_projects_of_local_is_ambiguous_where_it_is_used() {
        let report = run(
            (Some("local"), Some("local")),
            "enum A = Middle;",
            "enum B = Middle;",
            "const x: A = Middle;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec![concat!(
                "ERROR: The name Middle is ambiguous: it could be example.a::Middle or example.b::Middle ",
                "(app.types, line 1, col 14)"
            )]
        );
        // Declaring the name is not an error, and neither is using a qualified name.
        let report = run(
            (Some("local"), Some("local")),
            "enum A = Middle;",
            "enum B = Middle;",
            "const x: A = local::Middle;",
            None,
        );
        assert_eq!(report.diagnostics.len(), 1);
        let report = run(
            (Some("a"), Some("b")),
            "enum A = Middle;",
            "enum B = Middle;",
            "enum C = Own;",
            None,
        );
        assert!(report.is_ok());
    }

    #[test]
    fn a_group_with_two_projects_is_ambiguous_only_for_the_names_they_share() {
        let report = run(
            (Some("both"), Some("both")),
            "enum A = Left, Middle;",
            "enum B = Right, Middle;",
            "const x: both::A = both::Left; const y: both::B = both::Right;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_name_that_is_not_in_the_project_is_unknown() {
        let report = run(
            (Some("a"), None),
            "enum A = X;",
            "enum B = Y;",
            "const x: A = a::Y;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown constructor Y in a (app.types, line 1, col 14)"]
        );
        let report = run(
            (Some("a"), None),
            "enum A = X;",
            "enum B = Y;",
            "const x: z::A = a::X;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown group z (app.types, line 1, col 10)"]
        );
        let report = run(
            (Some("a"), None),
            "enum A = X;",
            "enum B = Y;",
            "const x: a::Nat = a::X;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown type Nat in a (app.types, line 1, col 10)"]
        );
    }

    #[test]
    fn a_project_can_name_itself_and_what_it_imports_by_full_name() {
        let app = "enum H = H(example.a::A); const x: example.app::H = example.app::H(example.a::X);";
        let report = run((None, None), "enum A = X;", "enum B = Y;", app, None);
        assert_eq!(messages(&report), Vec::<String>::new());
        let report = run(
            (None, None),
            "enum A = X;",
            "enum B = Y;",
            "const x: example.nowhere::A = X;",
            None,
        );
        assert!(messages(&report)[0].starts_with("ERROR: Unknown project example.nowhere"));
    }

    #[test]
    fn a_type_parameter_that_is_a_type_the_file_can_name_is_a_clash() {
        let report = run(
            (Some("local"), None),
            "enum T = T;",
            "enum B = Y;",
            "enum E<T> = E(T);",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Type parameter T clashes with a declared type (app.types, line 1, col 8)"]
        );
        // A type of a project that cannot be named bare is no clash.
        let report = run(
            (Some("a"), None),
            "enum T = T;",
            "enum B = Y;",
            "enum E<T> = E(T);",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_project_cannot_declare_what_the_prelude_declares() {
        let report = run((None, None), "enum A = X;", "enum B = Y;", "enum Bool = No, Yes;", None);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Duplicate type Bool (app.types, line 1, col 6); first declared at <prelude>, line 1, col 6"]
        );
    }

    #[test]
    fn the_prelude_is_in_local_for_every_project() {
        let report = run(
            (None, None),
            "enum A = X(Option<Nat>);",
            "enum B = Y;",
            "const x: Bool? = Some(true);",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn the_files_without_a_project_have_a_local_group() {
        let sources = |texts: &[&str]| -> Vec<Source> {
            texts
                .iter()
                .enumerate()
                .map(|(i, text)| Source {
                    name: format!("f{}", i),
                    text: text.to_string(),
                })
                .collect()
        };
        assert!(check_sources(&sources(&["enum A = X;", "local::X; [local::X]"])).is_ok());
        let report = check_sources(&sources(&["enum A = X;", "other::X"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown group other (f1, line 1, col 1)"]
        );
        let report = check_sources(&sources(&["enum A = X;\nconst c: A = local::Y;"]));
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown constructor Y in local (f0, line 2, col 14)"]
        );
    }

    #[test]
    fn a_name_is_shown_as_it_is_written_in_the_file() {
        let a = "enum Shape = Circle;";
        let report = run(
            (Some("a"), None),
            a,
            "enum B = Y;",
            "enum Shape = Own;\nconst x: Shape = a::Circle;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec![concat!(
                "ERROR: Expected Shape but found example.a::Circle of type example.a::Shape ",
                "(app.types, line 2, col 18)"
            )]
        );
        // A name that is bare in the file is not shown in full, and one that is not bare in it is.
        let report = run((Some("local"), None), a, "enum B = Y;", "const x: Nat = Circle;", None);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected Nat but found Circle of type Shape (app.types, line 1, col 16)"]
        );
    }

    #[test]
    fn the_names_in_a_message_are_found_by_their_shape() {
        let report = run(
            (Some("a"), None),
            "enum Shape = Circle;",
            "enum B = Y;",
            "enum H = H;",
            None,
        );
        let resolution = {
            let sources: Vec<Source> = vec![];
            resolve(&sources, &mut [], None)
        };
        // Without a layout every name is shown in full, and text without a canonical name is untouched.
        assert_eq!(
            resolution.show(0, "no names here, not even a.b or x@"),
            "no names here, not even a.b or x@"
        );
        assert!(report.is_ok());
    }
}
