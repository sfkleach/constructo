//! The codec of the Rust target: the traits, the `Codec` with its settings and its tables of names and consts, and the
//! impls that read, write and give the default of every type of the run. The part that is the same in every file is
//! `codec.rs.in`, and this writes the rest.
//!
//! The codec is at the root of the file, and the impls for the types of a project are in the module of the project, so
//! that the types of their parts are written as the types themselves are, and need no other paths. A submodule sees
//! the private helpers of the codec, as Rust lets it. Every path is written in full, as in the types, so that no name
//! of the declarations can hide one that the codec needs: a constructor can be called `Ok` or `Some`.
//!
//! Each helper of a generic impl is given its type, as `self.field::<T>(…)`, since Rust would otherwise take it from
//! the bound of the impl, which is `ReadValue<T>`, where the part is something else, such as a `Vec<T>`.

use super::super::model::{Body, Const, Enum, FullName, Model, TypeRef};
use super::adopt::Adoption;
use super::names::{is_rusts_own, Fields};
use super::{generic_list, Writer};
use crate::ast::{TaggedBody, Value, ValueKind};
use crate::diagnostics::Diagnostic;
use crate::resolve::Written;
use std::collections::HashMap;

const RT: &str = "::constructo_runtime::codec";
const VALUE: &str = "::constructo_runtime::ast::Value";
const ERROR: &str = "::constructo_runtime::codec::Error";
const NO_DEFAULT: &str = "::constructo_runtime::codec::NoDefault";
const RESULT: &str = "::std::result::Result";
const OPTION: &str = "::std::option::Option";

/// The part of the codec that is the same in every file.
const FIXED: &str = include_str!("codec.rs.in");

/// The defaults of the settings of a codec that `check` has options for, so that a codec reads what `check` reads with
/// the same options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Defaults {
    pub widening: bool,
    pub default_limit: usize,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            widening: true,
            default_limit: crate::limits::DEFAULT_VALUE_LIMIT,
        }
    }
}

/// How a constructor is built and matched in Rust.
enum Form {
    /// A path alone: a variant with no data, a unit `struct`, or `true`.
    Unit(String),
    /// A tuple `struct`, or `Some`, in a variant if `outer` is given.
    Tuple { outer: Option<String>, path: String },
    /// The one part itself, which is the value of an adopted type that is the whole of the enum.
    Bare,
    /// A `struct` with named fields, in a variant if `outer` is given.
    Named {
        outer: Option<String>,
        path: String,
        names: Vec<String>,
    },
}

impl Form {
    /// The expression that builds the constructor from its parts, or the pattern that matches it with its parts bound.
    fn with(&self, parts: &[String]) -> String {
        let wrap = |outer: &Option<String>, inner: String| match outer {
            Some(outer) => format!("{}({})", outer, inner),
            None => inner,
        };
        match self {
            Form::Unit(path) => path.clone(),
            Form::Bare => parts[0].clone(),
            Form::Tuple { outer, path } => wrap(outer, format!("{}({})", path, parts.join(", "))),
            Form::Named { outer, path, names } if names.is_empty() => wrap(outer, format!("{} {{}}", path)),
            Form::Named { outer, path, names } => {
                let fields: Vec<String> = names
                    .iter()
                    .zip(parts)
                    .map(|(name, part)| format!("{}: {}", name, part))
                    .collect();
                wrap(outer, format!("{} {{ {} }}", path, fields.join(", ")))
            }
        }
    }
}

/// A constructor as the codec sees it.
struct Plan<'m> {
    /// The full name, which is the key of the tables, and the form that the root project writes.
    full: String,
    written: String,
    /// The local name, for messages.
    local: &'m str,
    body: &'m Body,
    form: Form,
    /// The Rust type of each part that is read: each field or parameter, or the element of an array.
    types: Vec<String>,
    /// The adoption of the constructor, whose type holds the parts, converted by its functions.
    adopted: Option<&'m Adoption>,
}

/// The type of an impl: its Rust type, its parameters, the name that a message gives it, and the path from its module
/// to the root, where the codec is: `self` or `super`.
struct Target {
    ty: String,
    params: Vec<String>,
    shown: String,
    up: &'static str,
}

impl Target {
    /// `impl<T> TRAIT<TYPE> for self::Codec where self::Codec: BOUNDS<T>`, with each bound on each parameter.
    fn header(&self, trait_name: &str, bounds: &[&str]) -> String {
        self.header_with(trait_name, bounds, false)
    }

    /// The header, and with `cloned` a bound of `Clone` on each parameter, which the impl that writes an adopted
    /// generic type has, since the function of the adoption gives the arguments back owned, and may clone them to.
    fn header_with(&self, trait_name: &str, bounds: &[&str], cloned: bool) -> String {
        let up = self.up;
        let mut header = format!(
            "impl{} {}::{}<{}> for {}::Codec",
            generic_list(self.params.iter().cloned()),
            up,
            trait_name,
            self.ty,
            up
        );
        if !self.params.is_empty() {
            let bounds: Vec<String> = self
                .params
                .iter()
                .flat_map(|p| bounds.iter().map(move |b| format!("{}::{}<{}>", up, b, p)))
                .collect();
            header.push_str(&format!(" where {}::Codec: {}", up, bounds.join(" + ")));
            if cloned {
                for param in &self.params {
                    header.push_str(&format!(", {}: ::std::clone::Clone", param));
                }
            }
        }
        header.push_str(" {");
        header
    }
}

/// The errors for the consts whose values Rust cannot hold, such as a `Nat` that does not fit a `u64`, which are at
/// their declarations. They are found when the code is generated, so that they are not left to fail on the first read.
pub(super) fn const_errors(model: &Model, index: &HashMap<FullName, usize>) -> Vec<Diagnostic> {
    model
        .consts
        .iter()
        .filter_map(|constant| {
            let why = fits(model, index, &constant.value, &constant.ty).err()?;
            let message = format!("Rust cannot hold the value of the const {}: {}", constant.name, why);
            Some(Diagnostic::new(
                &model.files[constant.at.file],
                constant.at.pos,
                message,
            ))
        })
        .collect()
}

/// Whether the value, which `check` has accepted as the type, has numbers that Rust can hold. A reference to a const
/// is not followed, since that const is checked itself, and a part whose type is a parameter is not known here.
fn fits(model: &Model, index: &HashMap<FullName, usize>, value: &Value, ty: &TypeRef) -> Result<(), String> {
    let number = |text: &str, what: &str, ok: bool| {
        if ok {
            Ok(())
        } else {
            Err(format!("The number {} does not fit {}", text, what))
        }
    };
    match (ty, &value.kind) {
        (TypeRef::Nat, ValueKind::Nat(text)) => number(text, "Nat", text.parse::<u64>().is_ok()),
        (TypeRef::Int, ValueKind::Nat(text) | ValueKind::Int(text)) => number(text, "Int", text.parse::<i64>().is_ok()),
        (TypeRef::Float, ValueKind::Nat(text) | ValueKind::Int(text) | ValueKind::Float(text)) => {
            number(text, "Float", text.parse::<f64>().is_ok_and(f64::is_finite))
        }
        (TypeRef::Array(element), ValueKind::Array(items)) => {
            items.iter().try_for_each(|item| fits(model, index, item, element))
        }
        (TypeRef::Record(at, args), ValueKind::Record(members)) => {
            let record = &model.records[*at];
            members
                .iter()
                .try_for_each(|member| match record.fields.iter().find(|f| f.name == member.name) {
                    Some(field) => fits(
                        model,
                        index,
                        &member.value,
                        &substitute(&field.ty, &record.params, args),
                    ),
                    None => Ok(()),
                })
        }
        (TypeRef::Enum(name, args), ValueKind::Tagged { name: written, body }) => {
            let Some(&at) = index.get(name) else {
                return Ok(());
            };
            let decl = &model.enums[at];
            let Some(constructor) = decl.constructors.iter().find(|c| c.name.to_string() == *written) else {
                return Ok(());
            };
            let params: Vec<String> = decl.params.iter().map(|p| p.name.clone()).collect();
            let ty = |t: &TypeRef| substitute(t, &params, args);
            match (&constructor.body, body) {
                (Body::Record(fields), Some(TaggedBody::Record(members))) => {
                    members
                        .iter()
                        .try_for_each(|member| match fields.iter().find(|f| f.name == member.name) {
                            Some(field) => fits(model, index, &member.value, &ty(&field.ty)),
                            None => Ok(()),
                        })
                }
                (Body::Tuple(parameters), Some(TaggedBody::Tuple(arguments))) => {
                    arguments.iter().enumerate().try_for_each(|(position, argument)| {
                        let parameter = match &argument.name {
                            Some(name) => parameters.iter().find(|p| p.name.as_deref() == Some(name)),
                            None => parameters.get(position),
                        };
                        match parameter {
                            Some(parameter) => fits(model, index, &argument.value, &ty(&parameter.ty)),
                            None => Ok(()),
                        }
                    })
                }
                (Body::Array(element), Some(TaggedBody::Array(items))) => {
                    items.iter().try_for_each(|item| fits(model, index, item, &ty(element)))
                }
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }
}

/// The type with each of the parameters replaced by its argument.
fn substitute(ty: &TypeRef, params: &[String], args: &[TypeRef]) -> TypeRef {
    match ty {
        TypeRef::Param(name) => match params.iter().position(|p| p == name) {
            Some(at) if at < args.len() => args[at].clone(),
            _ => ty.clone(),
        },
        TypeRef::Array(element) => TypeRef::Array(Box::new(substitute(element, params, args))),
        TypeRef::Enum(name, list) => {
            TypeRef::Enum(name.clone(), list.iter().map(|t| substitute(t, params, args)).collect())
        }
        TypeRef::Record(at, list) => TypeRef::Record(*at, list.iter().map(|t| substitute(t, params, args)).collect()),
        TypeRef::Nat | TypeRef::Int | TypeRef::Float | TypeRef::String => ty.clone(),
    }
}

/// A name as a Rust string literal.
fn quoted(name: &str) -> String {
    format!("{:?}", name)
}

/// The form in which the root project writes a constructor.
fn written(name: &FullName, written: &Written) -> String {
    match written {
        Written::Bare(name) => name.clone(),
        Written::Group(group, name) => format!("{}::{}", group, name),
        Written::Full => name.to_string(),
    }
}

impl<'w> Writer<'w> {
    /// The lines of the codec, at the root of the file: the part that is the same in every file, the tables, and the
    /// impls for the prelude's `Bool` and `Option`, which are Rust's own and have no module.
    pub(super) fn codec(&self, defaults: &Defaults) -> Vec<String> {
        let root = &self.model.projects[self.model.root];
        let what = if root.name.is_empty() {
            "the types files".to_string()
        } else {
            format!("the project `{}`", root.name)
        };
        let fixed = FIXED
            .replace("@WHAT@", &what)
            .replace("@CONSTS@", &self.model.consts.len().to_string())
            .replace("@DEFAULT_LIMIT@", &defaults.default_limit.to_string())
            .replace("@WIDENING@", &defaults.widening.to_string());
        let mut lines: Vec<String> = fixed.lines().map(str::to_string).collect();
        lines.extend(self.tables());
        for (at, decl) in self.model.enums.iter().enumerate() {
            if is_rusts_own(&decl.name) {
                self.enum_impls(at, decl, &mut lines);
            }
        }
        lines
    }

    /// The impls for the types of a project, in its module: its enums, and the records that it uses.
    pub(super) fn impls(&self, project: usize) -> Vec<String> {
        let mut lines = Vec::new();
        for (at, decl) in self.model.enums.iter().enumerate() {
            if decl.project == project && !is_rusts_own(&decl.name) {
                self.enum_impls(at, decl, &mut lines);
            }
        }
        for (at, record) in self.model.records.iter().enumerate() {
            if record.projects.contains(&project) {
                self.record_impls(at, project, &mut lines);
            }
        }
        lines
    }

    /// The path from the module of the project to the root.
    fn up(&self, project: usize) -> &'static str {
        if project == self.model.root {
            "self"
        } else {
            "super"
        }
    }

    /// The tables of the names of constructors and of consts, from each name by which the root project reads them.
    fn tables(&self) -> Vec<String> {
        let mut lines = vec![
            String::new(),
            "impl self::Codec {".to_string(),
            "    /// The full name of a constructor of the run, and the name of its type, from each name by which the"
                .to_string(),
            "    /// project reads it.".to_string(),
            format!(
                "    fn constructor(name: &str) -> {}<(&'static str, &'static str)> {{",
                OPTION
            ),
            "        match name {".to_string(),
        ];
        for decl in &self.model.enums {
            for constructor in &decl.constructors {
                if constructor.read.is_empty() {
                    continue;
                }
                let forms: Vec<String> = constructor.read.iter().map(|n| quoted(n)).collect();
                lines.push(format!(
                    "            {} => {}::Some(({}, {})),",
                    forms.join(" | "),
                    OPTION,
                    quoted(&constructor.name.to_string()),
                    quoted(&decl.name.local)
                ));
            }
        }
        lines.push(format!("            _ => {}::None,", OPTION));
        lines.push("        }".to_string());
        lines.push("    }".to_string());
        lines.push(String::new());
        lines.push(
            "    /// The index of a const of the run, and the text of its value, with every name in full, from each"
                .to_string(),
        );
        lines.push("    /// name by which the project reads it.".to_string());
        lines.push(format!(
            "    fn constant(name: &str) -> {}<(usize, &'static str)> {{",
            OPTION
        ));
        lines.push("        match name {".to_string());
        for (at, constant) in self.model.consts.iter().enumerate() {
            if constant.read.is_empty() {
                continue;
            }
            let forms: Vec<String> = constant.read.iter().map(|n| quoted(n)).collect();
            lines.push(format!(
                "            {} => {}::Some(({}, {})),",
                forms.join(" | "),
                OPTION,
                at,
                quoted(&const_text(constant))
            ));
        }
        lines.push(format!("            _ => {}::None,", OPTION));
        lines.push("        }".to_string());
        lines.push("    }".to_string());
        lines.push("}".to_string());
        lines
    }

    /// The three impls of an enum, and three more for the type that adopts it whole, if one does.
    fn enum_impls(&self, at: usize, decl: &'w Enum, lines: &mut Vec<String>) {
        let (target, plans) = self.enum_plans(at, decl);
        self.read_impl(&target, &plans, lines);
        self.write_impl(&target, &plans, lines);
        self.default_impl(&target, decl.has_default.then(|| &plans[0]), lines);
        if let Some(adoption) = self.adopted_whole(at) {
            self.whole_impls(&target, adoption, lines);
        }
    }

    /// The impls for a type that adopts an enum whole. The generated enum is the step in between: the value is read
    /// into it, which decides its constructor, and its function converts it, and the other way round.
    fn whole_impls(&self, generated: &Target, adoption: &Adoption, lines: &mut Vec<String>) {
        let target = Target {
            ty: format!("{}{}", adoption.ty.path, generic_list(generated.params.iter().cloned())),
            params: generated.params.clone(),
            shown: generated.shown.clone(),
            up: generated.up,
        };
        let (from, to) = (&adoption.from.path, &adoption.to.path);
        lines.push(String::new());
        lines.push(target.header("ReadValue", &["ReadValue", "DefaultValue"]));
        lines.push(format!(
            "    fn read_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            VALUE, RESULT, target.ty, ERROR
        ));
        lines.push(format!(
            "        let generated = <Self as {}::ReadValue<{}>>::read_value(self, value, depth)?;",
            target.up, generated.ty
        ));
        lines.push(format!("        self.adopted(value, {}(generated))", from));
        lines.push("    }".to_string());
        lines.push("}".to_string());
        lines.push(String::new());
        lines.push(target.header_with("WriteValue", &["WriteValue"], true));
        lines.push(format!(
            "    fn write_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            target.ty, RESULT, VALUE, ERROR
        ));
        lines.push(format!(
            "        let generated = {}(value).map_err({}::new)?;",
            to, ERROR
        ));
        lines.push(format!(
            "        <Self as {}::WriteValue<{}>>::write_value(self, &generated, depth)",
            target.up, generated.ty
        ));
        lines.push("    }".to_string());
        lines.push("}".to_string());
        lines.push(String::new());
        lines.push(target.header("DefaultValue", &["DefaultValue"]));
        lines.push(format!(
            "    fn default_value(&self, budget: &mut usize) -> {}<{}, {}> {{",
            RESULT, target.ty, NO_DEFAULT
        ));
        lines.push(format!(
            "        let generated = <Self as {}::DefaultValue<{}>>::default_value(self, budget)?;",
            target.up, generated.ty
        ));
        lines.push(format!("        {}(generated).map_err({}::Refused)", from, NO_DEFAULT));
        lines.push("    }".to_string());
        lines.push("}".to_string());
    }

    /// The type of an enum, in the module of its project, and the plan of each of its constructors.
    fn enum_plans(&self, at: usize, decl: &'w Enum) -> (Target, Vec<Plan<'w>>) {
        let project = decl.project;
        let kept: Vec<String> = self.names.params[at]
            .iter()
            .zip(&self.names.kept[at])
            .filter(|(_, &keep)| keep)
            .map(|(name, _)| name.clone())
            .collect();
        let rename: HashMap<&str, &str> = decl
            .params
            .iter()
            .zip(&self.names.params[at])
            .map(|(param, rust)| (param.name.as_str(), rust.as_str()))
            .collect();
        let plan = |c: usize, form: Form| {
            let constructor = &decl.constructors[c];
            let types = match &constructor.body {
                Body::None => Vec::new(),
                Body::Record(fields) => fields
                    .iter()
                    .map(|f| self.field_type(&f.ty, f.boxed, project, &rename))
                    .collect(),
                Body::Tuple(parameters) => parameters
                    .iter()
                    .map(|p| self.field_type(&p.ty, p.boxed, project, &rename))
                    .collect(),
                Body::Array(element) => vec![self.ty(element, project, &rename)],
            };
            Plan {
                full: constructor.name.to_string(),
                written: written(&constructor.name, &constructor.written),
                local: &constructor.name.local,
                body: &constructor.body,
                form,
                types,
                adopted: self.adopted_constructor(at, c),
            }
        };
        let up = self.up(project);
        if is_rusts_own(&decl.name) {
            let (ty, forms): (String, Vec<Form>) = match decl.name.local.as_str() {
                "Bool" => (
                    "::std::primitive::bool".to_string(),
                    decl.constructors
                        .iter()
                        .map(|c| Form::Unit(c.name.local.to_lowercase()))
                        .collect(),
                ),
                _ => (
                    format!("{}<{}>", OPTION, kept.join(", ")),
                    decl.constructors
                        .iter()
                        .map(|c| match c.body {
                            Body::None => Form::Unit(format!("{}::None", OPTION)),
                            _ => Form::Tuple {
                                outer: None,
                                path: format!("{}::Some", OPTION),
                            },
                        })
                        .collect(),
                ),
            };
            let plans = forms.into_iter().enumerate().map(|(c, form)| plan(c, form)).collect();
            let target = Target {
                ty,
                params: kept,
                shown: decl.name.local.clone(),
                up: "self",
            };
            return (target, plans);
        }
        let path = format!("self::{}", self.names.types[at]);
        let struct_form = |outer: Option<String>, path: String, body: &Body, fields: &Fields| match (body, fields) {
            (Body::None, _) => Form::Unit(outer.unwrap_or(path)),
            (Body::Array(_), _) | (Body::Tuple(_), Fields::Tuple) => Form::Tuple { outer, path },
            (_, Fields::Named(names)) => Form::Named {
                outer,
                path,
                names: names.clone(),
            },
            (Body::Record(_), Fields::Tuple) => unreachable!("the fields of a record have names"),
        };
        let single = decl.constructors.len() == 1;
        let plans = if single && self.adopted_constructor(at, 0).is_some() {
            vec![plan(0, Form::Bare)]
        } else if single {
            let constructor = &decl.constructors[0];
            let fields = &self.names.fields[at][0];
            vec![plan(0, struct_form(None, path.clone(), &constructor.body, fields))]
        } else {
            decl.constructors
                .iter()
                .enumerate()
                .map(|(c, constructor)| {
                    let variant = format!("{}::{}", path, self.names.variants[at][c]);
                    let form = match &self.names.structs[at][c] {
                        // A variant whose constructor is adopted holds the adopted type.
                        Some(_) if self.adopted_constructor(at, c).is_some() => Form::Tuple {
                            outer: None,
                            path: variant,
                        },
                        None => Form::Unit(variant),
                        Some(name) => struct_form(
                            Some(variant),
                            format!("self::{}", name),
                            &constructor.body,
                            &self.names.fields[at][c],
                        ),
                    };
                    plan(c, form)
                })
                .collect()
        };
        // The type of an enum whose one constructor is adopted is the adopted type.
        let ty = match self.adopted_constructor(at, 0).filter(|_| single) {
            Some(adoption) => adoption.ty.path.clone(),
            None => path,
        };
        let target = Target {
            ty: format!("{}{}", ty, generic_list(kept.iter().cloned())),
            params: kept,
            shown: decl.name.local.clone(),
            up,
        };
        (target, plans)
    }

    /// The impl of `ReadValue` for an enum: the name of a tagged value is matched against the full names of its
    /// constructors, and any other is an error.
    fn read_impl(&self, target: &Target, plans: &[Plan], lines: &mut Vec<String>) {
        let deep = plans.iter().any(|p| has_parts(p.body));
        lines.push(String::new());
        lines.push(target.header("ReadValue", &["ReadValue", "DefaultValue"]));
        lines.push(format!(
            "    fn read_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            VALUE, RESULT, target.ty, ERROR
        ));
        lines.push(format!(
            "        self.read_with::<{}>(value, depth, |value, {}| {{",
            target.ty,
            if deep { "depth" } else { "_" }
        ));
        lines.push(format!(
            "            let (name, body) = self.tagged(value, {})?;",
            quoted(&target.shown)
        ));
        lines.push("            match name {".to_string());
        for plan in plans {
            let local = quoted(plan.local);
            lines.push(format!("                {} => {{", quoted(&plan.full)));
            let parts: Vec<String> = match plan.body {
                Body::None => {
                    lines.push(format!("                    self.no_body(value, body, {})?;", local));
                    Vec::new()
                }
                Body::Record(fields) => {
                    let names: Vec<String> = fields.iter().map(|f| quoted(&f.name)).collect();
                    let slots: Vec<String> = (0..fields.len()).map(|i| format!("f{}", i)).collect();
                    lines.push(format!(
                        "                    let members = self.record_body(value, body, {})?;",
                        local
                    ));
                    lines.push(format!(
                        "                    let [{}] = {}::fields(members, [{}], {}, value.pos)?;",
                        slots.join(", "),
                        RT,
                        names.join(", "),
                        local
                    ));
                    slots
                        .iter()
                        .zip(&names)
                        .zip(&plan.types)
                        .map(|((slot, name), ty)| format!("self.field::<{}>({}, depth, {})?", ty, slot, name))
                        .collect()
                }
                Body::Tuple(parameters) => {
                    let named = |name: &Option<String>| match name {
                        Some(name) => format!("{}::Some({})", OPTION, quoted(name)),
                        None => format!("{}::None", OPTION),
                    };
                    let params: Vec<String> = parameters.iter().map(|p| named(&p.name)).collect();
                    let slots: Vec<String> = (0..parameters.len()).map(|i| format!("a{}", i)).collect();
                    lines.push(format!(
                        "                    let args = self.tuple_body(value, body, {})?;",
                        local
                    ));
                    lines.push(format!(
                        "                    let [{}] = {}::arguments(args, [{}], {}, value.pos)?;",
                        slots.join(", "),
                        RT,
                        params.join(", "),
                        local
                    ));
                    slots
                        .iter()
                        .enumerate()
                        .map(|(i, slot)| {
                            format!(
                                "self.argument::<{}>({}, depth, value, {}, {})?",
                                plan.types[i], slot, i, params[i]
                            )
                        })
                        .collect()
                }
                Body::Array(_) => {
                    lines.push(format!(
                        "                    let items = self.array_body(value, body, {})?;",
                        local
                    ));
                    vec![format!("self.items::<{}>(items, depth)?", plan.types[0])]
                }
            };
            let parts = match plan.adopted {
                Some(adoption) => {
                    lines.push(format!("                    let arguments = {};", arguments(&parts)));
                    lines.push(format!(
                        "                    let adopted = self.adopted(value, {}(arguments))?;",
                        adoption.from.path
                    ));
                    vec!["adopted".to_string()]
                }
                None => parts,
            };
            lines.push(format!(
                "                    {}::Ok({})",
                RESULT,
                plan.form.with(&parts)
            ));
            lines.push("                }".to_string());
        }
        lines.push(format!(
            "                _ => {}::Err(self.mismatch({}, value)),",
            RESULT,
            quoted(&target.shown)
        ));
        lines.push("            }".to_string());
        lines.push("        })".to_string());
        lines.push("    }".to_string());
        lines.push("}".to_string());
    }

    /// The impl of `WriteValue` for an enum: a `match` on its constructors, each written with the name in the form of
    /// the codec's setting.
    fn write_impl(&self, target: &Target, plans: &[Plan], lines: &mut Vec<String>) {
        let deep = plans.iter().any(|p| has_parts(p.body));
        lines.push(String::new());
        let cloned = plans.iter().any(|p| p.adopted.is_some());
        lines.push(target.header_with("WriteValue", &["WriteValue"], cloned));
        lines.push(format!(
            "    fn write_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            target.ty, RESULT, VALUE, ERROR
        ));
        if deep {
            lines.push("        let depth = self.enter_write(depth)?;".to_string());
        } else {
            lines.push("        self.enter_write(depth)?;".to_string());
        }
        lines.push(format!("        {}::Ok(match value {{", RESULT));
        for plan in plans {
            let name = format!("self.name({}, {})", quoted(&plan.full), quoted(&plan.written));
            let count = match plan.body {
                Body::None => 0,
                Body::Record(fields) => fields.len(),
                Body::Tuple(parameters) => parameters.len(),
                Body::Array(_) => 1,
            };
            let binders: Vec<String> = (0..count).map(|i| format!("f{}", i)).collect();
            // The parts of an adopted type are made by its function, and are owned, so they are lent to the writers.
            let slots: Vec<String> = match plan.adopted {
                Some(_) => binders.iter().map(|b| format!("&{}", b)).collect(),
                None => binders.clone(),
            };
            let expr = match plan.body {
                Body::None => format!("{}::unit({})", RT, name),
                Body::Record(fields) => {
                    let members: Vec<String> = fields
                        .iter()
                        .zip(&slots)
                        .zip(&plan.types)
                        .map(|((f, slot), ty)| {
                            let field = quoted(&f.name);
                            format!("({}, self.put::<{}>({}, depth, {})?)", field, ty, slot, field)
                        })
                        .collect();
                    format!("{}::tagged_record({}, ::std::vec![{}])", RT, name, members.join(", "))
                }
                Body::Tuple(parameters) => {
                    let args: Vec<String> = parameters
                        .iter()
                        .zip(&slots)
                        .enumerate()
                        .map(|(i, (p, slot))| {
                            let ty = &plan.types[i];
                            match &p.name {
                                Some(n) => format!(
                                    "({}::Some({}), self.put::<{}>({}, depth, {})?)",
                                    OPTION,
                                    quoted(n),
                                    ty,
                                    slot,
                                    quoted(n)
                                ),
                                None => format!("({}::None, self.put_at::<{}>({}, depth, {})?)", OPTION, ty, slot, i),
                            }
                        })
                        .collect();
                    format!("{}::tagged_tuple({}, ::std::vec![{}])", RT, name, args.join(", "))
                }
                Body::Array(_) => format!(
                    "{}::tagged_array({}, self.put_items::<{}>(f0, depth)?)",
                    RT, name, plan.types[0]
                ),
            };
            match plan.adopted {
                Some(adoption) => lines.push(format!(
                    "            {} => {{ let {} = {}(adopted).map_err({}::new)?; {} }}",
                    plan.form.with(&["adopted".to_string()]),
                    arguments(&binders),
                    adoption.to.path,
                    ERROR,
                    expr
                )),
                None => lines.push(format!("            {} => {},", plan.form.with(&binders), expr)),
            }
        }
        lines.push("        })".to_string());
        lines.push("    }".to_string());
        lines.push("}".to_string());
    }

    /// The impl of `DefaultValue` for an enum: its first constructor with the default of each of its parts, or for an
    /// enum that has none, the error that says so.
    fn default_impl(&self, target: &Target, first: Option<&Plan>, lines: &mut Vec<String>) {
        lines.push(String::new());
        lines.push(target.header("DefaultValue", &["DefaultValue"]));
        let budget = if first.is_some() { "budget" } else { "_budget" };
        lines.push(format!(
            "    fn default_value(&self, {}: &mut usize) -> {}<{}, {}> {{",
            budget, RESULT, target.ty, NO_DEFAULT
        ));
        match first {
            None => lines.push(format!(
                "        {}::Err({}::None(::std::string::String::from({})))",
                RESULT,
                NO_DEFAULT,
                quoted(&target.shown)
            )),
            Some(plan) => {
                lines.push(format!("        {}::spend(budget)?;", RT));
                let parts: Vec<String> = match plan.body {
                    Body::Array(_) => vec!["::std::vec::Vec::new()".to_string()],
                    _ => plan
                        .types
                        .iter()
                        .map(|ty| format!("self.default_of::<{}>(budget)?", ty))
                        .collect(),
                };
                // The default of an adopted type is made by its function from the defaults of its arguments.
                let parts = match plan.adopted {
                    Some(adoption) => vec![format!(
                        "{}({}).map_err({}::Refused)?",
                        adoption.from.path,
                        arguments(&parts),
                        NO_DEFAULT
                    )],
                    None => parts,
                };
                lines.push(format!("        {}::Ok({})", RESULT, plan.form.with(&parts)));
            }
        }
        lines.push("    }".to_string());
        lines.push("}".to_string());
    }

    /// The impls for the `struct` of a record type without a name in the module of a project: it is read from a record
    /// with its fields in any order, written with them in the sorted order, and its default is that of each field.
    fn record_impls(&self, at: usize, project: usize, lines: &mut Vec<String>) {
        let record = &self.model.records[at];
        let params: Vec<String> = record
            .params
            .iter()
            .zip(&self.names.kept_records[at])
            .filter(|(_, &keep)| keep)
            .map(|(name, _)| name.clone())
            .collect();
        let path = format!("self::{}", self.names.records[at]);
        let target = Target {
            ty: format!("{}{}", path, generic_list(params.iter().cloned())),
            params,
            shown: "a record".to_string(),
            up: self.up(project),
        };
        let form = Form::Named {
            outer: None,
            path,
            names: self.names.record_fields[at].clone(),
        };
        let rename = HashMap::new();
        let types: Vec<String> = record
            .fields
            .iter()
            .map(|f| self.field_type(&f.ty, f.boxed, project, &rename))
            .collect();
        let deep = !record.fields.is_empty();
        let names: Vec<String> = record.fields.iter().map(|f| quoted(&f.name)).collect();
        let slots: Vec<String> = (0..names.len()).map(|i| format!("f{}", i)).collect();

        lines.push(String::new());
        lines.push(target.header("ReadValue", &["ReadValue", "DefaultValue"]));
        lines.push(format!(
            "    fn read_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            VALUE, RESULT, target.ty, ERROR
        ));
        lines.push(format!(
            "        self.read_with::<{}>(value, depth, |value, {}| match &value.kind {{",
            target.ty,
            if deep { "depth" } else { "_" }
        ));
        lines.push("            ::constructo_runtime::ast::ValueKind::Record(members) => {".to_string());
        lines.push(format!(
            "                let [{}] = {}::fields(members, [{}], \"record\", value.pos)?;",
            slots.join(", "),
            RT,
            names.join(", ")
        ));
        let parts: Vec<String> = slots
            .iter()
            .zip(&names)
            .zip(&types)
            .map(|((slot, name), ty)| format!("self.field::<{}>({}, depth, {})?", ty, slot, name))
            .collect();
        lines.push(format!("                {}::Ok({})", RESULT, form.with(&parts)));
        lines.push("            }".to_string());
        lines.push(format!(
            "            _ => {}::Err(self.mismatch(\"a record\", value)),",
            RESULT
        ));
        lines.push("        })".to_string());
        lines.push("    }".to_string());
        lines.push("}".to_string());

        lines.push(String::new());
        lines.push(target.header("WriteValue", &["WriteValue"]));
        lines.push(format!(
            "    fn write_value(&self, value: &{}, depth: u32) -> {}<{}, {}> {{",
            target.ty, RESULT, VALUE, ERROR
        ));
        if deep {
            lines.push("        let depth = self.enter_write(depth)?;".to_string());
        } else {
            lines.push("        self.enter_write(depth)?;".to_string());
        }
        let members: Vec<String> = slots
            .iter()
            .zip(&names)
            .zip(&types)
            .map(|((slot, name), ty)| format!("({}, self.put::<{}>({}, depth, {})?)", name, ty, slot, name))
            .collect();
        lines.push(format!("        let {} = value;", form.with(&slots)));
        lines.push(format!(
            "        {}::Ok({}::record(::std::vec![{}]))",
            RESULT,
            RT,
            members.join(", ")
        ));
        lines.push("    }".to_string());
        lines.push("}".to_string());

        lines.push(String::new());
        lines.push(target.header("DefaultValue", &["DefaultValue"]));
        lines.push(format!(
            "    fn default_value(&self, budget: &mut usize) -> {}<{}, {}> {{",
            RESULT, target.ty, NO_DEFAULT
        ));
        lines.push(format!("        {}::spend(budget)?;", RT));
        let parts: Vec<String> = types
            .iter()
            .map(|ty| format!("self.default_of::<{}>(budget)?", ty))
            .collect();
        lines.push(format!("        {}::Ok({})", RESULT, form.with(&parts)));
        lines.push("    }".to_string());
        lines.push("}".to_string());
    }
}

/// The arguments that the functions of an adoption take and give: the one part itself, or a tuple of the parts in the
/// order of the declaration, which is `()` if there are none.
fn arguments(parts: &[String]) -> String {
    match parts {
        [part] => part.clone(),
        _ => format!("({})", parts.join(", ")),
    }
}

/// Whether a body has parts, which are read and written a level down.
fn has_parts(body: &Body) -> bool {
    match body {
        Body::None => false,
        Body::Record(fields) => !fields.is_empty(),
        Body::Tuple(parameters) => !parameters.is_empty(),
        Body::Array(_) => true,
    }
}

/// The text of the value of a const, compact, with every name in full.
fn const_text(constant: &Const) -> String {
    crate::values_text::render(&constant.value, true)
        .trim_end_matches(";\n")
        .to_string()
}
