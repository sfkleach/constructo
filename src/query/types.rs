//! What a typed query knows of the types: the declarations, owned so that they outlive the check that read them, and
//! the type of each value as far as it can be worked out from them. A typed query uses them to find a named argument
//! that was given by position, the default of one that was left out, the value of a const, and the kind of a number
//! that the types widen, as `5` in a `Float` field is a `Float`.

use crate::ast::{Argument, ConstDecl, EnumDecl, Member, TaggedBody, Value, ValueKind, Variant, VariantBody};
use crate::diagnostics::Pos;
use crate::limits::MAX_DEPTH;
use crate::parser::Parsed;
use crate::typecheck::{fill, instantiate, instantiate_fields, Env, Ty};
use std::collections::HashMap;
use std::rc::Rc;

/// What the run of a typed query is given, once every value has been checked.
pub struct QueryInputs<'s> {
    pub types: Types,
    /// Each value, resolved, with the index of its source and the type that the check inferred for it, which is `None`
    /// with `--type`, or if the type was too large to work out.
    pub values: Vec<(usize, Value, Option<Ty>)>,
    /// The type of every value, if `--type` gave one.
    pub target: Option<Ty>,
    /// The constructor names of the query, resolved, in the order in which they are written.
    pub names: Vec<String>,
    /// The index that the values that the query makes are shown as if they were in, which is that of its names.
    pub names_source: usize,
    pub source_names: Vec<String>,
    /// Writes one canonical name as the source with the index writes it: bare, else through a group, else in full.
    pub write: &'s dyn Fn(usize, &str) -> String,
}

/// The declarations of a run, owned.
pub struct Types {
    ctors: HashMap<String, (Rc<EnumDecl>, usize)>,
    enums: HashMap<String, Rc<EnumDecl>>,
    consts: HashMap<String, (Value, Option<Ty>)>,
    default_limit: usize,
}

/// A type that is not known, such as an argument of an enum that is not known from the value alone.
pub fn unknown() -> Ty {
    Ty::Param("?".to_string())
}

/// Whether a type is known, as opposed to a variable or a parameter.
pub fn known(ty: &Ty) -> bool {
    !matches!(ty, Ty::Var(_) | Ty::Param(_) | Ty::Gen(_))
}

impl Types {
    /// Gathers the declarations of the types files, with the types of the consts from the environment that checked
    /// them.
    pub fn from_parsed(parsed: &[Parsed], env: &Env) -> Types {
        let mut types = Types {
            ctors: HashMap::new(),
            enums: HashMap::new(),
            consts: HashMap::new(),
            default_limit: env.default_limit(),
        };
        for file in parsed {
            let Parsed::Types(file) = file else { continue };
            for decl in &file.enums {
                let decl = Rc::new(decl.clone());
                for (index, variant) in decl.variants.iter().enumerate() {
                    types.ctors.entry(variant.name.clone()).or_insert((decl.clone(), index));
                }
                types.enums.entry(decl.name.clone()).or_insert(decl);
            }
            for constant in &file.consts {
                let ty = const_type(constant, env);
                types
                    .consts
                    .entry(constant.name.clone())
                    .or_insert((constant.value.clone(), ty));
            }
        }
        types
    }

    /// The constructor with the canonical name, with the enum that declares it.
    pub fn ctor(&self, name: &str) -> Option<(&EnumDecl, &Variant)> {
        self.ctors
            .get(name)
            .map(|(decl, index)| (&**decl, &decl.variants[*index]))
    }

    /// The value of a const and its type, if the name is a const.
    pub fn constant(&self, name: &str) -> Option<&(Value, Option<Ty>)> {
        self.consts.get(name)
    }

    /// The type of a top-level value, as far as the value tells: the enum of its constructor, with its arguments not
    /// known.
    pub fn type_of(&self, value: &Value) -> Option<Ty> {
        match &value.kind {
            ValueKind::Tagged { name, .. } => {
                let (decl, _) = self.ctor(name)?;
                let args = decl.params.iter().map(|_| unknown()).collect();
                Some(Ty::Enum(decl.name.clone(), Rc::new(args)))
            }
            _ => None,
        }
    }

    /// The arguments of the enum that declares a constructor, from the type of a value of it if that is known.
    fn enum_args(decl: &EnumDecl, ty: Option<&Ty>) -> Vec<Ty> {
        match ty {
            Some(Ty::Enum(name, args)) if *name == decl.name && args.len() == decl.params.len() => args.to_vec(),
            _ => decl.params.iter().map(|_| unknown()).collect(),
        }
    }

    /// The types of the fields of a record, bare or as the body of a constructor, given the type of the value.
    pub fn record_fields(&self, value: &Value, ty: Option<&Ty>) -> Option<Vec<(String, Ty)>> {
        match (&value.kind, ty) {
            (ValueKind::Record(_), Some(Ty::Record(fields))) => Some(fields.to_vec()),
            (ValueKind::Tagged { name, .. }, _) => {
                let (decl, variant) = self.ctor(name)?;
                let VariantBody::Record(fields) = &variant.body else {
                    return None;
                };
                Some(instantiate_fields(fields, &decl.params, &Types::enum_args(decl, ty)))
            }
            _ => None,
        }
    }

    /// The fields of a constructor with a record body, with their types, given the type of a value of it.
    pub fn ctor_fields(&self, name: &str, ty: Option<&Ty>) -> Option<Vec<(String, Ty)>> {
        let (decl, variant) = self.ctor(name)?;
        let VariantBody::Record(fields) = &variant.body else {
            return None;
        };
        Some(instantiate_fields(fields, &decl.params, &Types::enum_args(decl, ty)))
    }

    /// The type of the elements of a constructor with an array body, given the type of a value of it.
    pub fn ctor_element(&self, name: &str, ty: Option<&Ty>) -> Option<Ty> {
        let (decl, variant) = self.ctor(name)?;
        let VariantBody::Array(element) = &variant.body else {
            return None;
        };
        Some(instantiate(element, &decl.params, &Types::enum_args(decl, ty)))
    }

    /// The enum with the canonical name.
    pub fn enum_decl(&self, name: &str) -> Option<&EnumDecl> {
        self.enums.get(name).map(|decl| &**decl)
    }

    /// The type of a value of a constructor: its enum, with the arguments of `ty` if it is a value of that enum.
    pub fn ctor_type(&self, name: &str, ty: Option<&Ty>) -> Option<Ty> {
        let (decl, _) = self.ctor(name)?;
        Some(Ty::Enum(decl.name.clone(), Rc::new(Types::enum_args(decl, ty))))
    }

    /// The parameters of a tuple constructor, with their types, given the type of the value.
    pub fn tuple_params(&self, name: &str, ty: Option<&Ty>) -> Option<(Vec<crate::ast::Parameter>, Vec<Ty>)> {
        let (decl, variant) = self.ctor(name)?;
        let VariantBody::Tuple(params) = &variant.body else {
            return None;
        };
        let args = Types::enum_args(decl, ty);
        let tys = params.iter().map(|p| instantiate(&p.ty, &decl.params, &args)).collect();
        Some((params.clone(), tys))
    }

    /// The type of the elements of an array, bare or as the body of a constructor, given the type of the value.
    pub fn element_type(&self, value: &Value, ty: Option<&Ty>) -> Option<Ty> {
        match (&value.kind, ty) {
            (ValueKind::Array(_), Some(Ty::Array(element))) => Some((**element).clone()),
            (ValueKind::Tagged { name, .. }, _) => {
                let (decl, variant) = self.ctor(name)?;
                let VariantBody::Array(element) = &variant.body else {
                    return None;
                };
                Some(instantiate(element, &decl.params, &Types::enum_args(decl, ty)))
            }
            _ => None,
        }
    }

    /// The default value of a type, which is what an omitted argument is: the zero of a number, the empty string and
    /// the empty array, the record of the defaults of its fields, and the first constructor of an enum with the
    /// defaults of its fields. The check of the value has found that it can be worked out within the limit.
    pub fn default_value(&self, ty: &Ty, pos: Pos) -> Result<Value, String> {
        let mut budget = self.default_limit;
        self.default_in(ty, pos, &mut budget, 0)
    }

    fn default_in(&self, ty: &Ty, pos: Pos, budget: &mut usize, depth: u32) -> Result<Value, String> {
        // The check of the value counted the nodes against the same limit, so these are defensive checks that keep the
        // recursion bounded whatever the declarations are.
        if *budget == 0 || depth > MAX_DEPTH {
            return Err("The default value of the omitted argument is too large to work out".to_string());
        }
        *budget -= 1;
        let value = |kind| Ok(Value { pos, kind });
        match ty {
            Ty::Nat => value(ValueKind::Nat("0".to_string())),
            Ty::Int => value(ValueKind::Int("+0".to_string())),
            Ty::Float => value(ValueKind::Float("0.0".to_string())),
            Ty::Str => value(ValueKind::Str(String::new())),
            Ty::Array(_) => value(ValueKind::Array(Vec::new())),
            Ty::Record(fields) => {
                let mut members = Vec::new();
                for (name, field) in fields.iter() {
                    members.push(Member {
                        name: name.clone(),
                        pos,
                        value: self.default_in(field, pos, budget, depth + 1)?,
                    });
                }
                value(ValueKind::Record(members))
            }
            Ty::Enum(name, args) => {
                let Some(decl) = self.enums.get(name) else {
                    return Err(format!("Cannot work out the default value of {}", name));
                };
                let Some(variant) = decl.variants.first() else {
                    return Err(format!("Cannot work out the default value of {}", name));
                };
                let body = match &variant.body {
                    VariantBody::Unit => None,
                    VariantBody::Array(_) => Some(TaggedBody::Array(Vec::new())),
                    VariantBody::Record(fields) => {
                        let mut members = Vec::new();
                        for (field, field_ty) in instantiate_fields(fields, &decl.params, args) {
                            members.push(Member {
                                name: field,
                                pos,
                                value: self.default_in(&field_ty, pos, budget, depth + 1)?,
                            });
                        }
                        Some(TaggedBody::Record(members))
                    }
                    VariantBody::Tuple(params) => {
                        let mut out = Vec::new();
                        for p in params {
                            let p_ty = instantiate(&p.ty, &decl.params, args);
                            out.push(Argument {
                                name: p.name.clone(),
                                pos,
                                value: self.default_in(&p_ty, pos, budget, depth + 1)?,
                            });
                        }
                        Some(TaggedBody::Tuple(out))
                    }
                };
                value(ValueKind::Tagged {
                    name: variant.name.clone(),
                    body,
                })
            }
            _ => Err("Cannot work out the default value of the omitted argument: its type is not known".to_string()),
        }
    }
}

/// The type of a const: its declared type, or the type that was inferred for it, with the types that it leaves open
/// not known.
fn const_type(constant: &ConstDecl, env: &Env) -> Option<Ty> {
    match &constant.ty {
        Some(ty) => {
            let args: Vec<Ty> = constant.params.iter().map(|_| unknown()).collect();
            Some(instantiate(ty, &constant.params, &args))
        }
        None => {
            let scheme = env.inferred_type(&constant.name)?;
            let args: Vec<Ty> = (0..scheme.arity).map(|_| unknown()).collect();
            Some(fill(&scheme.ty, &args))
        }
    }
}

/// A type as it is written, with the names of enums shown without their projects, for a message.
pub fn type_name(ty: &Ty) -> String {
    match ty {
        Ty::Nat => "Nat".to_string(),
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Str => "String".to_string(),
        Ty::Array(element) => format!("[{}]", type_name(element)),
        Ty::Record(fields) if fields.is_empty() => "{}".to_string(),
        Ty::Record(fields) => {
            let fields: Vec<String> = fields.iter().map(|(n, t)| format!("{}: {}", n, type_name(t))).collect();
            format!("{{ {} }}", fields.join(", "))
        }
        Ty::Enum(name, args) => {
            let name = name.split('@').next().unwrap_or(name);
            if args.is_empty() {
                name.to_string()
            } else {
                let args: Vec<String> = args.iter().map(type_name).collect();
                format!("{}<{}>", name, args.join(", "))
            }
        }
        _ => "_".to_string(),
    }
}
