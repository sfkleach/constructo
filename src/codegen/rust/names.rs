//! The Rust names of a model: the modules, the types, the variants, the fields and the parameters, and which parameters
//! a Rust type keeps. Every renaming is reported as a warning at the declaration.

use super::super::model::{At, Body, Enum, FullName, Model, TypeRef};
use super::super::recursion::Opaque;
use crate::diagnostics::{Diagnostic, Pos};
use crate::PRELUDE_PROJECT;
use std::collections::{HashMap, HashSet};

/// The keywords and reserved words of Rust 2021, and `gen`, which Rust 2024 reserves, so that the file compiles in
/// either. A name that is one of them is written as a raw identifier.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn", "else", "enum",
    "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "macro", "match", "mod", "move",
    "mut", "override", "priv", "pub", "ref", "return", "static", "struct", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// The names that cannot be raw identifiers, which get a `_` at the end instead.
const NOT_RAW: &[&str] = &["self", "super", "crate", "Self", "_"];

/// The names of types that the generated code adds, which a type of the declarations yields to. `codec`, the function,
/// is in Rust's other namespace, so no type can clash with it.
const FIXED: &[&str] = &["Codec", "ReadValue", "WriteValue", "DefaultValue"];

/// A name as an identifier: raw if it is a keyword, and `None` if it cannot be one at all.
fn identifier(name: &str) -> Option<String> {
    if NOT_RAW.contains(&name) {
        None
    } else if KEYWORDS.contains(&name) {
        Some(format!("r#{}", name))
    } else {
        Some(name.to_string())
    }
}

/// The name with `_` added at the end until it is not taken.
fn with_underscores(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let mut candidate = format!("{}_", name);
    while taken(&candidate) {
        candidate.push('_');
    }
    candidate
}

/// The name with `_2`, `_3` and so on after it until it is not taken, or the name itself if it is not.
fn numbered(name: &str, taken: &HashSet<String>) -> String {
    let mut candidate = name.to_string();
    let mut count = 1;
    while taken.contains(&candidate) {
        count += 1;
        candidate = format!("{}_{}", name, count);
    }
    candidate
}

/// The Rust names of the parts of a `struct`: a tuple with no names, or named fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Fields {
    Tuple,
    Named(Vec<String>),
}

/// The Rust names of a model.
pub(super) struct Names {
    /// The module of each project, which is `None` for the root, whose module is the file, and for the prelude.
    pub modules: Vec<Option<String>>,
    /// The Rust name of each enum: its `enum`, or its `struct` if it has one constructor.
    pub types: Vec<String>,
    /// For each enum of several constructors, the variant of each constructor.
    pub variants: Vec<Vec<String>>,
    /// For each enum of several constructors, the `struct` of each constructor that has a body.
    pub structs: Vec<Vec<Option<String>>>,
    /// The fields of the body of each constructor. An enum of one constructor has the fields of that constructor.
    pub fields: Vec<Vec<Fields>>,
    /// The Rust name of each parameter of each enum.
    pub params: Vec<Vec<String>>,
    /// The Rust name of each record type without a name, and the names of its fields.
    pub records: Vec<String>,
    pub record_fields: Vec<Vec<String>>,
    /// Which parameters each enum and each record keeps: a parameter that no field holds is left out of the Rust type.
    pub kept: Vec<Vec<bool>>,
    pub kept_records: Vec<Vec<bool>>,
    /// The index of each enum by its full name.
    pub index: HashMap<FullName, usize>,
    pub warnings: Vec<Diagnostic>,
    /// The constructors and enums that are adopted, whose `struct` is not generated, and so is renamed without a
    /// warning. Their names are still worked out, so that an adoption renames nothing else.
    silent: HashSet<FullName>,
}

/// Whether the enum is the prelude's `Bool` or `Option`, which are Rust's own `bool` and `Option`.
pub(super) fn is_rusts_own(name: &FullName) -> bool {
    name.project == PRELUDE_PROJECT && (name.local == "Bool" || name.local == "Option")
}

/// The types that a body holds, in order.
pub(super) fn body_types(body: &Body) -> Vec<&TypeRef> {
    match body {
        Body::None => Vec::new(),
        Body::Tuple(parameters) => parameters.iter().map(|p| &p.ty).collect(),
        Body::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
        Body::Array(element) => vec![element],
    }
}

impl Names {
    pub fn new(model: &Model, opaque: &Opaque) -> Names {
        let index: HashMap<FullName, usize> = model
            .enums
            .iter()
            .enumerate()
            .map(|(at, decl)| (decl.name.clone(), at))
            .collect();
        let (kept, kept_records) = kept_params(model, &index);
        let mut names = Names {
            modules: vec![None; model.projects.len()],
            types: model.enums.iter().map(|decl| decl.name.local.clone()).collect(),
            variants: vec![Vec::new(); model.enums.len()],
            structs: vec![Vec::new(); model.enums.len()],
            fields: vec![Vec::new(); model.enums.len()],
            params: vec![Vec::new(); model.enums.len()],
            records: Vec::new(),
            record_fields: Vec::new(),
            kept,
            kept_records,
            index,
            warnings: Vec::new(),
            silent: opaque.constructors.iter().cloned().collect(),
        };
        // An enum whose one constructor is adopted is the adopted type, and has no `struct` either.
        for decl in &model.enums {
            if let [constructor] = &decl.constructors[..] {
                if opaque.constructors.contains(&constructor.name) {
                    names.silent.insert(decl.name.clone());
                }
            }
        }
        for project in 0..model.projects.len() {
            names.name_types(model, project);
        }
        names.name_records(model);
        for (at, decl) in model.enums.iter().enumerate() {
            names.name_parts(model, at, decl);
        }
        names.name_modules(model);
        names
    }

    fn warn(&mut self, model: &Model, at: At, message: String) {
        self.warnings
            .push(Diagnostic::warning(&model.files[at.file], at.pos, message));
    }

    /// The names of the enums of a project, and of the `struct`s of their constructors. A name that is free is kept,
    /// whatever the order of the declarations: a type yields only to a name that the generated code adds, and a
    /// constructor only to an enum, by taking the name of its enum in front, and after that to a number.
    fn name_types(&mut self, model: &Model, project: usize) {
        let enums: Vec<usize> = (0..model.enums.len())
            .filter(|&at| model.enums[at].project == project)
            .collect();
        let declared: HashSet<&str> = enums.iter().map(|&at| model.enums[at].name.local.as_str()).collect();
        let yields = |name: &str| FIXED.contains(&name) || NOT_RAW.contains(&name);
        let mut taken: HashSet<String> = enums
            .iter()
            .map(|&at| model.enums[at].name.local.clone())
            .filter(|name| !yields(name))
            .collect();
        for &at in &enums {
            let decl = &model.enums[at];
            let local = decl.name.local.as_str();
            if yields(local) {
                let name = with_underscores(local, &|n| taken.contains(n) || declared.contains(n));
                taken.insert(name.clone());
                let reason = if FIXED.contains(&local) {
                    format!("since {} is a name that the generated code adds", local)
                } else {
                    format!("since {} cannot be the name of a type", local)
                };
                if !self.silent.contains(&decl.name) {
                    self.warn(
                        model,
                        decl.at,
                        format!("The enum {} is {} in Rust, {}", decl.name, name, reason),
                    );
                }
                self.types[at] = name;
            }
        }
        for &at in &enums {
            let decl = &model.enums[at];
            if decl.constructors.len() == 1 {
                continue;
            }
            let mut structs = Vec::new();
            for constructor in &decl.constructors {
                if constructor.body == Body::None {
                    structs.push(None);
                    continue;
                }
                let local = constructor.name.local.as_str();
                let (name, reason) = if declared.contains(local) {
                    let base = format!("{}{}", self.types[at], local);
                    let name = numbered(&base, &taken);
                    let reason = if name == base {
                        format!("since the enum {} has its name", local)
                    } else {
                        format!("since the enum {} has its name and {} is taken", local, base)
                    };
                    (name, Some(reason))
                } else if yields(local) || taken.contains(local) {
                    let name = with_underscores(local, &|n| taken.contains(n) || declared.contains(n));
                    let reason = format!(
                        "since {} is a name that the generated code adds or that is taken",
                        local
                    );
                    (name, Some(reason))
                } else {
                    (local.to_string(), None)
                };
                if let Some(reason) = reason.filter(|_| !self.silent.contains(&constructor.name)) {
                    let message = format!("The constructor {} is {} in Rust, {}", constructor.name, name, reason);
                    self.warn(model, constructor.at, message);
                }
                taken.insert(name.clone());
                structs.push(Some(name));
            }
            self.structs[at] = structs;
        }
    }

    /// The names of the records without a name, which are the model's, made into identifiers.
    fn name_records(&mut self, model: &Model) {
        let declared: HashSet<&str> = model.records.iter().map(|r| r.name.as_str()).collect();
        let mut taken: HashSet<String> = HashSet::new();
        for record in &model.records {
            let name = match identifier(&record.name) {
                Some(name) => name,
                None => {
                    let name = with_underscores(&record.name, &|n| taken.contains(n) || declared.contains(n));
                    let message = format!(
                        "The record type {} is {} in Rust, since {} cannot be the name of a type",
                        record.name, name, record.name
                    );
                    self.warn(model, record.at, message);
                    name
                }
            };
            taken.insert(name.clone());
            self.records.push(name);
            let fields: Vec<(Option<&str>, At)> = record.fields.iter().map(|f| (Some(f.name.as_str()), f.at)).collect();
            let what = format!("the record type {}", record.name);
            let Fields::Named(fields) = self.name_fields(model, &what, &fields) else {
                unreachable!("every field of a record has a name");
            };
            self.record_fields.push(fields);
        }
    }

    /// The names of the variants, the fields and the parameters of an enum.
    fn name_parts(&mut self, model: &Model, at: usize, decl: &Enum) {
        if decl.constructors.len() > 1 {
            let declared: HashSet<&str> = decl.constructors.iter().map(|c| c.name.local.as_str()).collect();
            let mut variants = Vec::new();
            for constructor in &decl.constructors {
                let local = constructor.name.local.as_str();
                let variant = match identifier(local) {
                    Some(name) => name,
                    None => {
                        let name = with_underscores(local, &|n| declared.contains(n));
                        let message = format!(
                            "The variant of the constructor {} is {} in Rust, since {} cannot be the name of a variant",
                            constructor.name, name, local
                        );
                        self.warn(model, constructor.at, message);
                        name
                    }
                };
                variants.push(variant);
            }
            self.variants[at] = variants;
        }
        for constructor in &decl.constructors {
            let fields: Vec<(Option<&str>, At)> = match &constructor.body {
                Body::Tuple(parameters) => parameters.iter().map(|p| (p.name.as_deref(), p.at)).collect(),
                Body::Record(fields) => fields.iter().map(|f| (Some(f.name.as_str()), f.at)).collect(),
                Body::None | Body::Array(_) => Vec::new(),
            };
            let named = matches!(&constructor.body, Body::Record(_))
                || matches!(&constructor.body, Body::Tuple(ps) if ps.iter().any(|p| p.name.is_some()));
            let names = if named {
                self.name_fields(model, &format!("{}", constructor.name), &fields)
            } else {
                Fields::Tuple
            };
            self.fields[at].push(names);
        }
        let declared: HashSet<&str> = decl.params.iter().map(|p| p.name.as_str()).collect();
        let mut params = Vec::new();
        for (p, param) in decl.params.iter().enumerate() {
            let name = match identifier(&param.name) {
                Some(name) => name,
                None => {
                    let name = with_underscores(&param.name, &|n| declared.contains(n));
                    let message = format!(
                        "The parameter {} of {} is {} in Rust, since {} cannot be the name of a type",
                        param.name, decl.name, name, param.name
                    );
                    self.warn(model, param.at, message);
                    name
                }
            };
            if !self.kept[at][p] && !param.underscore {
                let message = format!(
                    "The parameter {} of {} is left out in Rust, since no field holds it",
                    param.name, decl.name
                );
                self.warn(model, param.at, message);
            }
            params.push(name);
        }
        self.params[at] = params;
    }

    /// The names of the fields of a `struct` with named fields, where a parameter with no name is named by its
    /// position, `_0`, `_1` and so on. `what` says whose fields they are, in a warning.
    fn name_fields(&mut self, model: &Model, what: &str, fields: &[(Option<&str>, At)]) -> Fields {
        let positions: HashSet<String> = fields
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name.is_none())
            .map(|(position, _)| format!("_{}", position))
            .collect();
        let declared: HashSet<&str> = fields.iter().filter_map(|(name, _)| *name).collect();
        let mut taken: HashSet<String> = HashSet::new();
        let mut names = Vec::new();
        for (position, (name, at)) in fields.iter().enumerate() {
            let Some(name) = name else {
                names.push(format!("_{}", position));
                continue;
            };
            let clash = positions.contains(*name);
            let rust = match identifier(name) {
                Some(rust) if !clash => rust,
                _ => {
                    let rust = with_underscores(name, &|n| {
                        taken.contains(n) || declared.contains(n) || positions.contains(n)
                    });
                    let reason = if clash {
                        format!("since {} names a parameter that has no name", name)
                    } else {
                        format!("since {} cannot be the name of a field", name)
                    };
                    let message = format!("The field {} of {} is {} in Rust, {}", name, what, rust, reason);
                    self.warn(model, *at, message);
                    rust
                }
            };
            taken.insert(rust.clone());
            names.push(rust);
        }
        Fields::Named(names)
    }

    /// The names of the modules of the projects other than the root, which are the parts of the full name joined with
    /// `_`. A clash with another module, or with a type of the root, is settled by number in the order of the layout.
    fn name_modules(&mut self, model: &Model) {
        let root = model.root;
        let mut taken: HashSet<String> = FIXED.iter().map(|name| name.to_string()).collect();
        for (at, decl) in model.enums.iter().enumerate() {
            if decl.project == root {
                taken.insert(self.types[at].clone());
                taken.extend(self.structs[at].iter().flatten().cloned());
            }
        }
        for (at, record) in model.records.iter().enumerate() {
            if record.projects.contains(&root) {
                taken.insert(self.records[at].clone());
            }
        }
        for (project, info) in model.projects.iter().enumerate() {
            if project == root || !self.has_module(model, project) {
                continue;
            }
            let joined = info.name.replace('.', "_");
            let base = identifier(&joined).unwrap_or_else(|| format!("{}_", joined));
            let name = numbered(&base, &taken);
            if name != base {
                let message = format!(
                    "The project {} is the module {} in Rust, since {} is taken",
                    info.name, name, base
                );
                let at = match info.files.first() {
                    Some(&file) => At {
                        file,
                        pos: Pos { line: 1, col: 1 },
                    },
                    // A project with no types files has nowhere to point, so the warning is about the project.
                    None => {
                        self.warnings
                            .push(Diagnostic::warning(&info.name, Pos { line: 1, col: 1 }, message));
                        taken.insert(name.clone());
                        self.modules[project] = Some(name);
                        continue;
                    }
                };
                self.warn(model, at, message);
            }
            taken.insert(name.clone());
            self.modules[project] = Some(name);
        }
    }

    /// Whether the project has a module. The prelude has none unless it declares something besides `Bool` and
    /// `Option`, which are Rust's own.
    pub fn has_module(&self, model: &Model, project: usize) -> bool {
        if model.projects[project].name != PRELUDE_PROJECT {
            return true;
        }
        model
            .enums
            .iter()
            .any(|e| e.project == project && !is_rusts_own(&e.name))
            || model.records.iter().any(|r| r.projects.contains(&project))
    }
}

/// Which parameters each enum and each record keeps, as a fixed point: a parameter is kept if a field holds it,
/// directly, in an array, or as the argument of a kept parameter of another type. One that is not kept could only be
/// written with a `PhantomData`, since Rust refuses a parameter that nothing uses.
fn kept_params(model: &Model, index: &HashMap<FullName, usize>) -> (Vec<Vec<bool>>, Vec<Vec<bool>>) {
    let mut kept: Vec<Vec<bool>> = model.enums.iter().map(|e| vec![false; e.params.len()]).collect();
    let mut kept_records: Vec<Vec<bool>> = model.records.iter().map(|r| vec![false; r.params.len()]).collect();
    // Each round keeps at least one more parameter, or ends.
    loop {
        let mut changed = false;
        for (at, decl) in model.enums.iter().enumerate() {
            for (p, param) in decl.params.iter().enumerate() {
                let found = !kept[at][p]
                    && decl
                        .constructors
                        .iter()
                        .flat_map(|c| body_types(&c.body))
                        .any(|ty| occurs(ty, &param.name, &kept, &kept_records, index));
                if found {
                    kept[at][p] = true;
                    changed = true;
                }
            }
        }
        for (at, record) in model.records.iter().enumerate() {
            for (p, param) in record.params.iter().enumerate() {
                let found = !kept_records[at][p]
                    && record
                        .fields
                        .iter()
                        .any(|f| occurs(&f.ty, param, &kept, &kept_records, index));
                if found {
                    kept_records[at][p] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            return (kept, kept_records);
        }
    }
}

/// Whether the parameter occurs in the type, in a place that the Rust type keeps.
pub(super) fn occurs(
    ty: &TypeRef,
    name: &str,
    kept: &[Vec<bool>],
    kept_records: &[Vec<bool>],
    index: &HashMap<FullName, usize>,
) -> bool {
    let through = |args: &[TypeRef], keeps: Option<&Vec<bool>>| {
        args.iter().enumerate().any(|(j, arg)| {
            keeps.and_then(|keeps| keeps.get(j)).copied().unwrap_or(false)
                && occurs(arg, name, kept, kept_records, index)
        })
    };
    match ty {
        TypeRef::Param(param) => param == name,
        TypeRef::Array(element) => occurs(element, name, kept, kept_records, index),
        TypeRef::Enum(decl, args) => through(args, index.get(decl).map(|&at| &kept[at])),
        TypeRef::Record(at, args) => through(args, kept_records.get(*at)),
        TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => false,
    }
}
