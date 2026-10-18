//! The post-parse checks: names, duplicates and generic arity.
//!
//! All the parsed files are checked together as a single namespace.

use crate::ast::{EnumDecl, Field, Type, VariantBody};
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
    underscore: bool,
    used: bool,
}

struct Checker<'a> {
    files: &'a [ParsedFile],
    types: HashMap<&'a str, TypeInfo>,
    constructors: HashMap<&'a str, Site>,
    /// Each diagnostic is stored with the index of its file so they can be ordered.
    found: Vec<(usize, Diagnostic)>,
}

/// Runs every post-parse check and returns the errors ordered by file, line and column.
pub fn validate(files: &[ParsedFile]) -> Vec<Diagnostic> {
    let mut checker = Checker {
        files,
        types: HashMap::new(),
        constructors: HashMap::new(),
        found: Vec::new(),
    };
    checker.collect_types();
    checker.collect_constructors();
    for (file, parsed) in files.iter().enumerate() {
        for decl in &parsed.decls {
            checker.check_decl(file, decl);
        }
    }
    let mut found = checker.found;
    // The sort is stable, so errors at one position keep the order in which they were found.
    found.sort_by_key(|(file, d)| (*file, d.pos));
    found.into_iter().map(|(_, d)| d).collect()
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

    fn collect_constructors(&mut self) {
        let files = self.files;
        for (file, parsed) in files.iter().enumerate() {
            for variant in parsed.decls.iter().flat_map(|d| &d.variants) {
                let name = variant.name.as_str();
                if is_basic(name) {
                    let message = format!("Constructor name {} clashes with a basic type", name);
                    self.report(file, variant.pos, message, None);
                } else if let Some(&first) = self.constructors.get(name) {
                    self.report(
                        file,
                        variant.pos,
                        format!("Duplicate constructor {}", name),
                        Some(first),
                    );
                } else {
                    self.constructors.insert(name, Site { file, pos: variant.pos });
                }
            }
        }
    }

    fn check_decl(&mut self, file: usize, decl: &'a EnumDecl) {
        let mut params: Vec<Param<'a>> = Vec::new();
        for param in &decl.params {
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
                underscore: param.underscore,
                used: false,
            });
        }
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
        for (param, parsed) in params.iter().zip(&decl.params) {
            if !param.used && !param.underscore {
                self.report(file, parsed.pos, format!("Unused type parameter {}", param.name), None);
            }
        }
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
        check_sources(&sources).iter().map(|d| d.to_string()).collect()
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
    fn skips_validation_when_any_file_has_a_syntax_error() {
        assert_eq!(
            errors(&["enum A = X(B);", "enum B = ;"]),
            vec!["ERROR: Expected constructor name (f1, line 1, col 10)"]
        );
    }
}
