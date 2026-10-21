//! The post-parse checks: names, duplicates, generic arity and references between consts.
//!
//! All the parsed files are checked together as a single namespace.

use crate::ast::{ConstDecl, EnumDecl, Field, Type, TypeParam, Value, VariantBody};
use crate::diagnostics::{Diagnostic, Pos};
use std::collections::HashMap;

/// The basic types, which are built in to the language.
pub const BASIC_TYPES: [&str; 4] = ["Nat", "Int", "Float", "String"];

fn is_basic(name: &str) -> bool {
    BASIC_TYPES.contains(&name)
}

/// The declarations of one successfully parsed file.
pub struct ParsedFile {
    pub name: String,
    pub decls: Vec<EnumDecl>,
    pub consts: Vec<ConstDecl>,
    /// True for the standard prelude, whose consts may be referred to from the definition of any const.
    pub prelude: bool,
}

/// Where a name was first declared: the index of the file and the position within it.
#[derive(Clone, Copy)]
struct Site {
    file: usize,
    pos: Pos,
}

struct TypeInfo {
    site: Site,
    arity: usize,
}

struct Param<'a> {
    name: &'a str,
    pos: Pos,
    underscore: bool,
    used: bool,
}

struct Checker<'a> {
    files: &'a [ParsedFile],
    types: HashMap<&'a str, TypeInfo>,
    /// The constructors and the consts, which share a namespace.
    constructors: HashMap<&'a str, Site>,
    /// The consts that were registered, with the index of their file. A const whose name is a duplicate is not one.
    consts: HashMap<&'a str, (usize, &'a ConstDecl)>,
    allow_refs: bool,
    /// Each diagnostic is stored with the index of its file so they can be ordered.
    found: Vec<(usize, Diagnostic)>,
}

/// Runs every post-parse check and returns the errors, each with the index of its file, ordered by file, line and
/// column. A const may only refer to another const if `allow_refs` is set, and then the references must not be circular.
pub fn validate(files: &[ParsedFile], allow_refs: bool) -> Vec<(usize, Diagnostic)> {
    let mut checker = Checker {
        files,
        types: HashMap::new(),
        constructors: HashMap::new(),
        consts: HashMap::new(),
        allow_refs,
        found: Vec::new(),
    };
    checker.collect_types();
    checker.collect_value_names();
    for (file, parsed) in files.iter().enumerate() {
        for decl in &parsed.decls {
            checker.check_decl(file, decl);
        }
    }
    checker.check_consts();
    let mut found = checker.found;
    // The sort is stable, so errors at one position keep the order in which they were found.
    found.sort_by_key(|(file, d)| (*file, d.pos));
    found
}

impl<'a> Checker<'a> {
    fn report(&mut self, file: usize, pos: Pos, message: String, first: Option<Site>) {
        let mut diagnostic = Diagnostic::new(&self.files[file].name, pos, message);
        if let Some(site) = first {
            let place = if site.file == file {
                site.pos.to_string()
            } else {
                format!("{}, {}", self.files[site.file].name, site.pos)
            };
            diagnostic = diagnostic.with_note(format!("first declared at {}", place));
        }
        self.found.push((file, diagnostic));
    }

    fn collect_types(&mut self) {
        let files = self.files;
        for (file, parsed) in files.iter().enumerate() {
            for decl in &parsed.decls {
                let name = decl.name.as_str();
                if is_basic(name) {
                    self.report(
                        file,
                        decl.pos,
                        format!("Type name {} clashes with a basic type", name),
                        None,
                    );
                } else if let Some(existing) = self.types.get(name) {
                    let first = existing.site;
                    self.report(file, decl.pos, format!("Duplicate type {}", name), Some(first));
                } else {
                    let site = Site { file, pos: decl.pos };
                    let arity = decl.params.len();
                    self.types.insert(name, TypeInfo { site, arity });
                }
            }
        }
    }

    /// Registers the constructors and the consts, which share a namespace. Within a file the names are taken in the
    /// order they were written, so that the one reported as a duplicate is the later one.
    fn collect_value_names(&mut self) {
        let files = self.files;
        for (file, parsed) in files.iter().enumerate() {
            let variants = parsed.decls.iter().flat_map(|d| &d.variants);
            let mut names: Vec<(Pos, &str, Option<&ConstDecl>)> = variants
                .map(|v| (v.pos, v.name.as_str(), None))
                .chain(parsed.consts.iter().map(|c| (c.pos, c.name.as_str(), Some(c))))
                .collect();
            names.sort_by_key(|(pos, ..)| *pos);
            for (pos, name, constant) in names {
                let (kind, label) = match constant {
                    Some(_) => ("const", "Const"),
                    None => ("constructor", "Constructor"),
                };
                if is_basic(name) {
                    let message = format!("{} name {} clashes with a basic type", label, name);
                    self.report(file, pos, message, None);
                } else if let Some(&first) = self.constructors.get(name) {
                    self.report(file, pos, format!("Duplicate {} {}", kind, name), Some(first));
                } else {
                    self.constructors.insert(name, Site { file, pos });
                    if let Some(constant) = constant {
                        self.consts.insert(name, (file, constant));
                    }
                }
            }
        }
    }

    /// Checks the type of each const, and the references that its value makes to other consts.
    fn check_consts(&mut self) {
        let files = self.files;
        // Each const with the registered consts that its value refers to, and where.
        let mut edges: HashMap<&str, Vec<(&str, Pos)>> = HashMap::new();
        for (file, parsed) in files.iter().enumerate() {
            for constant in &parsed.consts {
                match &constant.ty {
                    Some(ty) => {
                        let mut params = self.check_params(file, &constant.params);
                        self.check_type(file, ty, &mut params);
                        self.report_unused_params(file, &params);
                    }
                    // The parameters are scoped to the type, and inference finds the generic type by itself.
                    None => {
                        if let Some(first) = constant.params.first() {
                            let message = format!("Type parameters of const {} need a type", constant.name);
                            self.report(file, first.pos, message, None);
                        }
                    }
                }
                let mut refs = Vec::new();
                self.collect_refs(&constant.value, &mut refs);
                if !self.allow_refs {
                    for &(target, pos) in &refs {
                        // The consts of the prelude refer to nothing, so they can take no part in a cycle.
                        if files[self.consts[target].0].prelude {
                            continue;
                        }
                        let message = format!(
                            "Reference to const {} in const {} requires --allow-refs",
                            target, constant.name
                        );
                        self.report(file, pos, message, None);
                    }
                }
                // A const whose name was a duplicate is not the one that references to the name resolve to.
                if self
                    .consts
                    .get(constant.name.as_str())
                    .is_some_and(|(f, c)| *f == file && c.pos == constant.pos)
                {
                    edges.insert(&constant.name, refs);
                }
            }
        }
        if self.allow_refs {
            self.check_cycles(&edges);
        }
    }

    /// Finds the references to registered consts in a value.
    fn collect_refs(&self, value: &'a Value, refs: &mut Vec<(&'a str, Pos)>) {
        let mut names = Vec::new();
        value.names(&mut names);
        refs.extend(names.into_iter().filter(|(name, _)| self.consts.contains_key(name)));
    }

    /// Reports each reference that closes a circle of consts. The search is a depth-first one with an explicit stack,
    /// because a long chain of consts must not overflow the call stack.
    fn check_cycles(&mut self, edges: &HashMap<&'a str, Vec<(&'a str, Pos)>>) {
        let mut order: Vec<&str> = edges.keys().copied().collect();
        order.sort_by_key(|name| {
            let (file, constant) = self.consts[name];
            (file, constant.pos)
        });
        // A const is on the current path while its state is `OnPath` and is finished once it is `Done`.
        #[derive(PartialEq)]
        enum State {
            OnPath,
            Done,
        }
        let mut state: HashMap<&str, State> = HashMap::new();
        for start in order {
            if state.contains_key(start) {
                continue;
            }
            state.insert(start, State::OnPath);
            // Each entry is a const on the path and the index of the next reference of it to follow.
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            while let Some(&(name, next)) = stack.last() {
                let Some(&(target, pos)) = edges[name].get(next) else {
                    state.insert(name, State::Done);
                    stack.pop();
                    continue;
                };
                stack.last_mut().expect("the stack is not empty").1 += 1;
                match state.get(target) {
                    None => {
                        state.insert(target, State::OnPath);
                        stack.push((target, 0));
                    }
                    Some(State::OnPath) => {
                        let from = stack
                            .iter()
                            .position(|(n, _)| *n == target)
                            .expect("the target is on the path");
                        let mut path: Vec<&str> = stack[from..].iter().map(|(n, _)| *n).collect();
                        path.push(target);
                        let message = format!("Circular const definition {}", path.join(" -> "));
                        self.report(self.consts[name].0, pos, message, None);
                    }
                    Some(State::Done) => {}
                }
            }
        }
    }

    /// Checks the type parameters of a declaration, and returns them ready to have their uses counted.
    fn check_params(&mut self, file: usize, declared: &'a [TypeParam]) -> Vec<Param<'a>> {
        let mut params: Vec<Param<'a>> = Vec::new();
        for param in declared {
            let name = param.name.as_str();
            if params.iter().any(|p| p.name == name) {
                self.report(file, param.pos, format!("Duplicate type parameter {}", name), None);
                continue;
            }
            if is_basic(name) {
                self.report(
                    file,
                    param.pos,
                    format!("Type parameter {} clashes with a basic type", name),
                    None,
                );
            } else if self.types.contains_key(name) {
                self.report(
                    file,
                    param.pos,
                    format!("Type parameter {} clashes with a declared type", name),
                    None,
                );
            }
            params.push(Param {
                name,
                pos: param.pos,
                underscore: param.underscore,
                used: false,
            });
        }
        params
    }

    fn report_unused_params(&mut self, file: usize, params: &[Param<'a>]) {
        for param in params {
            if !param.used && !param.underscore {
                self.report(file, param.pos, format!("Unused type parameter {}", param.name), None);
            }
        }
    }

    fn check_decl(&mut self, file: usize, decl: &'a EnumDecl) {
        let mut params = self.check_params(file, &decl.params);
        for variant in &decl.variants {
            match &variant.body {
                VariantBody::Unit => {}
                VariantBody::Record(fields) => self.check_fields(file, fields, &mut params),
                VariantBody::Tuple(types) => {
                    for ty in types {
                        self.check_type(file, ty, &mut params);
                    }
                }
            }
        }
        self.report_unused_params(file, &params);
    }

    fn check_fields(&mut self, file: usize, fields: &'a [Field], params: &mut Vec<Param<'a>>) {
        let mut seen: HashMap<&str, Pos> = HashMap::new();
        for field in fields {
            if let Some(&first) = seen.get(field.name.as_str()) {
                let site = Site { file, pos: first };
                self.report(file, field.pos, format!("Duplicate field {}", field.name), Some(site));
            } else {
                seen.insert(&field.name, field.pos);
            }
            self.check_type(file, &field.ty, params);
        }
    }

    fn check_type(&mut self, file: usize, ty: &'a Type, params: &mut Vec<Param<'a>>) {
        match ty {
            Type::Array(element) => self.check_type(file, element, params),
            Type::Record(fields) => self.check_fields(file, fields, params),
            Type::Ref { name, pos, args } => {
                for arg in args {
                    self.check_type(file, arg, params);
                }
                let name = name.as_str();
                let expected = if let Some(param) = params.iter_mut().find(|p| p.name == name) {
                    param.used = true;
                    Some(0)
                } else if is_basic(name) {
                    Some(0)
                } else {
                    self.types.get(name).map(|info| info.arity)
                };
                match expected {
                    None => self.report(file, *pos, format!("Unknown type {}", name), None),
                    Some(arity) if arity != args.len() => {
                        let message = format!(
                            "Type {} takes {} type argument(s) but {} supplied",
                            name,
                            arity,
                            args.len()
                        );
                        self.report(file, *pos, message, None);
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_sources, Source};

    fn errors(texts: &[&str]) -> Vec<String> {
        let sources: Vec<Source> = texts
            .iter()
            .enumerate()
            .map(|(i, text)| Source {
                name: format!("f{}", i),
                text: text.to_string(),
            })
            .collect();
        check_sources(&sources)
            .diagnostics
            .iter()
            .map(|d| d.to_string())
            .collect()
    }

    #[test]
    fn accepts_forward_references_and_shared_names() {
        assert!(errors(&["enum Tree<T> = Leaf(T), Node(Forest<T>);\nenum Forest<T> = Trees([Tree<T>]);"]).is_empty());
        assert!(errors(&["enum Config = Config { name: String, limits: { cpu: Nat } };"]).is_empty());
        assert!(errors(&["enum P<_T> = P;"]).is_empty());
    }

    #[test]
    fn reports_duplicate_constructors_with_the_first_site() {
        assert_eq!(
            errors(&["enum E = Dev, Staging, Dev;"]),
            vec!["ERROR: Duplicate constructor Dev (f0, line 1, col 24); first declared at line 1, col 10"]
        );
    }

    #[test]
    fn treats_all_files_as_one_namespace() {
        assert!(errors(&["enum A = X(B);", "enum B = Y;"]).is_empty());
        assert_eq!(
            errors(&["enum A = X;", "enum A = Y;"]),
            vec!["ERROR: Duplicate type A (f1, line 1, col 6); first declared at f0, line 1, col 6"]
        );
        assert_eq!(errors(&["enum A = X;", "enum B = X;"]).len(), 1);
    }

    #[test]
    fn rejects_basic_type_names() {
        assert_eq!(errors(&["enum Nat = A;"]).len(), 1);
        assert_eq!(errors(&["enum E = Int;"]).len(), 1);
    }

    #[test]
    fn checks_names_and_arity() {
        assert_eq!(errors(&["enum A = X(Missing);"]).len(), 1);
        assert_eq!(errors(&["enum L<T> = Nil;\nenum W = W(L<Int, Int>);"]).len(), 2);
        assert_eq!(errors(&["enum L<T> = C(T);\nenum W = W(L);"]).len(), 1);
        assert_eq!(errors(&["enum W = W(Int<String>);"]).len(), 1);
        assert_eq!(errors(&["enum W<T> = W(T<Int>);"]).len(), 1);
    }

    #[test]
    fn checks_fields_including_nested_records() {
        assert_eq!(errors(&["enum D = R { x: Int, x: Int };"]).len(), 1);
        assert_eq!(errors(&["enum D = R { c: { a: Nat, a: Nat } };"]).len(), 1);
    }

    #[test]
    fn checks_type_parameters() {
        assert_eq!(errors(&["enum A<T, T> = X(T);"]).len(), 1);
        assert_eq!(errors(&["enum A<Int> = X(Int);"]).len(), 1);
        assert_eq!(errors(&["enum B = Y;\nenum A<B> = X(B);"]).len(), 1);
        assert_eq!(
            errors(&["enum A<T> = X;"]),
            vec!["ERROR: Unused type parameter T (f0, line 1, col 8)"]
        );
        assert_eq!(errors(&["enum A = X(T);"]).len(), 1);
    }

    #[test]
    fn reports_an_unused_parameter_at_its_own_position_after_a_duplicate() {
        assert_eq!(
            errors(&["enum A<T, T, U> = X(T);"]),
            vec![
                "ERROR: Duplicate type parameter T (f0, line 1, col 11)",
                "ERROR: Unused type parameter U (f0, line 1, col 14)",
            ]
        );
    }

    #[test]
    fn skips_validation_when_any_file_has_a_syntax_error() {
        assert_eq!(
            errors(&["enum A = X(B);", "enum B = ;"]),
            vec!["ERROR: Expected constructor name (f1, line 1, col 10); in enum B"]
        );
    }
}
