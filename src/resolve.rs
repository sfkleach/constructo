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
    /// The imports, in the order of the project-file.
    pub imports: Vec<ImportInfo>,
    /// True for a project that is built in.
    pub builtin: bool,
}

/// An import as the resolution sees it: the group that the project is put into, if any, and the project that it brings
/// in, with the names of that project which the import hides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportInfo {
    pub group: Option<String>,
    /// The index of the project that is brought in.
    pub target: usize,
    /// The names that are hidden, each with its place in the project-file. A hidden name is not visible through this
    /// import, though the project still has it, and it can be written with the full name of the project.
    pub hide: Vec<(String, Pos)>,
    /// The names that are renamed, each as the name that is written, the name that the project declares, and the place
    /// in the project-file. The declared name is not visible through this import, and the new one stands for it.
    pub rename: Vec<(String, String, Pos)>,
    /// The project-file that has the import, as it is shown in a message.
    pub file: String,
}

impl ImportInfo {
    /// An import that hides nothing.
    pub fn new(group: Option<&str>, target: usize) -> ImportInfo {
        ImportInfo {
            group: group.map(String::from),
            target,
            hide: Vec::new(),
            rename: Vec::new(),
            file: String::new(),
        }
    }

    fn hides(&self, name: &str) -> bool {
        self.hide.iter().any(|(hidden, _)| hidden == name)
    }

    /// The name that a declared name is written as through this import, or `None` if it is hidden.
    fn visible_name(&self, declared: &str) -> Option<String> {
        if self.hides(declared) {
            return None;
        }
        match self.rename.iter().find(|(_, old, _)| old == declared) {
            Some((new, _, _)) => Some(new.clone()),
            None => Some(declared.to_string()),
        }
    }

    /// The name that the project declares for a name written through this import, or `None` if the name is not
    /// visible: it is hidden, or it has been renamed and so is written differently.
    fn declared_name(&self, written: &str) -> Option<String> {
        if let Some((_, old, _)) = self.rename.iter().find(|(new, _, _)| new == written) {
            return Some(old.clone());
        }
        if self.hides(written) || self.rename.iter().any(|(_, old, _)| old == written) {
            return None;
        }
        Some(written.to_string())
    }
}

/// A project of a group together with what is hidden from it: the project itself, which hides nothing, or an import.
#[derive(Clone, Copy)]
struct Member<'a> {
    home: Home,
    import: Option<&'a ImportInfo>,
}

impl Member<'_> {
    /// The name that a declared name is written as in the scope, or `None` if it is hidden.
    fn visible_name(&self, declared: &str) -> Option<String> {
        match self.import {
            Some(import) => import.visible_name(declared),
            None => Some(declared.to_string()),
        }
    }

    /// The declared name for a name written in the scope, or `None` if no declaration is written that way.
    fn declared_name(&self, written: &str) -> Option<String> {
        match self.import {
            Some(import) => import.declared_name(written),
            None => Some(written.to_string()),
        }
    }
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
    /// The errors in the imports, each with the index of the project whose project-file has the import. They are in
    /// the project-files and not in the sources. They are reported before the
    /// others, and the later checks do not run if there are any.
    pub import_errors: Vec<(usize, Diagnostic)>,
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
    let import_errors = resolver.import_errors();
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
        import_errors,
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

    /// The mistakes in the hide and rename of the imports: a name that the imported project does not declare, which is
    /// most likely a typo, and a new name that the imported project declares itself.
    fn import_errors(&self) -> Vec<(usize, Diagnostic)> {
        let mut errors = Vec::new();
        for (owner, project) in self.layout.projects.iter().enumerate() {
            for import in &project.imports {
                let target = &self.layout.projects[import.target].name;
                let declared = |name: &str| {
                    self.declares(import.target, Ns::Type, name) || self.declares(import.target, Ns::Value, name)
                };
                for (name, pos) in &import.hide {
                    if !declared(name) {
                        let message = format!("Cannot hide {}: the project {} does not declare it", name, target);
                        errors.push((owner, Diagnostic::new(&import.file, *pos, message)));
                    }
                }
                for (new, old, pos) in &import.rename {
                    if !declared(old) {
                        let message = format!("Cannot rename {}: the project {} does not declare it", old, target);
                        errors.push((owner, Diagnostic::new(&import.file, *pos, message)));
                    } else if declared(new) && import.visible_name(new).as_deref() == Some(new.as_str()) {
                        let message = format!(
                            "Cannot rename {} to {}: the project {} declares {} too",
                            old, new, target, new
                        );
                        errors.push((owner, Diagnostic::new(&import.file, *pos, message)));
                    }
                }
            }
        }
        errors
    }

    fn add(&mut self, home: Home, ns: Ns, name: &str, site: (usize, Pos)) {
        self.declared.entry((home, ns)).or_default().insert(name.to_string());
        self.sites.entry((home, ns, name.to_string())).or_insert(site);
    }

    /// The declarations that clash with a declaration of an earlier project in one group. The projects of a group, in
    /// the order of the import with the project itself first for `local`, share one scope, so a name may be declared
    /// by only one of them. The later declaration is the error, given with its message and the place of the first.
    /// This is found for every group of every project, and each declaration is reported once, so that a clash is
    /// found when the projects are loaded and never at a use. A name that an import hides is not in the scope, and one
    /// that it renames is in it under the new name.
    fn clashes(&self, parsed: &[Parsed]) -> Vec<(usize, Pos, String, (usize, Pos))> {
        let mut found: Vec<(usize, Pos, String, (usize, Pos))> = Vec::new();
        let mut seen = HashSet::new();
        for home in 0..self.layout.projects.len() {
            let mut groups: Vec<&str> = vec!["local"];
            for import in &self.layout.projects[home].imports {
                if let Some(group) = import.group.as_deref() {
                    if !groups.contains(&group) {
                        groups.push(group);
                    }
                }
            }
            for group in groups {
                let mut members = self.group_members(home, group).unwrap_or_default();
                // The project itself comes last, so that a clash with an import is reported in the project, whose
                // files the user can change, and not in the import.
                if let Some(at) = members.iter().position(|m| m.home == home) {
                    let own = members.remove(at);
                    members.push(own);
                }
                for (at, later) in members.iter().enumerate().skip(1) {
                    let earlier = &members[..at];
                    for (index, file) in parsed.iter().enumerate() {
                        let Parsed::Types(file) = file else { continue };
                        if self.homes[index] != later.home {
                            continue;
                        }
                        let mut check = |ns: Ns, kind: &str, declared: &str, pos: Pos| {
                            let Some(name) = later.visible_name(declared) else {
                                return;
                            };
                            let first = earlier.iter().find_map(|other| {
                                let theirs = other.declared_name(&name)?;
                                self.sites.get(&(other.home, ns, theirs)).copied()
                            });
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

    /// The members of the group `local` of a home: the project itself, then the projects imported into `local`.
    fn local_members(&self, home: Home) -> Vec<Member<'a>> {
        let mut members = vec![Member { home, import: None }];
        for import in &self.layout.projects[home].imports {
            if import.group.as_deref() == Some("local") && !members.iter().any(|m| m.home == import.target) {
                members.push(Member {
                    home: import.target,
                    import: Some(import),
                });
            }
        }
        members
    }

    /// The members of a group of a home, or `None` if the project has no such group.
    fn group_members(&self, home: Home, group: &str) -> Option<Vec<Member<'a>>> {
        if group == "local" {
            return Some(self.local_members(home));
        }
        let mut members: Vec<Member<'a>> = Vec::new();
        for import in &self.layout.projects[home].imports {
            if import.group.as_deref() == Some(group) && !members.iter().any(|m| m.home == import.target) {
                members.push(Member {
                    home: import.target,
                    import: Some(import),
                });
            }
        }
        (!members.is_empty()).then_some(members)
    }

    /// The project that a full name refers to: the project itself, or one that it imports.
    fn full_home(&self, home: Home, full: &str) -> Option<Home> {
        if self.layout.projects[home].name == full {
            return Some(home);
        }
        self.layout.projects[home]
            .imports
            .iter()
            .find(|import| self.layout.projects[import.target].name == full)
            .map(|import| import.target)
    }

    fn declares(&self, home: Home, ns: Ns, name: &str) -> bool {
        self.declared.get(&(home, ns)).is_some_and(|names| names.contains(name))
    }

    fn visible_types(&self, home: Home) -> HashSet<String> {
        self.local_members(home)
            .into_iter()
            .flat_map(|member| {
                self.declared
                    .get(&(member.home, Ns::Type))
                    .into_iter()
                    .flat_map(|names| names.iter())
                    .filter_map(move |name| member.visible_name(name))
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
        // A name written with the full name of a project is the project's own, so nothing is hidden from it.
        let members = match qualifier {
            None => self.local_members(home),
            Some(qualifier) if qualifier.contains('.') => match self.full_home(home, qualifier) {
                Some(found) => vec![Member {
                    home: found,
                    import: None,
                }],
                None => {
                    let message = format!(
                        "Unknown project {}: a project can name itself and the projects it imports",
                        qualifier
                    );
                    return (written.to_string(), Some(message));
                }
            },
            Some(qualifier) => match self.group_members(home, qualifier) {
                Some(members) => members,
                None => return (written.to_string(), Some(format!("Unknown group {}", qualifier))),
            },
        };
        let found: Vec<(Home, String)> = members
            .into_iter()
            .filter_map(|m| m.declared_name(name).map(|declared| (m.home, declared)))
            .filter(|(h, declared)| self.declares(*h, ns, declared))
            .collect();
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
            // A name that two projects of a group declare is reported as a duplicate when the projects are loaded,
            // so the first is taken here and the clash is not reported again at each use.
            [(first, declared), ..] => (key(self.layout, *first, declared), None),
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
        // The name is bare if, in the namespace that declares it, nothing but the target has the name in `local`. It
        // is shown as the project writes it, which is the new name if the import renames it.
        let members = resolver.local_members(*home);
        let Some(own) = members.iter().find(|m| m.home == target) else {
            return full;
        };
        let Some(written) = own.visible_name(local) else {
            return full;
        };
        for ns in [Ns::Type, Ns::Value] {
            let declares = |h: Home, name: &str| self.declared.get(&(h, ns)).is_some_and(|names| names.contains(name));
            if !declares(target, local) {
                continue;
            }
            let found: Vec<Home> = members
                .iter()
                .filter(|m| {
                    m.declared_name(&written)
                        .is_some_and(|declared| declares(m.home, &declared))
                })
                .map(|m| m.home)
                .collect();
            if found == [target] {
                return written;
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
        with_hidden(prelude, groups, (&[], &[]), a, b, app, values)
    }

    /// As `with_prelude`, with the names that `app` hides from the imports of `a` and of `b`.
    fn with_hidden(
        prelude: Option<&str>,
        groups: (Option<&str>, Option<&str>),
        hide: (&[&str], &[&str]),
        a: &str,
        b: &str,
        app: &str,
        values: Option<&str>,
    ) -> Report {
        with_renamed(prelude, groups, hide, (&[], &[]), a, b, app, values)
    }

    /// The names that an import renames, each as the new name and the name that the project declares.
    type Renames<'a> = &'a [(&'a str, &'a str)];

    /// As `with_hidden`, with the names that `app` renames in the imports of `a` and of `b`.
    #[allow(clippy::too_many_arguments)]
    fn with_renamed(
        prelude: Option<&str>,
        groups: (Option<&str>, Option<&str>),
        hide: (&[&str], &[&str]),
        rename: (Renames, Renames),
        a: &str,
        b: &str,
        app: &str,
        values: Option<&str>,
    ) -> Report {
        let info = |name: &str, imports: Vec<ImportInfo>, builtin: bool| ProjectInfo {
            name: name.to_string(),
            imports,
            builtin,
        };
        let mut imports = vec![ImportInfo::new(groups.0, 1), ImportInfo::new(groups.1, 2)];
        for (import, names) in imports.iter_mut().zip([rename.0, rename.1]) {
            import.rename = names
                .iter()
                .map(|(new, old)| (new.to_string(), old.to_string(), Pos { line: 5, col: 7 }))
                .collect();
        }
        for (import, names) in imports.iter_mut().zip([hide.0, hide.1]) {
            import.hide = names
                .iter()
                .map(|name| (name.to_string(), Pos { line: 3, col: 9 }))
                .collect();
            import.file = "constructo.toml".to_string();
        }
        if prelude.is_some() {
            imports.insert(0, ImportInfo::new(prelude, 0));
        }
        let layout_of = Layout {
            projects: vec![
                info("constructo.prelude", vec![], true),
                info("example.a", vec![ImportInfo::new(Some("local"), 0)], false),
                info("example.b", vec![ImportInfo::new(Some("local"), 0)], false),
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
    fn a_hidden_name_is_not_visible_and_causes_no_clash() {
        let a = "enum A = Middle, Left;";
        let b = "enum B = Middle, Right;";
        let local = (Some("local"), Some("local"));
        // Without the hide the name is a clash.
        assert_eq!(messages(&run(local, a, b, "const x: A = Left;", None)).len(), 1);
        // Hiding it in either import removes the clash.
        for hide in [(&["Middle"][..], &[][..]), (&[][..], &["Middle"][..])] {
            let report = with_hidden(LOCAL, local, hide, a, b, "const x: A = Left;", None);
            assert!(report.is_ok(), "{:?}", messages(&report));
        }
    }

    #[test]
    fn a_hidden_name_is_unknown_bare_and_in_its_group() {
        let a = "enum A = Left, Hidden;";
        let b = "enum B = Right;";
        let local = (Some("local"), Some("other"));
        let report = with_hidden(LOCAL, local, (&["Hidden"], &[]), a, b, "const x: A = Hidden;", None);
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown constructor Hidden (app.types, line 1, col 14)"]
        );
        let report = with_hidden(
            LOCAL,
            local,
            (&["Hidden"], &[]),
            a,
            b,
            "const x: A = local::Hidden;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Unknown constructor Hidden in local (app.types, line 1, col 14)"]
        );
        // Other names of the project are still there.
        let report = with_hidden(LOCAL, local, (&["Hidden"], &[]), a, b, "const x: A = Left;", None);
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_hidden_name_can_be_written_with_the_full_name_of_the_project() {
        let a = "enum A = Left, Hidden;";
        let report = with_hidden(
            LOCAL,
            (Some("local"), Some("other")),
            (&["Hidden"], &[]),
            a,
            "enum B = Right;",
            "const x: A = example.a::Hidden;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_hide_that_names_nothing_of_the_project_is_an_error_in_the_project_file() {
        let report = with_hidden(
            LOCAL,
            (Some("local"), Some("other")),
            (&["Nope"], &[]),
            "enum A = Left;",
            "enum B = Right;",
            "const x: A = Left;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Cannot hide Nope: the project example.a does not declare it (constructo.toml, line 3, col 9)"]
        );
        assert!(!report.type_checked);
        // The project whose project-file has the import has failed, though none of its files has.
        assert_eq!(report.failed_projects, vec![3]);
    }

    #[test]
    fn a_renamed_name_is_written_with_the_new_name_only() {
        let a = "enum A = Left, Old;";
        let b = "enum B = Right;";
        let groups = (Some("local"), Some("other"));
        let renamed: (Renames, Renames) = (&[("New", "Old")], &[]);
        let run = |app: &str| with_renamed(LOCAL, groups, (&[], &[]), renamed, a, b, app, None);
        assert!(run("const x: A = New;").is_ok());
        assert!(run("const x: A = local::New;").is_ok());
        assert_eq!(
            messages(&run("const x: A = Old;")),
            vec!["ERROR: Unknown constructor Old (app.types, line 1, col 14)"]
        );
        // The full name of the project is the project's own, so it is not renamed.
        assert!(run("const x: A = example.a::Old;").is_ok());
        assert_eq!(
            messages(&run("const x: A = example.a::New;")),
            vec!["ERROR: Unknown constructor New in example.a (app.types, line 1, col 14)"]
        );
    }

    #[test]
    fn a_rename_removes_a_clash_and_can_cause_one() {
        let a = "enum A = Middle, Left;";
        let b = "enum B = Middle, Right;";
        let local = (Some("local"), Some("local"));
        let run =
            |rename: (Renames, Renames), app: &str| with_renamed(LOCAL, local, (&[], &[]), rename, a, b, app, None);
        let ok = run(
            (&[], &[("OtherMiddle", "Middle")]),
            "const x: B = OtherMiddle; const y: A = Middle;",
        );
        assert!(ok.is_ok(), "{:?}", messages(&ok));
        // Renamed onto the name of another project of the group, the clash is on the new name.
        let clash = run((&[("Right", "Left")], &[]), "const x: A = Right;");
        assert!(messages(&clash).contains(
            &concat!(
                "ERROR: Duplicate constructor Right (b.types, line 1, col 18); ",
                "first declared at a.types, line 1, col 18"
            )
            .to_string()
        ));
    }

    #[test]
    fn a_renamed_type_and_constructor_are_renamed_together() {
        let a = "enum Shape = Shape, Left;";
        let b = "enum B = Right;";
        let groups = (Some("local"), Some("other"));
        let renamed: (Renames, Renames) = (&[("Form", "Shape")], &[]);
        let report = with_renamed(
            LOCAL,
            groups,
            (&[], &[]),
            renamed,
            a,
            b,
            "const x: Form = Form; const y: Form = Left;",
            None,
        );
        assert!(report.is_ok(), "{:?}", messages(&report));
    }

    #[test]
    fn a_message_shows_the_new_name() {
        let a = "enum A = Left, Old;";
        let b = "enum B = Right;";
        let renamed: (Renames, Renames) = (&[("New", "Old")], &[]);
        let report = with_renamed(
            LOCAL,
            (Some("local"), Some("other")),
            (&[], &[]),
            renamed,
            a,
            b,
            "const x: other::B = New;",
            None,
        );
        assert_eq!(
            messages(&report),
            vec!["ERROR: Expected example.b::B but found New of type A (app.types, line 1, col 21)"]
        );
    }

    #[test]
    fn a_rename_of_nothing_or_onto_a_declared_name_is_an_error_in_the_project_file() {
        let (a, b) = ("enum A = Left, Taken;", "enum B = Right;");
        let groups = (Some("local"), Some("other"));
        let run = |renamed: Renames| {
            let report = with_renamed(
                LOCAL,
                groups,
                (&[], &[]),
                (renamed, &[]),
                a,
                b,
                "const x: A = Left;",
                None,
            );
            messages(&report)
        };
        assert_eq!(
            run(&[("New", "Nope")]),
            vec![
                "ERROR: Cannot rename Nope: the project example.a does not declare it (constructo.toml, line 5, col 7)"
            ]
        );
        assert_eq!(
            run(&[("Taken", "Left")]),
            vec![concat!(
                "ERROR: Cannot rename Left to Taken: the project example.a declares Taken too ",
                "(constructo.toml, line 5, col 7)"
            )]
        );
        // Swapping two names is fine, since neither is left in place.
        assert!(run(&[("Taken", "Left"), ("Left", "Taken")]).is_empty());
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
