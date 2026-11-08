//! Resolution of names between projects.
//!
//! Every declaration has a canonical name, which is the local name followed by `@` and the full name of its project,
//! such as `Shape@com.example.shapes`. The files that were given without a project are the exception: the project that
//! they make has no name, and its names are bare. A reference is rewritten to the canonical name of the declaration it
//! refers to, so that after this pass a name identifies one declaration wherever it is written, and the validity checks
//! and the type-check need not know about projects at all.
//!
//! A name is resolved in the scope of the project whose file it is written in. A bare `Foo` is `local::Foo`, and the
//! group `local` holds the project itself and any project that was imported into `local`, which is the standard prelude
//! unless the project says otherwise. A qualified `group::Foo` or `full.name::Foo` is looked up in the projects of that
//! group, or in the project of that name, which must be the project itself or one that it imports. Imports are not
//! transitive, so those are the only projects that a project can name.

use crate::ast::{ConstDecl, EnumDecl, TaggedBody, Type, TypesFile, Value, ValueKind, VariantBody};
use crate::diagnostics::{Diagnostic, Pos};
use crate::parser::Parsed;
use crate::validate::BASIC_TYPES;
use crate::Source;
use std::collections::{HashMap, HashSet};

/// A project as the resolution sees it: its name, and what it imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    /// The full name, which is empty for the files that were given without a project.
    pub name: String,
    /// Each import as the group it is put into, if any, and the index of the project that it brings in.
    pub imports: Vec<(Option<String>, usize)>,
    /// True for a project that is built in.
    pub builtin: bool,
}

/// How the sources are divided into projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub projects: Vec<ProjectInfo>,
    /// The index of the project that each source belongs to, which is the project that it is written in. The values
    /// files are checked in the context of the project that `root` names. A source with no owner is taken to be in
    /// the root project, which is only a defensive check that stops the index from running off the end of the list.
    pub owners: Vec<usize>,
    pub root: usize,
}

/// The place where a declaration is written, which is the index of a project in the layout.
type Home = usize;

/// The two namespaces: types, and constructors with consts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Ns {
    Type,
    Value,
}

/// What the resolution found.
pub struct Resolution {
    /// The errors in types files, each with the index of its source.
    pub diagnostics: Vec<(usize, Diagnostic)>,
    /// The first error in each top-level value that has one, with the index of its source and of the value. These
    /// are kept apart because they are reported in turn with the type errors of the values, not before them.
    pub value_errors: Vec<(usize, usize, Diagnostic)>,
    /// For each source, the local names of the types that can be written bare in it.
    pub visible_types: Vec<HashSet<String>>,
    names: Names,
}

struct Resolver<'a> {
    layout: &'a Layout,
    homes: Vec<Home>,
    declared: HashMap<(Home, Ns), HashSet<String>>,
    /// Where each name was first declared in a home: the index of the source and the position in it.
    sites: HashMap<(Home, Ns, String), (usize, Pos)>,
}

/// What is needed to show a canonical name as it would be written in a file.
struct Names {
    layout: Layout,
    homes: Vec<Home>,
    declared: HashMap<(Home, Ns), HashSet<String>>,
}

/// The canonical name of a declaration.
fn key(layout: &Layout, home: Home, name: &str) -> String {
    match layout.projects[home].name.as_str() {
        "" => name.to_string(),
        project => format!("{}@{}", name, project),
    }
}

/// Resolves the names in the parsed sources, in place, and reports the names that cannot be resolved.
pub fn resolve(sources: &[Source], parsed: &mut [Parsed], layout: &Layout) -> Resolution {
    let homes: Vec<Home> = (0..sources.len())
        .map(|index| layout.owners.get(index).copied().unwrap_or(layout.root))
        .collect();
    let mut resolver = Resolver {
        layout,
        homes,
        declared: HashMap::new(),
        sites: HashMap::new(),
    };
    resolver.collect(parsed);
    let mut diagnostics = Vec::new();
    // This is done before the names are rewritten, as the declarations are compared by the names written.
    for (index, pos, message, (other, at)) in resolver.clashes(parsed) {
        let note = format!("first declared at {}, {}", sources[other].name, at);
        diagnostics.push((
            index,
            Diagnostic::new(&sources[index].name, pos, message).with_note(note),
        ));
    }
    let mut value_errors = Vec::new();
    let mut visible_types = vec![HashSet::new(); sources.len()];
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
                for (position, value) in values.iter_mut().enumerate() {
                    let before = context.diagnostics.len();
                    resolver.value(&mut context, value);
                    // The names are visited in the order they are written, so the first error is the leftmost.
                    if context.diagnostics.len() > before {
                        let (_, first) = context.diagnostics[before].clone();
                        context.diagnostics.truncate(before);
                        value_errors.push((index, position, first));
                    }
                }
            }
        }
    }
    let names = Names {
        layout: layout.clone(),
        homes: resolver.homes.clone(),
        declared: resolver.declared.clone(),
    };
    Resolution {
        diagnostics,
        value_errors,
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
                self.add(home, Ns::Type, &decl.name, (index, decl.pos));
                for variant in &decl.variants {
                    self.add(home, Ns::Value, &variant.name, (index, variant.pos));
                }
            }
            for constant in &file.consts {
                self.add(home, Ns::Value, &constant.name, (index, constant.pos));
            }
        }
    }

    fn add(&mut self, home: Home, ns: Ns, name: &str, site: (usize, Pos)) {
        self.declared.entry((home, ns)).or_default().insert(name.to_string());
        self.sites.entry((home, ns, name.to_string())).or_insert(site);
    }

    /// The declarations that clash with a declaration of an earlier project in one group. The projects of a group, in
    /// the order of the import with the project itself first for `local`, share one scope, so a name may be declared
    /// by only one of them. The later declaration is the error, given with its message and the place of the first.
    /// This is found for every group of every project, and each declaration is reported once, so that a clash is
    /// found when the projects are loaded and never at a use.
    fn clashes(&self, parsed: &[Parsed]) -> Vec<(usize, Pos, String, (usize, Pos))> {
        let mut found: Vec<(usize, Pos, String, (usize, Pos))> = Vec::new();
        let mut seen = HashSet::new();
        for home in 0..self.layout.projects.len() {
            let mut groups: Vec<&str> = vec!["local"];
            for (group, _) in &self.layout.projects[home].imports {
                if let Some(group) = group.as_deref() {
                    if !groups.contains(&group) {
                        groups.push(group);
                    }
                }
            }
            for group in groups {
                let mut homes = self.group_homes(home, group).unwrap_or_default();
                // The project itself comes last, so that a clash with an import is reported in the project, whose
                // files the user can change, and not in the import.
                if let Some(at) = homes.iter().position(|h| *h == home) {
                    homes.remove(at);
                    homes.push(home);
                }
                for (at, later) in homes.iter().enumerate().skip(1) {
                    let earlier = &homes[..at];
                    for (index, file) in parsed.iter().enumerate() {
                        let Parsed::Types(file) = file else { continue };
                        if self.homes[index] != *later {
                            continue;
                        }
                        let mut check = |ns: Ns, kind: &str, name: &str, pos: Pos| {
                            let first = earlier
                                .iter()
                                .find_map(|other| self.sites.get(&(*other, ns, name.to_string())).copied());
                            let message = format!("Duplicate {} {}", kind, name);
                            if let Some(site) = first {
                                if seen.insert((index, pos.line, pos.col, message.clone())) {
                                    found.push((index, pos, message, site));
                                }
                            }
                        };
                        for decl in &file.enums {
                            check(Ns::Type, "type", &decl.name, decl.pos);
                            for variant in &decl.variants {
                                check(Ns::Value, "constructor", &variant.name, variant.pos);
                            }
                        }
                        for constant in &file.consts {
                            check(Ns::Value, "const", &constant.name, constant.pos);
                        }
                    }
                }
            }
        }
        found.sort_by_key(|(index, pos, _, _)| (*index, *pos));
        found
    }

    /// The homes in the group `local` of a home: the project itself, then the projects imported into `local`.
    fn local_homes(&self, home: Home) -> Vec<Home> {
        let mut homes = vec![home];
        for (group, target) in &self.layout.projects[home].imports {
            if group.as_deref() == Some("local") && !homes.contains(target) {
                homes.push(*target);
            }
        }
        homes
    }

    /// The projects in a group of a home, or `None` if the project has no such group.
    fn group_homes(&self, home: Home, group: &str) -> Option<Vec<Home>> {
        if group == "local" {
            return Some(self.local_homes(home));
        }
        let mut homes = Vec::new();
        for (name, target) in &self.layout.projects[home].imports {
            if name.as_deref() == Some(group) && !homes.contains(target) {
                homes.push(*target);
            }
        }
        (!homes.is_empty()).then_some(homes)
    }

    /// The project that a full name refers to: the project itself, or one that it imports.
    fn full_home(&self, home: Home, full: &str) -> Option<Home> {
        if self.layout.projects[home].name == full {
            return Some(home);
        }
        self.layout.projects[home]
            .imports
            .iter()
            .find(|(_, target)| self.layout.projects[*target].name == full)
            .map(|(_, target)| *target)
    }

    fn declares(&self, home: Home, ns: Ns, name: &str) -> bool {
        self.declared.get(&(home, ns)).is_some_and(|names| names.contains(name))
    }

    fn visible_types(&self, home: Home) -> HashSet<String> {
        self.local_homes(home)
            .into_iter()
            .flat_map(|h| {
                self.declared
                    .get(&(h, Ns::Type))
                    .into_iter()
                    .flat_map(|names| names.iter().cloned())
            })
            .collect()
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
        let found: Vec<Home> = homes.into_iter().filter(|h| self.declares(*h, ns, name)).collect();
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
            // A name that two projects of a group declare is reported as a duplicate when the projects are loaded,
            // so the first is taken here and the clash is not reported again at each use.
            [first, ..] => (key(self.layout, *first, name), None),
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
    ///
    /// The names of the files in `files` are left as they are. A message can quote one, as in `first declared at
    /// FILE, line 1`, and a file name with an `@` in it must not be taken for a canonical name.
    pub fn show(&self, file: usize, text: &str, files: &[&str]) -> String {
        let mut out = String::new();
        let mut rest = text;
        loop {
            // The earliest occurrence of a file name, and the longest if several start at the same place.
            let next = files
                .iter()
                .filter(|name| name.contains('@'))
                .filter_map(|name| rest.find(name).map(|at| (at, name)))
                .min_by_key(|(at, name)| (*at, std::cmp::Reverse(name.len())));
            let Some((at, name)) = next else {
                out.push_str(&self.rewrite(file, rest));
                return out;
            };
            out.push_str(&self.rewrite(file, &rest[..at]));
            out.push_str(name);
            rest = &rest[at + name.len()..];
        }
    }

    /// Rewrites every canonical name in the text.
    fn rewrite(&self, file: usize, text: &str) -> String {
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
        let Some(home) = self.homes.get(file) else {
            return full;
        };
        let Some(target) = self.layout.projects.iter().position(|p| p.name == project) else {
            return full;
        };
        let resolver = Resolver {
            layout: &self.layout,
            homes: Vec::new(),
            declared: HashMap::new(),
            sites: HashMap::new(),
        };
        // The name is bare if, in the namespace that declares it, nothing but the target has the name in `local`.
        for ns in [Ns::Type, Ns::Value] {
            let declares = |h: Home| self.declared.get(&(h, ns)).is_some_and(|names| names.contains(local));
            if !declares(target) {
                continue;
            }
            let found: Vec<Home> = resolver
                .local_homes(*home)
                .into_iter()
                .filter(|h| declares(*h))
                .collect();
            if found == [target] {
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

    /// The group that the prelude is imported into by default.
    const LOCAL: Option<&str> = Some("local");

    /// The prelude and the projects `a` and `b`, and an `app` that imports the prelude into the group given first
    /// (or not at all), and `a` and `b` into the groups given next. The sources are the prelude, the text of `a`, the
    /// text of `b`, and then the text of `app`, followed by values in the context of `app`.
    fn with_prelude(
        prelude: Option<&str>,
        groups: (Option<&str>, Option<&str>),
        a: &str,
        b: &str,
        app: &str,
        values: Option<&str>,
    ) -> Report {
        let info = |name: &str, imports: Vec<(Option<String>, usize)>, builtin: bool| ProjectInfo {
            name: name.to_string(),
            imports,
            builtin,
        };
        let mut imports = vec![(groups.0.map(String::from), 1), (groups.1.map(String::from), 2)];
        if prelude.is_some() {
            imports.insert(0, (prelude.map(String::from), 0));
        }
        let layout_of = Layout {
            projects: vec![
                info("constructo.prelude", vec![], true),
                info("example.a", vec![(Some("local".to_string()), 0)], false),
                info("example.b", vec![(Some("local".to_string()), 0)], false),
                info("example.app", imports, false),
            ],
            owners: [0, 1, 2, 3].into_iter().chain(values.map(|_| 3)).collect(),
            root: 3,
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

    /// As `with_prelude`, with the prelude imported into `local` as it is by default.
    fn run(groups: (Option<&str>, Option<&str>), a: &str, b: &str, app: &str, values: Option<&str>) -> Report {
        with_prelude(LOCAL, groups, a, b, app, values)
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
    fn a_name_in_two_projects_of_one_group_is_a_duplicate_where_it_is_declared() {
        let expected = vec![
            "ERROR: Duplicate constructor Middle (b.types, line 1, col 10); first declared at a.types, line 1, col 10"
                .to_string(),
        ];
        // The clash is found though nothing uses the name, and a use of it is not reported again.
        for (groups, app) in [
            ((Some("local"), Some("local")), "const x: A = Middle;"),
            ((Some("local"), Some("local")), "const x: A = local::Middle;"),
            ((Some("both"), Some("both")), "enum C = Own;"),
        ] {
            let report = run(groups, "enum A = Middle;", "enum B = Middle;", app, None);
            assert_eq!(messages(&report), expected);
        }
        // Different groups are different scopes.
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
    fn a_group_with_two_projects_may_share_no_names() {
        let report = run(
            (Some("both"), Some("both")),
            "enum A = Left, Middle;",
            "enum B = Right, Middle;",
            "const x: both::A = both::Left; const y: both::B = both::Right;",
            None,
        );
        assert_eq!(messages(&report).len(), 1);
        let report = run(
            (Some("both"), Some("both")),
            "enum A = Left;",
            "enum B = Right;",
            "const x: both::A = both::Left; const y: both::B = both::Right;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
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
    fn a_project_that_declares_a_name_of_the_prelude_has_a_duplicate_where_it_is_declared() {
        // The prelude is in the scope of the project, so the declaration clashes, and the uses are not reported again.
        let report = run(
            (None, None),
            "enum A = X;",
            "enum B = Y;",
            "enum Bool = No, True;\nconst b: Bool = No;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec![
                concat!(
                    "ERROR: Duplicate type Bool (app.types, line 1, col 6); ",
                    "first declared at constructo:/prelude/prelude.types, line 1, col 6"
                ),
                concat!(
                    "ERROR: Duplicate constructor True (app.types, line 1, col 17); ",
                    "first declared at constructo:/prelude/prelude.types, line 1, col 20"
                ),
            ]
        );
    }

    #[test]
    fn a_project_that_declares_a_name_of_a_project_in_its_local_has_a_duplicate() {
        let report = run(
            (Some("local"), None),
            "enum A = Middle;",
            "enum B = Y;",
            "enum C = Middle;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Duplicate constructor Middle (app.types, line 1, col 10); first declared at a.types, line 1, col 10"]
        );
        // A project in another group is not in the scope.
        let report = run(
            (Some("a"), None),
            "enum A = Middle;",
            "enum B = Y;",
            "enum C = Middle;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_project_can_override_the_prelude_by_importing_it_into_another_group() {
        let app = "enum Bool = No, Yes;\nconst b: Bool = Yes;\nconst c: std::Bool = std::True;";
        let report = with_prelude(Some("std"), (None, None), "enum A = X;", "enum B = Y;", app, None);
        assert!(report.is_ok(), "{:?}", messages(&report));
        // The question mark is Option in the scope, so it needs an Option there.
        let report = with_prelude(
            Some("std"),
            (None, None),
            "enum A = X;",
            "enum B = Y;",
            "enum E = E(Nat?);",
            None,
        );
        assert_eq!(report.diagnostics.len(), 1);
        assert!(
            messages(&report)[0].starts_with("ERROR: Unknown type Option"),
            "{:?}",
            messages(&report)
        );
    }

    #[test]
    fn a_project_without_the_prelude_does_not_have_its_names() {
        let report = with_prelude(
            None,
            (None, None),
            "enum A = X;",
            "enum B = Y;",
            "const b: Bool = True;",
            None,
        );
        assert_eq!(report.diagnostics.len(), 1);
        assert!(
            messages(&report)[0].starts_with("ERROR: Unknown type Bool"),
            "{:?}",
            messages(&report)
        );
    }

    #[test]
    fn the_prelude_can_be_named_by_its_full_name() {
        let report = run(
            (None, None),
            "enum A = X;",
            "enum B = Y;",
            "const b: constructo.prelude::Bool = constructo.prelude::True;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
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
            let layout = Layout {
                projects: vec![],
                owners: vec![],
                root: 0,
            };
            resolve(&sources, &mut [], &layout)
        };
        // Text without a canonical name is untouched.
        assert_eq!(
            resolution.show(0, "no names here, not even a.b or x@", &[]),
            "no names here, not even a.b or x@"
        );
        // A file name with an `@` is quoted as it is, even if it has the form of a canonical name.
        assert_eq!(
            resolution.show(0, "first declared at Shape@x.y.types, line 1", &["Shape@x.y.types"]),
            "first declared at Shape@x.y.types, line 1"
        );
        assert!(report.is_ok());
    }
}
