//! The model of the declarations of a run: its projects, enums, record types without a name and consts, which a target
//! writes code from. It is built by `crate::build_model` once every source has been checked, so the declarations it
//! describes are valid.
//!
//! The types of the model are its own and not the checker's, since they are an interface for targets: the checker's
//! `Ty` has variables that only inference needs, and names in the canonical form `Local@project`, which is internal.

use crate::ast::{self, ConstDecl, EnumDecl, TaggedBody, Type, TypeParam, Value, ValueKind, VariantBody};
use crate::parser::Parsed;
use crate::resolve::{Layout, Resolution, Written};
use crate::typecheck::{gen_name, Scheme, Ty};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// The declarations of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// The projects, in the order of the layout, which puts every project after those it imports.
    pub projects: Vec<Project>,
    /// The index of the project that the run is for.
    pub root: usize,
    /// The enums, in the order of the files and of the declarations in them.
    pub enums: Vec<Enum>,
    /// The record types without a name, in the order in which they are first met. A record whose fields hold another
    /// record comes after that one.
    pub records: Vec<Record>,
    /// The consts, in the order of the files and of the declarations in them.
    pub consts: Vec<Const>,
}

/// A project, and what it imports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    /// The full name, which is empty for the files that were given without a project.
    pub name: String,
    /// The imports, in the order of the project-file.
    pub imports: Vec<Import>,
    /// True for a project that is built in, such as the prelude.
    pub builtin: bool,
}

/// An import of a project-file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The group that the project is put into, if any.
    pub group: Option<String>,
    /// The index of the project that is brought in.
    pub target: usize,
    /// The names that the import hides.
    pub hide: Vec<String>,
    /// The names that the import renames, each as the name that is written and the name that the project declares.
    pub rename: Vec<(String, String)>,
}

/// The name of a declaration with the full name of its project, which is empty for the files that were given without
/// a project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FullName {
    pub project: String,
    pub local: String,
}

impl FullName {
    /// The name of a declaration from its canonical name, `Local@project`, or a name with no `@` for a declaration of
    /// the files that were given without a project.
    fn from_canonical(name: &str) -> FullName {
        match name.split_once('@') {
            Some((local, project)) => FullName {
                project: project.to_string(),
                local: local.to_string(),
            },
            None => FullName {
                project: String::new(),
                local: name.to_string(),
            },
        }
    }
}

/// Shown as `project::local`, as a file writes the name in full, or as `local` alone when there is no project.
impl fmt::Display for FullName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.project.is_empty() {
            write!(f, "{}", self.local)
        } else {
            write!(f, "{}::{}", self.project, self.local)
        }
    }
}

/// A type, as a field, a parameter or a const has it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeRef {
    Nat,
    Int,
    Float,
    String,
    Array(Box<TypeRef>),
    /// An enum, with its arguments.
    Enum(FullName, Vec<TypeRef>),
    /// A record type without a name: the index of it in `Model::records`, and the arguments for its parameters.
    Record(usize, Vec<TypeRef>),
    /// A parameter of the declaration that the type is in.
    Param(String),
}

impl TypeRef {
    /// Whether a value of the type can be the absent value of a target that has one, such as `None` in Python: the
    /// prelude's `Option` can, and so can a parameter, which may be given an `Option`. A target that makes `Option<T>`
    /// its own optional type has to tell `None` from `Some(None)` where one of these is inside another.
    pub fn can_be_absent(&self) -> bool {
        match self {
            TypeRef::Enum(name, _) => name.project == crate::PRELUDE_PROJECT && name.local == "Option",
            TypeRef::Param(_) => true,
            _ => false,
        }
    }
}

/// A parameter of an enum or a const.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    /// True for a parameter written with a leading underscore, which is allowed to be unused.
    pub underscore: bool,
}

/// An enum declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    pub name: FullName,
    /// The index of the project that declares it.
    pub project: usize,
    pub params: Vec<Param>,
    /// The constructors, in the order of the declaration.
    pub constructors: Vec<Constructor>,
    /// False for an enum whose default would expand for ever, whatever its arguments: its first constructor holds,
    /// through its fields and records but not through arrays, a type that leads back to the enum. `check` refuses to
    /// leave out an argument of such a type, with `Type X has no default value`. The default of any other enum is
    /// finite for finite arguments, and it is for the target to build, within the default limit.
    pub has_default: bool,
}

/// A constructor of an enum.
#[derive(Debug, Clone, PartialEq)]
pub struct Constructor {
    pub name: FullName,
    pub body: Body,
    /// The form in which the root project writes the constructor: bare, else through a group, else in full.
    pub written: Written,
}

/// What a constructor holds.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Nothing, as in `Dot`.
    None,
    /// A tuple, as in `Point(x: Float, y: Float)`, which may be empty.
    Tuple(Vec<Parameter>),
    /// A record, as in `Circle { radius: Float }`, with its fields in the order of the declaration.
    Record(Vec<Field>),
    /// An array of one element type, as in `Polygon[Point]`.
    Array(TypeRef),
}

/// A parameter of a tuple constructor. The ones without a name come first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub name: Option<String>,
    pub ty: TypeRef,
    /// Whether the parameter holds its type by value on a cycle of types that are held by value, which a target such
    /// as Rust has to break with a pointer. Every such parameter and field is boxed, and no other.
    pub boxed: bool,
}

/// A field of a record constructor or of a record type without a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: TypeRef,
    /// As for `Parameter::boxed`.
    pub boxed: bool,
}

/// A record type without a name, such as `{ dx: Int, dy: Int }`. There is one for each distinct type: two record types
/// are the same if they have the same fields, in any order, with the same types, where the parameters of the
/// declarations that they are written in are renamed in the order in which they first occur in the fields, sorted by
/// name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The field names, sorted and joined with `_`, or `empty` for `{}`. Where two records have the same name, the
    /// first met keeps it and the others are `_2`, `_3` and so on.
    pub name: String,
    /// The parameters, `A`, `B` and so on, in the order in which they first occur in the fields.
    pub params: Vec<String>,
    /// The fields, sorted by name.
    pub fields: Vec<Field>,
    /// The indexes of the projects whose declarations use it, in order.
    pub projects: Vec<usize>,
}

/// A const declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Const {
    pub name: FullName,
    /// The index of the project that declares it.
    pub project: usize,
    /// The parameters that are declared, or for a const whose type is inferred, those that its value leaves open,
    /// named as `check` names them: `T`, `U` and so on.
    pub params: Vec<Param>,
    /// The type, as it is declared, or as `check` infers it.
    pub ty: TypeRef,
    /// The value, with every name written in full, as `com.example.shapes::Circle`. A reference to another const
    /// stays a reference.
    pub value: Value,
}

/// The name of the `index`th parameter of a record type without a name.
fn record_param(index: usize) -> String {
    match "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().nth(index) {
        Some(letter) => letter.to_string(),
        None => format!("A{}", index),
    }
}

/// Builds the model from the sources of a run that has been checked with no errors: their syntax trees, with every
/// name resolved, the layout, the resolution, and the inferred type of each const that does not declare one, by its
/// canonical name.
pub(crate) fn build(
    parsed: &[Parsed],
    layout: &Layout,
    resolution: &Resolution,
    inferred: &HashMap<String, Scheme>,
) -> Model {
    let mut builder = Builder::default();
    let projects = layout
        .projects
        .iter()
        .map(|project| Project {
            name: project.name.clone(),
            imports: project
                .imports
                .iter()
                .map(|import| Import {
                    group: import.group.clone(),
                    target: import.target,
                    hide: import.hide.iter().map(|(name, _)| name.clone()).collect(),
                    rename: import
                        .rename
                        .iter()
                        .map(|(new, old, _)| (new.clone(), old.clone()))
                        .collect(),
                })
                .collect(),
            builtin: project.builtin,
        })
        .collect();
    let types_files: Vec<(usize, &ast::TypesFile)> = parsed
        .iter()
        .enumerate()
        .filter_map(|(index, parsed)| match parsed {
            Parsed::Types(file) => Some((layout.owners.get(index).copied().unwrap_or(layout.root), file)),
            Parsed::Values(_) => None,
        })
        .collect();
    let mut enums = Vec::new();
    for &(project, file) in &types_files {
        for decl in &file.enums {
            enums.push(builder.enum_decl(decl, project, layout.root, resolution));
        }
    }
    let mut consts = Vec::new();
    for &(project, file) in &types_files {
        for constant in &file.consts {
            consts.push(builder.const_decl(constant, project, inferred));
        }
    }
    let mut model = Model {
        projects,
        root: layout.root,
        enums,
        records: builder.records,
        consts,
    };
    super::recursion::work_out(&mut model);
    model
}

/// What is built up while the declarations are met: the record types without a name, and how to find each by its
/// fields.
#[derive(Default)]
struct Builder {
    records: Vec<Record>,
    /// Each record by its fields, sorted, with the parameters renamed as in `Record::params`.
    identities: HashMap<Vec<(String, TypeRef)>, usize>,
    /// The names that the records have been given.
    names: HashSet<String>,
}

impl Builder {
    fn enum_decl(&mut self, decl: &EnumDecl, project: usize, root: usize, resolution: &Resolution) -> Enum {
        let params: Vec<String> = decl.params.iter().map(|p| p.name.clone()).collect();
        let constructors = decl
            .variants
            .iter()
            .map(|variant| {
                let body = match &variant.body {
                    VariantBody::Unit => Body::None,
                    VariantBody::Tuple(parameters) => Body::Tuple(
                        parameters
                            .iter()
                            .map(|parameter| Parameter {
                                name: parameter.name.clone(),
                                ty: self.declared(&parameter.ty, &params, project),
                                boxed: false,
                            })
                            .collect(),
                    ),
                    VariantBody::Record(fields) => Body::Record(
                        fields
                            .iter()
                            .map(|field| Field {
                                name: field.name.clone(),
                                ty: self.declared(&field.ty, &params, project),
                                boxed: false,
                            })
                            .collect(),
                    ),
                    VariantBody::Array(element) => Body::Array(self.declared(element, &params, project)),
                };
                let name = FullName::from_canonical(&variant.name);
                let written = resolution.written_in_project(root, &name.local, &name.project);
                Constructor { name, body, written }
            })
            .collect();
        Enum {
            name: FullName::from_canonical(&decl.name),
            project,
            params: model_params(&decl.params),
            constructors,
            has_default: true,
        }
    }

    fn const_decl(&mut self, constant: &ConstDecl, project: usize, inferred: &HashMap<String, Scheme>) -> Const {
        let (params, ty) = match &constant.ty {
            Some(ty) => {
                let names: Vec<String> = constant.params.iter().map(|p| p.name.clone()).collect();
                (model_params(&constant.params), self.declared(ty, &names, project))
            }
            None => {
                // A const of a run with no errors has its type, declared or inferred, since `check_consts` has
                // inferred every one that it could, and a const that it could not infer is an error.
                let scheme = inferred
                    .get(&constant.name)
                    .expect("the type of a const of a run with no errors has been inferred");
                let params = (0..scheme.arity)
                    .map(|index| Param {
                        name: gen_name(index),
                        underscore: false,
                    })
                    .collect();
                (params, self.inferred(&scheme.ty, project))
            }
        };
        let mut value = constant.value.clone();
        write_in_full(&mut value);
        Const {
            name: FullName::from_canonical(&constant.name),
            project,
            params,
            ty,
            value,
        }
    }

    /// A type as it is written in a declaration of the project, whose parameters are `params`.
    fn declared(&mut self, ty: &Type, params: &[String], project: usize) -> TypeRef {
        match ty {
            // A parameter is found before a type of the same name, as `typecheck::instantiate` finds it.
            Type::Ref { name, .. } if params.contains(name) => TypeRef::Param(name.clone()),
            Type::Ref { name, args, .. } => match name.as_str() {
                "Nat" => TypeRef::Nat,
                "Int" => TypeRef::Int,
                "Float" => TypeRef::Float,
                "String" => TypeRef::String,
                _ => TypeRef::Enum(
                    FullName::from_canonical(name),
                    args.iter().map(|arg| self.declared(arg, params, project)).collect(),
                ),
            },
            Type::Array(element) => TypeRef::Array(Box::new(self.declared(element, params, project))),
            Type::Record(fields) => {
                let fields = fields
                    .iter()
                    .map(|field| (field.name.clone(), self.declared(&field.ty, params, project)))
                    .collect();
                self.record(fields, project)
            }
        }
    }

    /// A type that the checker inferred for a const of the project, in which an open type is a parameter.
    fn inferred(&mut self, ty: &Ty, project: usize) -> TypeRef {
        match ty {
            Ty::Nat => TypeRef::Nat,
            Ty::Int => TypeRef::Int,
            Ty::Float => TypeRef::Float,
            Ty::Str => TypeRef::String,
            Ty::Array(element) => TypeRef::Array(Box::new(self.inferred(element, project))),
            Ty::Enum(name, args) => TypeRef::Enum(
                FullName::from_canonical(name),
                args.iter().map(|arg| self.inferred(arg, project)).collect(),
            ),
            Ty::Record(fields) => {
                let fields = fields
                    .iter()
                    .map(|(name, field)| (name.clone(), self.inferred(field, project)))
                    .collect();
                self.record(fields, project)
            }
            Ty::Param(name) => TypeRef::Param(name.clone()),
            Ty::Gen(index) => TypeRef::Param(gen_name(*index)),
            // A scheme has had its variables made into open types, `Ty::Gen`, so none is left.
            Ty::Var(_) => unreachable!("an inferred type has no variables"),
        }
    }

    /// The record type without a name that has the fields, which the project uses. It is made the first time that it is
    /// met.
    fn record(&mut self, mut fields: Vec<(String, TypeRef)>, project: usize) -> TypeRef {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let mut order = Vec::new();
        for (_, ty) in &fields {
            params_of(ty, &mut order);
        }
        let identity: Vec<(String, TypeRef)> = fields
            .into_iter()
            .map(|(name, ty)| (name, renamed(&ty, &order)))
            .collect();
        let index = match self.identities.get(&identity) {
            Some(&index) => index,
            None => {
                let name = self.record_name(&identity);
                self.records.push(Record {
                    name,
                    params: (0..order.len()).map(record_param).collect(),
                    fields: identity
                        .iter()
                        .map(|(name, ty)| Field {
                            name: name.clone(),
                            ty: ty.clone(),
                            boxed: false,
                        })
                        .collect(),
                    projects: Vec::new(),
                });
                self.identities.insert(identity, self.records.len() - 1);
                self.records.len() - 1
            }
        };
        let projects = &mut self.records[index].projects;
        if let Err(at) = projects.binary_search(&project) {
            projects.insert(at, project);
        }
        TypeRef::Record(index, order.into_iter().map(TypeRef::Param).collect())
    }

    /// The name of a new record with the fields: the names of the fields, or `empty`, with `_2`, `_3` and so on if a
    /// record that was met before has that name.
    fn record_name(&mut self, fields: &[(String, TypeRef)]) -> String {
        let base = if fields.is_empty() {
            "empty".to_string()
        } else {
            fields
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join("_")
        };
        let mut name = base.clone();
        let mut count = 1;
        while self.names.contains(&name) {
            count += 1;
            name = format!("{}_{}", base, count);
        }
        self.names.insert(name.clone());
        name
    }
}

fn model_params(params: &[TypeParam]) -> Vec<Param> {
    params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            underscore: p.underscore,
        })
        .collect()
}

/// Adds the parameters that occur in the type to `order`, in the order in which they first occur.
fn params_of(ty: &TypeRef, order: &mut Vec<String>) {
    match ty {
        TypeRef::Param(name) => {
            if !order.contains(name) {
                order.push(name.clone());
            }
        }
        TypeRef::Array(element) => params_of(element, order),
        TypeRef::Enum(_, args) | TypeRef::Record(_, args) => args.iter().for_each(|arg| params_of(arg, order)),
        TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => {}
    }
}

/// The type with each parameter renamed by its place in `order`, as the parameters of a record are named.
fn renamed(ty: &TypeRef, order: &[String]) -> TypeRef {
    match ty {
        TypeRef::Param(name) => {
            let index = order
                .iter()
                .position(|p| p == name)
                .expect("every parameter of the type is in the order");
            TypeRef::Param(record_param(index))
        }
        TypeRef::Array(element) => TypeRef::Array(Box::new(renamed(element, order))),
        TypeRef::Enum(name, args) => TypeRef::Enum(name.clone(), args.iter().map(|a| renamed(a, order)).collect()),
        TypeRef::Record(index, args) => TypeRef::Record(*index, args.iter().map(|a| renamed(a, order)).collect()),
        other => other.clone(),
    }
}

/// Rewrites the canonical names in a value in full, as `com.example.shapes::Circle`.
fn write_in_full(value: &mut Value) {
    match &mut value.kind {
        ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_) | ValueKind::Str(_) => {}
        ValueKind::Array(items) => items.iter_mut().for_each(write_in_full),
        ValueKind::Record(members) => members.iter_mut().for_each(|m| write_in_full(&mut m.value)),
        ValueKind::Tagged { name, body } => {
            *name = FullName::from_canonical(name).to_string();
            match body {
                Some(TaggedBody::Record(members)) => members.iter_mut().for_each(|m| write_in_full(&mut m.value)),
                Some(TaggedBody::Tuple(arguments)) => arguments.iter_mut().for_each(|a| write_in_full(&mut a.value)),
                Some(TaggedBody::Array(items)) => items.iter_mut().for_each(write_in_full),
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::{ImportInfo, ProjectInfo};
    use crate::{build_model, plain_layout, prelude_source, CheckOptions, ProjectSet, Source};
    use std::path::Path;

    /// A project for `model_of`: its full name, its imports other than that of the prelude, and the texts of its
    /// types files.
    struct P<'a> {
        name: &'a str,
        imports: Vec<ImportInfo>,
        texts: &'a [&'a str],
    }

    fn project<'a>(name: &'a str, imports: Vec<ImportInfo>, texts: &'a [&'a str]) -> P<'a> {
        P { name, imports, texts }
    }

    /// An import of the project with the index into the group, which hides and renames nothing.
    fn import(group: &str, target: usize) -> ImportInfo {
        ImportInfo::new(Some(group), target)
    }

    /// The model of the prelude, which is project 0 and imported into `local` by every other, and then the projects,
    /// numbered from 1, the last of which is the root, which has the values. The errors are given as messages.
    fn model_of(projects: &[P], values: Option<&str>) -> Result<Model, Vec<String>> {
        let mut infos = vec![ProjectInfo {
            name: crate::PRELUDE_PROJECT.to_string(),
            imports: Vec::new(),
            builtin: true,
        }];
        let mut sources = vec![prelude_source()];
        let mut owners = vec![0];
        for (index, p) in projects.iter().enumerate() {
            let mut imports = vec![import("local", 0)];
            imports.extend(p.imports.iter().cloned());
            infos.push(ProjectInfo {
                name: p.name.to_string(),
                imports,
                builtin: false,
            });
            for (number, text) in p.texts.iter().enumerate() {
                sources.push(Source {
                    name: format!("{}-{}.types", p.name, number),
                    text: text.to_string(),
                });
                owners.push(index + 1);
            }
        }
        let root = projects.len();
        if let Some(text) = values {
            sources.push(Source {
                name: "root.values".to_string(),
                text: text.to_string(),
            });
            owners.push(root);
        }
        let layout = Layout {
            projects: infos,
            owners,
            root,
        };
        build_model(&sources, &layout, &CheckOptions::default())
            .map_err(|found| found.iter().map(|d| d.to_string()).collect())
    }

    /// The model of one project, `example.app`, with the types files.
    fn model_of_app(texts: &[&str]) -> Model {
        model_of(&[project("example.app", vec![], texts)], None).unwrap()
    }

    fn name(project: &str, local: &str) -> FullName {
        FullName {
            project: project.to_string(),
            local: local.to_string(),
        }
    }

    fn find_enum<'m>(model: &'m Model, local: &str) -> &'m Enum {
        model
            .enums
            .iter()
            .find(|e| e.name.local == local)
            .unwrap_or_else(|| panic!("no enum {}", local))
    }

    fn find_const<'m>(model: &'m Model, local: &str) -> &'m Const {
        model
            .consts
            .iter()
            .find(|c| c.name.local == local)
            .unwrap_or_else(|| panic!("no const {}", local))
    }

    fn constructor<'m>(model: &'m Model, local: &str) -> &'m Constructor {
        model
            .enums
            .iter()
            .flat_map(|e| &e.constructors)
            .find(|c| c.name.local == local)
            .unwrap_or_else(|| panic!("no constructor {}", local))
    }

    /// The types of the fields of the record constructor.
    fn field_types(model: &Model, local: &str) -> Vec<TypeRef> {
        match &constructor(model, local).body {
            Body::Record(fields) => fields.iter().map(|f| f.ty.clone()).collect(),
            other => panic!("{} has the body {:?}", local, other),
        }
    }

    /// The value as Constructo text, compact.
    fn text(value: &Value, compact: bool) -> String {
        crate::values_text::render(value, compact)
            .trim_end_matches(";\n")
            .to_string()
    }

    fn param(name: &str) -> TypeRef {
        TypeRef::Param(name.to_string())
    }

    const IDS: &str = "enum Uuid = Uuid(String);";

    const SHAPES: &str = "
        enum Shape =
          Circle { radius: Float },
          Box { width: Float, height: Float },
          Dot,
          Polygon[Point];
        enum Point = Point(x: Float, y: Float);
        enum Colour = Red, Green, Blue;
        enum Styled<T> = Styled { item: T, colour: Colour?, offset: { dx: Int, dy: Int } };
        enum Drawing = Drawing(id: ids::Uuid, title: String, shapes: [Styled<Shape>], layer: Nat);
        const unit_circle = Circle { radius: 1.0 };
    ";

    /// The projects of the scenario of the codegen task, `com.example.ids` and `com.example.shapes`, which imports the
    /// first into the group `ids`.
    fn scenario() -> Model {
        model_of(
            &[
                project("com.example.ids", vec![], &[IDS]),
                project("com.example.shapes", vec![import("ids", 1)], &[SHAPES]),
            ],
            Some("Drawing(id: ids::Uuid(\"x\"), title: \"t\", shapes: [], layer: 0);"),
        )
        .unwrap()
    }

    #[test]
    fn the_scenario_has_its_projects_in_the_order_of_the_layout() {
        let model = scenario();
        let names: Vec<&str> = model.projects.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["constructo.prelude", "com.example.ids", "com.example.shapes"]);
        assert_eq!(model.root, 2);
        assert!(model.projects[0].builtin);
        assert!(!model.projects[2].builtin);
        assert_eq!(
            model.projects[2].imports,
            [
                Import {
                    group: Some("local".to_string()),
                    target: 0,
                    hide: vec![],
                    rename: vec![],
                },
                Import {
                    group: Some("ids".to_string()),
                    target: 1,
                    hide: vec![],
                    rename: vec![],
                },
            ]
        );
    }

    #[test]
    fn the_scenario_has_its_enums_in_the_order_of_the_files() {
        let model = scenario();
        let names: Vec<String> = model.enums.iter().map(|e| e.name.to_string()).collect();
        assert_eq!(
            names,
            [
                "constructo.prelude::Bool",
                "constructo.prelude::Option",
                "com.example.ids::Uuid",
                "com.example.shapes::Shape",
                "com.example.shapes::Point",
                "com.example.shapes::Colour",
                "com.example.shapes::Styled",
                "com.example.shapes::Drawing",
            ]
        );
        assert_eq!(find_enum(&model, "Uuid").project, 1);
        assert_eq!(find_enum(&model, "Shape").project, 2);
        let shape: Vec<&str> = find_enum(&model, "Shape")
            .constructors
            .iter()
            .map(|c| c.name.local.as_str())
            .collect();
        assert_eq!(shape, ["Circle", "Box", "Dot", "Polygon"]);
    }

    #[test]
    fn the_root_writes_a_constructor_of_an_import_through_its_group() {
        let model = scenario();
        assert_eq!(
            constructor(&model, "Uuid").written,
            Written::Group("ids".into(), "Uuid".into())
        );
        assert_eq!(constructor(&model, "Circle").written, Written::Bare("Circle".into()));
        assert_eq!(constructor(&model, "None").written, Written::Bare("None".into()));
        // In a run whose root is the project that declares it, it is bare.
        let ids = model_of(&[project("com.example.ids", vec![], &[IDS])], None).unwrap();
        assert_eq!(constructor(&ids, "Uuid").written, Written::Bare("Uuid".into()));
    }

    #[test]
    fn a_body_has_its_types() {
        let model = scenario();
        assert_eq!(constructor(&model, "Dot").body, Body::None);
        assert_eq!(
            constructor(&model, "Polygon").body,
            Body::Array(TypeRef::Enum(name("com.example.shapes", "Point"), vec![]))
        );
        assert_eq!(
            constructor(&model, "Point").body,
            Body::Tuple(vec![
                Parameter {
                    name: Some("x".into()),
                    ty: TypeRef::Float,
                    boxed: false,
                },
                Parameter {
                    name: Some("y".into()),
                    ty: TypeRef::Float,
                    boxed: false,
                },
            ])
        );
        let Body::Tuple(drawing) = &constructor(&model, "Drawing").body else {
            panic!("Drawing has a tuple");
        };
        let types: Vec<&TypeRef> = drawing.iter().map(|p| &p.ty).collect();
        let styled_shape = TypeRef::Enum(
            name("com.example.shapes", "Styled"),
            vec![TypeRef::Enum(name("com.example.shapes", "Shape"), vec![])],
        );
        assert_eq!(
            types,
            [
                &TypeRef::Enum(name("com.example.ids", "Uuid"), vec![]),
                &TypeRef::String,
                &TypeRef::Array(Box::new(styled_shape)),
                &TypeRef::Nat,
            ]
        );
    }

    #[test]
    fn a_generic_enum_has_its_parameters_and_an_option_and_a_record() {
        let model = scenario();
        let styled = find_enum(&model, "Styled");
        assert_eq!(
            styled.params,
            [Param {
                name: "T".into(),
                underscore: false,
            }]
        );
        let colour = TypeRef::Enum(name("com.example.shapes", "Colour"), vec![]);
        assert_eq!(
            field_types(&model, "Styled"),
            [
                param("T"),
                TypeRef::Enum(name("constructo.prelude", "Option"), vec![colour]),
                TypeRef::Record(0, vec![]),
            ]
        );
        assert_eq!(
            model.records,
            [Record {
                name: "dx_dy".into(),
                params: vec![],
                fields: vec![
                    Field {
                        name: "dx".into(),
                        ty: TypeRef::Int,
                        boxed: false,
                    },
                    Field {
                        name: "dy".into(),
                        ty: TypeRef::Int,
                        boxed: false,
                    },
                ],
                projects: vec![2],
            }]
        );
    }

    #[test]
    fn a_const_has_its_inferred_type_and_its_value_with_names_in_full() {
        let model = scenario();
        let circle = find_const(&model, "unit_circle");
        assert_eq!(circle.name, name("com.example.shapes", "unit_circle"));
        assert_eq!(circle.project, 2);
        assert!(circle.params.is_empty());
        assert_eq!(circle.ty, TypeRef::Enum(name("com.example.shapes", "Shape"), vec![]));
        assert_eq!(text(&circle.value, true), "com.example.shapes::Circle { radius: 1.0 }");
    }

    #[test]
    fn a_generic_const_has_its_parameters() {
        let model = scenario();
        let null = find_const(&model, "null");
        assert_eq!(
            null.params,
            [Param {
                name: "T".into(),
                underscore: false,
            }]
        );
        assert_eq!(
            null.ty,
            TypeRef::Enum(name("constructo.prelude", "Option"), vec![param("T")])
        );
        assert_eq!(text(&null.value, true), "constructo.prelude::None");
    }

    #[test]
    fn an_inferred_const_that_leaves_a_type_open_has_a_parameter_for_it() {
        let model = model_of_app(&["const nothing = None; const pair = { a: None, b: [] };"]);
        let nothing = find_const(&model, "nothing");
        assert_eq!(nothing.params.len(), 1);
        assert_eq!(
            nothing.ty,
            TypeRef::Enum(name("constructo.prelude", "Option"), vec![param("T")])
        );
        let pair = find_const(&model, "pair");
        let names: Vec<&str> = pair.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["T", "U"]);
        assert_eq!(pair.ty, TypeRef::Record(0, vec![param("T"), param("U")]));
        let record = &model.records[0];
        assert_eq!(record.name, "a_b");
        assert_eq!(record.params, ["A", "B"]);
        assert_eq!(
            record.fields[0].ty,
            TypeRef::Enum(name("constructo.prelude", "Option"), vec![param("A")])
        );
        assert_eq!(record.fields[1].ty, TypeRef::Array(Box::new(param("B"))));
    }

    #[test]
    fn a_const_that_refers_to_another_keeps_the_reference() {
        let model = model_of(
            &[project(
                "example.app",
                vec![],
                &["const one: Nat = 1; const two: [Nat] = [one, 2];"],
            )],
            None,
        );
        // A reference to a const is only allowed with `--allow-refs`.
        assert!(model.is_err());
        let layout_and_sources = |text: &str| {
            let sources = vec![
                prelude_source(),
                Source {
                    name: "app.types".into(),
                    text: text.into(),
                },
            ];
            let layout = plain_layout(&sources);
            (sources, layout)
        };
        let (sources, layout) = layout_and_sources("const one: Nat = 1; const two: [Nat] = [one, 2];");
        let options = CheckOptions {
            allow_refs: true,
            ..CheckOptions::default()
        };
        let model = build_model(&sources, &layout, &options).unwrap();
        let two = find_const(&model, "two");
        assert_eq!(two.name, name("", "two"));
        assert_eq!(two.ty, TypeRef::Array(Box::new(TypeRef::Nat)));
        assert_eq!(text(&two.value, true), "[one, 2]");
    }

    #[test]
    fn a_const_with_a_declared_type_has_it() {
        let model = model_of_app(&["enum Pair<A, _B> = Pair(A?); const p<T>: Pair<T, Nat> = Pair(null);"]);
        let pair = find_enum(&model, "Pair");
        assert!(!pair.params[0].underscore);
        assert!(pair.params[1].underscore);
        let p = find_const(&model, "p");
        assert_eq!(p.params[0].name, "T");
        assert_eq!(
            p.ty,
            TypeRef::Enum(name("example.app", "Pair"), vec![param("T"), TypeRef::Nat])
        );
        assert_eq!(text(&p.value, true), "example.app::Pair(constructo.prelude::null)");
    }

    #[test]
    fn a_name_is_written_through_a_group_with_its_rename_and_not_where_it_is_hidden() {
        let lib = "enum Shape = Circle, Square;";
        let renamed = ImportInfo {
            rename: vec![(
                "Round".into(),
                "Circle".into(),
                crate::diagnostics::Pos { line: 1, col: 1 },
            )],
            ..import("g", 1)
        };
        let model = model_of(
            &[
                project("example.lib", vec![], &[lib]),
                project("example.app", vec![renamed], &["enum Scene = Scene;"]),
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            constructor(&model, "Circle").written,
            Written::Group("g".into(), "Round".into())
        );
        assert_eq!(
            constructor(&model, "Square").written,
            Written::Group("g".into(), "Square".into())
        );
        // The project is in two groups, and the first hides the name, so it is written through the second.
        let hidden = ImportInfo {
            hide: vec![("Circle".into(), crate::diagnostics::Pos { line: 1, col: 1 })],
            ..import("g", 1)
        };
        let model = model_of(
            &[
                project("example.lib", vec![], &[lib]),
                project("example.app", vec![hidden, import("h", 1)], &["enum Scene = Scene;"]),
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            constructor(&model, "Circle").written,
            Written::Group("h".into(), "Circle".into())
        );
        assert_eq!(
            constructor(&model, "Square").written,
            Written::Group("g".into(), "Square".into())
        );
        // A project that the root does not import is written in full.
        let model = model_of(
            &[
                project("example.lib", vec![], &[lib]),
                project("example.mid", vec![import("lib", 1)], &["enum Mid = Mid(lib::Shape);"]),
                project(
                    "example.app",
                    vec![import("mid", 2)],
                    &["enum Scene = Scene(mid::Mid);"],
                ),
            ],
            None,
        )
        .unwrap();
        assert_eq!(constructor(&model, "Circle").written, Written::Full);
        assert_eq!(
            constructor(&model, "Mid").written,
            Written::Group("mid".into(), "Mid".into())
        );
    }

    #[test]
    fn the_same_record_in_two_projects_is_one_record() {
        let model = model_of(
            &[
                project("example.lib", vec![], &["enum A = A { at: { x: Nat, y: Nat } };"]),
                project(
                    "example.app",
                    vec![import("lib", 1)],
                    &["enum B = B(pos: { y: Nat, x: Nat });"],
                ),
            ],
            None,
        )
        .unwrap();
        assert_eq!(model.records.len(), 1);
        assert_eq!(model.records[0].name, "x_y");
        assert_eq!(model.records[0].projects, [1, 2]);
    }

    #[test]
    fn records_with_the_same_names_and_other_types_are_numbered() {
        let model = model_of_app(&[
            "enum A = A { a: { x: Nat }, b: { x: String }, c: { x: Nat }, d: { x_2: Nat } };",
            "enum B = B { e: { x: Float } };",
        ]);
        let names: Vec<&str> = model.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["x", "x_2", "x_2_2", "x_3"]);
        assert_eq!(
            field_types(&model, "A"),
            [
                TypeRef::Record(0, vec![]),
                TypeRef::Record(1, vec![]),
                TypeRef::Record(0, vec![]),
                TypeRef::Record(2, vec![]),
            ]
        );
    }

    #[test]
    fn a_record_is_the_same_in_any_order_of_its_fields_and_empty_has_a_name() {
        let model = model_of_app(&["enum A = A { a: { cpu: Nat, mem: Nat }, b: { mem: Nat, cpu: Nat }, c: {} };"]);
        let names: Vec<&str> = model.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["cpu_mem", "empty"]);
        assert_eq!(
            field_types(&model, "A"),
            [
                TypeRef::Record(0, vec![]),
                TypeRef::Record(0, vec![]),
                TypeRef::Record(1, vec![]),
            ]
        );
    }

    #[test]
    fn a_record_is_the_same_whatever_its_parameters_are_called() {
        let model = model_of_app(&[
            "enum Foo<T> = Foo { pos: { x: T } };",
            "enum Bar<U> = Bar { at: { x: U } };",
            "enum Two<T, U> = Two { p: { a: T, b: U }, q: { a: U, b: T } };",
        ]);
        let names: Vec<&str> = model.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["x", "a_b"]);
        assert_eq!(model.records[0].params, ["A"]);
        assert_eq!(model.records[0].fields[0].ty, param("A"));
        assert_eq!(field_types(&model, "Foo"), [TypeRef::Record(0, vec![param("T")])]);
        assert_eq!(field_types(&model, "Bar"), [TypeRef::Record(0, vec![param("U")])]);
        assert_eq!(
            field_types(&model, "Two"),
            [
                TypeRef::Record(1, vec![param("T"), param("U")]),
                TypeRef::Record(1, vec![param("U"), param("T")]),
            ]
        );
    }

    #[test]
    fn a_record_in_a_record_comes_first() {
        let model = model_of_app(&["enum A<T> = A { r: { outer: { inner: [T] } } };"]);
        let names: Vec<&str> = model.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["inner", "outer"]);
        assert_eq!(model.records[1].fields[0].ty, TypeRef::Record(0, vec![param("A")]));
        assert_eq!(field_types(&model, "A"), [TypeRef::Record(1, vec![param("T")])]);
    }

    /// Which parameters or fields of the constructor are boxed.
    fn boxes(model: &Model, local: &str) -> Vec<bool> {
        match &constructor(model, local).body {
            Body::Tuple(parameters) => parameters.iter().map(|p| p.boxed).collect(),
            Body::Record(fields) => fields.iter().map(|f| f.boxed).collect(),
            Body::None | Body::Array(_) => Vec::new(),
        }
    }

    #[test]
    fn a_mutual_recursion_is_boxed_at_both_sites() {
        let model = model_of_app(&["enum Expr = Num(Nat), Neg(Term); enum Term = Lit(Nat), Paren(Expr);"]);
        assert_eq!(boxes(&model, "Num"), [false]);
        assert_eq!(boxes(&model, "Neg"), [true]);
        assert_eq!(boxes(&model, "Lit"), [false]);
        assert_eq!(boxes(&model, "Paren"), [true]);
    }

    #[test]
    fn a_recursion_through_an_array_is_not_boxed() {
        let model = model_of_app(&[
            "enum Tree<T> = Leaf(T), Node(Forest<T>); enum Forest<T> = Trees([Tree<T>]);",
            "enum Many<T> = Many([T]); enum N = N(Many<N>), Bare([N]);",
        ]);
        for local in ["Leaf", "Node", "Many", "N"] {
            assert_eq!(boxes(&model, local), [false], "{}", local);
        }
        assert!(model.enums.iter().all(|e| e.has_default));
    }

    #[test]
    fn a_recursion_through_an_option_is_boxed() {
        let model = model_of_app(&["enum Node = Node { value: Nat, next: Node? };"]);
        assert_eq!(boxes(&model, "Node"), [false, true]);
        // The default of `next` is `None`, so the default of the whole is finite.
        assert!(find_enum(&model, "Node").has_default);
    }

    #[test]
    fn a_recursion_through_a_type_argument_is_boxed_where_the_parameter_is_held() {
        let model = model_of_app(&[
            "enum Styled<T> = Styled { item: T, colour: Nat };",
            "enum Node = Leaf, Node(Styled<Node>);",
        ]);
        assert_eq!(boxes(&model, "Node"), [true]);
        assert_eq!(boxes(&model, "Styled"), [false, false]);
        // A parameter that is held through another declaration is held too.
        let model = model_of_app(&[
            "enum Inner<T> = Inner(T); enum Outer<U> = Outer(Inner<U>);",
            "enum Node = Leaf, Node(Outer<Node>);",
        ]);
        assert_eq!(boxes(&model, "Node"), [true]);
        assert_eq!(boxes(&model, "Inner"), [false]);
        assert_eq!(boxes(&model, "Outer"), [false]);
    }

    #[test]
    fn a_recursion_that_changes_its_argument_is_boxed() {
        let model = model_of_app(&["enum Nested<T> = Flat(T), Deep(Nested<[T]>);"]);
        assert_eq!(boxes(&model, "Flat"), [false]);
        assert_eq!(boxes(&model, "Deep"), [true]);
        assert!(find_enum(&model, "Nested").has_default);
    }

    #[test]
    fn a_recursion_through_a_record_boxes_the_field_and_the_record() {
        let model = model_of_app(&["enum Node = Node { r: { next: Node?, v: Nat }, w: Nat };"]);
        assert_eq!(boxes(&model, "Node"), [true, false]);
        let record = &model.records[0];
        assert_eq!(record.name, "next_v");
        let boxed: Vec<bool> = record.fields.iter().map(|f| f.boxed).collect();
        assert_eq!(boxed, [true, false]);
    }

    #[test]
    fn an_enum_whose_first_constructor_leads_back_to_it_has_no_default() {
        let cons_first = model_of_app(&["enum List<T> = Cons { head: T, tail: List<T> }, Nil;"]);
        assert!(!find_enum(&cons_first, "List").has_default);
        assert_eq!(boxes(&cons_first, "Cons"), [false, true]);
        let nil_first = model_of_app(&["enum List<T> = Nil, Cons { head: T, tail: List<T> };"]);
        assert!(find_enum(&nil_first, "List").has_default);
        let mutual = model_of_app(&["enum A = A(b: B); enum B = B(a: A); enum C = C(x: A);"]);
        assert!(!find_enum(&mutual, "A").has_default);
        assert!(!find_enum(&mutual, "B").has_default);
        // The default of `C` needs that of `A`, which reports that it has none, as `check` does.
        assert!(find_enum(&mutual, "C").has_default);
        let through_argument = model_of_app(&["enum Wrap<T> = Wrap(T); enum L = L(Wrap<L>), N;"]);
        assert!(!find_enum(&through_argument, "L").has_default);
        assert!(find_enum(&through_argument, "Wrap").has_default);
        let through_record = model_of_app(&["enum R = R { r: { again: R } }, S;"]);
        assert!(!find_enum(&through_record, "R").has_default);
    }

    #[test]
    fn a_generic_enum_whose_default_needs_its_argument_has_one() {
        let model = model_of_app(&["enum Wrap<T> = Wrap(T); enum Holder = Holder(Wrap<Nat>);"]);
        assert!(find_enum(&model, "Wrap").has_default);
        assert!(find_enum(&model, "Holder").has_default);
        assert!(find_enum(&model, "Option").has_default);
        assert!(find_enum(&model, "Bool").has_default);
    }

    #[test]
    fn an_option_or_a_parameter_can_be_the_absent_value() {
        let model = scenario();
        let types = field_types(&model, "Styled");
        assert!(types[0].can_be_absent(), "the parameter T");
        assert!(types[1].can_be_absent(), "Colour?");
        assert!(!types[2].can_be_absent(), "the record");
        assert!(!TypeRef::Enum(name("com.example.shapes", "Colour"), vec![]).can_be_absent());
        assert!(!TypeRef::Nat.can_be_absent());
        // An enum called `Option` of another project is not the prelude's.
        assert!(!TypeRef::Enum(name("example.app", "Option"), vec![]).can_be_absent());
    }

    #[test]
    fn a_run_with_an_error_in_a_value_builds_no_model() {
        let found = model_of(
            &[project("example.app", vec![], &["enum A = A(Nat);"])],
            Some("A(1);\nA(\"no\");"),
        )
        .unwrap_err();
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0].contains("root.values, line 2"), "{:?}", found);
    }

    #[test]
    fn a_run_with_an_error_in_the_types_builds_no_model() {
        assert!(model_of(&[project("example.app", vec![], &["enum A = A(Missing);"])], None).is_err());
    }

    fn examples() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("syntax/examples")
    }

    #[test]
    fn every_valid_types_example_builds_a_model() {
        let folder = examples().join("types/valid");
        let mut count = 0;
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let sources = vec![
                prelude_source(),
                Source {
                    name: path.display().to_string(),
                    text: std::fs::read_to_string(&path).unwrap(),
                },
            ];
            let layout = plain_layout(&sources);
            if let Err(found) = build_model(&sources, &layout, &CheckOptions::default()) {
                panic!("{}: {:?}", path.display(), found);
            }
            count += 1;
        }
        assert!(count > 10, "only {} examples", count);
    }

    /// Every folder under the folder that has a project-file, at any depth.
    fn project_folders(folder: &Path, found: &mut Vec<std::path::PathBuf>) {
        if folder.join(crate::project::PROJECT_FILE).is_file() {
            found.push(folder.to_path_buf());
        }
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                project_folders(&path, found);
            }
        }
    }

    #[test]
    fn every_valid_project_example_builds_a_model() {
        let mut folders = Vec::new();
        for kind in ["valid", "valid-imports", "valid-names", "valid-prelude"] {
            project_folders(&examples().join("project").join(kind), &mut folders);
        }
        folders.sort();
        assert!(folders.len() > 20, "only {} projects", folders.len());
        // A reference to a const is allowed, as one example needs it.
        let options = CheckOptions {
            allow_refs: true,
            ..CheckOptions::default()
        };
        for folder in folders {
            let set = ProjectSet::load(&folder, true).unwrap_or_else(|e| panic!("{}: {}", folder.display(), e));
            let mut sources: Vec<Source> = set
                .projects
                .iter()
                .flat_map(|project| &project.files)
                .map(|file| Source {
                    name: file.path.display().to_string(),
                    text: match file.embedded {
                        Some(text) => text.to_string(),
                        None => std::fs::read_to_string(&file.path).unwrap(),
                    },
                })
                .collect();
            // The values beside the project-file are in the root project.
            let mut values: Vec<_> = std::fs::read_dir(&folder)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|e| e == "values"))
                .collect();
            values.sort();
            for path in values {
                sources.push(Source {
                    name: path.display().to_string(),
                    text: std::fs::read_to_string(&path).unwrap(),
                });
            }
            let layout = set.layout(sources.len());
            let model = build_model(&sources, &layout, &options)
                .unwrap_or_else(|found| panic!("{}: {:?}", folder.display(), found));
            assert_eq!(model.projects.len(), set.projects.len());
        }
    }
}
