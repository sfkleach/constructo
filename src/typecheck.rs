//! The type-check of data values against data-type declarations.
//!
//! A value is checked against an expected type, which for a top-level value is a fresh type variable. Type
//! parameters of generic enums are instantiated with fresh variables as constructors are met, and the variables are
//! bound by the values that use them, so that one parameter has the same type throughout a top-level value.
//!
//! The errors are reported at the innermost term that fails. Checking a top-level value stops at its first error.

use crate::ast::{
    Argument, ConstDecl, EnumDecl, Field, Parameter, TaggedBody, Type, TypeParam, Value, ValueKind, Variant,
    VariantBody,
};
use crate::diagnostics::Pos;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// A failed type-check: where the offending term is, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError {
    pub pos: Pos,
    pub message: String,
}

type Result<T> = std::result::Result<T, TypeError>;

fn error<T>(pos: Pos, message: impl Into<String>) -> Result<T> {
    Err(TypeError {
        pos,
        message: message.into(),
    })
}

/// The declarations that are in scope for a values file.
pub struct Env<'a> {
    enums: HashMap<&'a str, &'a EnumDecl>,
    /// Every enum that is declared, whether or not a values file can see it yet. A type can refer to an enum that is
    /// declared later, and the default of that type has to be worked out from it.
    declared: HashMap<&'a str, &'a EnumDecl>,
    constructors: HashMap<&'a str, (&'a EnumDecl, &'a Variant)>,
    consts: HashMap<&'a str, &'a ConstDecl>,
    /// The types of the consts that were declared without one, once they have been inferred.
    inferred: HashMap<&'a str, Scheme>,
    /// The most nodes that the default value of an omitted argument may have.
    default_limit: usize,
}

/// The inferred type of a const: a type in which `Ty::Gen(i)` stands for the `i`th type that the value left open. Each
/// reference to the const replaces them with fresh variables.
#[derive(Debug, Clone)]
pub struct Scheme {
    ty: Ty,
    /// The number of types that the value left open.
    pub arity: usize,
    /// The type as it is written in a message.
    pub text: String,
}

impl<'a> Env<'a> {
    /// Builds the scope from declarations that have already passed validation, so that there are no duplicates.
    pub fn new(decls: impl IntoIterator<Item = &'a EnumDecl>, consts: impl IntoIterator<Item = &'a ConstDecl>) -> Self {
        let mut env = Env {
            enums: HashMap::new(),
            declared: HashMap::new(),
            constructors: HashMap::new(),
            consts: HashMap::new(),
            inferred: HashMap::new(),
            default_limit: DEFAULT_VALUE_LIMIT,
        };
        for constant in consts {
            env.consts.entry(constant.name.as_str()).or_insert(constant);
        }
        for decl in decls {
            env.enums.entry(decl.name.as_str()).or_insert(decl);
            env.declared.entry(decl.name.as_str()).or_insert(decl);
            for variant in &decl.variants {
                env.constructors.entry(variant.name.as_str()).or_insert((decl, variant));
            }
        }
        env
    }

    /// Gives this scope the inferred types, the default limit and the declared enums of another, which has the consts
    /// and enums that this one has and possibly more. The scope still only sees its own constructors.
    pub fn with_inferred_from(mut self, other: &Env<'a>) -> Self {
        self.inferred = other.inferred.clone();
        self.default_limit = other.default_limit;
        self.declared = other.declared.clone();
        self
    }

    /// Sets the most nodes that the default value of an omitted argument may have.
    pub fn with_default_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    /// The inferred type of a const, if it has been worked out.
    pub fn inferred_type(&self, name: &str) -> Option<&Scheme> {
        self.inferred.get(name)
    }
}

/// Checks one top-level value. With `widening`, a Nat fits where an Int or Float is expected and an Int where a Float
/// is expected; otherwise a number must have exactly the type it is expected to have.
pub fn check_value(env: &Env, widening: bool, value: &Value) -> Result<()> {
    let mut checker = Checker {
        env,
        widening,
        slots: Vec::new(),
        omitted: Vec::new(),
    };
    let top = checker.fresh();
    checker.check(value, &top)?;
    checker.check_omitted()
}

/// The default for the most nodes that the inferred type of a const may have. Types share their parts, so a short value
/// can have a type that is exponentially large when written out, and generalising it would take as long.
pub const DEFAULT_INFER_LIMIT: usize = 10_000;

/// The default for the most nodes that the default value of an omitted argument may have. A default is the first
/// constructor of an enum, filled in all the way down, so a few short declarations can make one that doubles in size
/// at each level.
pub const DEFAULT_VALUE_LIMIT: usize = 10_000;

/// Checks the consts of a run, each given with the index of its source, and returns the errors ordered by source and
/// position. A const without a declared type has its type inferred from its value, and the consts are taken in
/// dependency order for that, so that the type of one const is known when another refers to it. The consts that are
/// declared with a type are checked against it afterwards.
///
/// A const whose type cannot be inferred has an error reported once. The references to it are then left alone
/// instead of reporting the same failure again.
pub fn check_consts<'a>(
    env: &mut Env<'a>,
    widening: bool,
    infer_limit: usize,
    consts: &[(usize, &'a ConstDecl)],
) -> Vec<(usize, TypeError)> {
    let mut errors = Vec::new();
    for &(source, constant) in &inference_order(env, consts) {
        match infer_const(env, widening, infer_limit, constant) {
            Ok(scheme) => {
                env.inferred.insert(constant.name.as_str(), scheme);
            }
            Err(e) => errors.push((source, e)),
        }
    }
    for &(source, constant) in consts {
        if let Some(ty) = &constant.ty {
            if let Err(e) = check_declared_const(env, widening, constant, ty) {
                errors.push((source, e));
            }
        }
    }
    // The sort is stable, so errors at one position keep the order in which they were found.
    errors.sort_by_key(|(source, e)| (*source, e.pos));
    errors
}

/// The consts without a declared type, with each coming after the such consts that it refers to. The search is a
/// depth-first one with an explicit stack, because a long chain of consts must not overflow the call stack. A circular
/// definition has been reported already, and the order of a cycle does not matter here.
fn inference_order<'a>(env: &Env<'a>, consts: &[(usize, &'a ConstDecl)]) -> Vec<(usize, &'a ConstDecl)> {
    let undeclared: HashMap<&'a str, (usize, &'a ConstDecl)> = consts
        .iter()
        .filter(|(_, c)| c.ty.is_none() && env.consts.get(c.name.as_str()).is_some_and(|e| std::ptr::eq(*e, *c)))
        .map(|&(source, c)| (c.name.as_str(), (source, c)))
        .collect();
    let dependencies = |constant: &'a ConstDecl| -> Vec<&'a str> {
        let mut names = Vec::new();
        constant.value.names(&mut names);
        names
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| undeclared.contains_key(name))
            .collect()
    };
    let mut order = Vec::new();
    // A set, because a list would be searched in full for every const and every reference.
    let mut started: HashSet<&'a str> = HashSet::new();
    for &(_, constant) in consts {
        if !undeclared.contains_key(constant.name.as_str()) || !started.insert(constant.name.as_str()) {
            continue;
        }
        // Each entry is a const and the dependencies of it that are still to be visited.
        let mut stack = vec![(constant, dependencies(constant))];
        while let Some((current, pending)) = stack.last_mut() {
            match pending.pop() {
                Some(name) if started.insert(name) => {
                    let (_, next) = undeclared[name];
                    stack.push((next, dependencies(next)));
                }
                Some(_) => {}
                None => {
                    let (source, _) = undeclared[current.name.as_str()];
                    order.push((source, *current));
                    stack.pop();
                }
            }
        }
    }
    order
}

/// Checks the value of a const against the type that the const declares.
fn check_declared_const(env: &Env, widening: bool, constant: &ConstDecl, ty: &Type) -> Result<()> {
    let mut checker = Checker {
        env,
        widening,
        slots: Vec::new(),
        omitted: Vec::new(),
    };
    // The type parameters are unknown types here, because the value has to be valid whatever they turn out to be.
    let args: Vec<Ty> = constant.params.iter().map(|p| Ty::Param(p.name.clone())).collect();
    let declared = instantiate(ty, &constant.params, &args);
    checker.check(&constant.value, &declared)?;
    checker.check_omitted()
}

/// Works out the type of a const from its value. Whatever the value leaves open is generalised.
fn infer_const(env: &Env, widening: bool, infer_limit: usize, constant: &ConstDecl) -> Result<Scheme> {
    let mut checker = Checker {
        env,
        widening,
        slots: Vec::new(),
        omitted: Vec::new(),
    };
    let top = checker.fresh();
    checker.check(&constant.value, &top)?;
    checker.check_omitted()?;
    let mut open = Vec::new();
    let mut budget = infer_limit;
    let too_large = || TypeError {
        pos: constant.pos,
        message: format!(
            "The type of const {} has more than {} nodes, which is too large to infer; give it a type or raise \
             --infer-limit",
            constant.name, infer_limit
        ),
    };
    let ty = checker.generalise(&top, &mut open, &mut budget).ok_or_else(too_large)?;
    let text = checker.show(&ty);
    Ok(Scheme {
        ty,
        arity: open.len(),
        text,
    })
}

/// A type. The compound types share their parts, so that cloning a type, and so substituting it for a type parameter,
/// costs the same however large it is. Without that, a declaration such as `enum E<T> = Step(E<{ a: T, b: T }>)`
/// would double the size of the argument at each level of nesting in a value.
#[derive(Debug, Clone)]
enum Ty {
    Nat,
    Int,
    Float,
    Str,
    Array(Rc<Ty>),
    Enum(String, Rc<Vec<Ty>>),
    Record(Rc<Vec<(String, Ty)>>),
    Var(usize),
    /// A type parameter of the const being checked, which stands for some unknown type.
    Param(String),
    /// A type that the value of a const left open, in the inferred type of it.
    Gen(usize),
}

enum Slot {
    Free,
    /// A variable bound to a type. A variable that was bound by a number literal is flexible: a later, wider
    /// number can widen it, which is how the join of `[1, 2.5]` becomes `[Float]`.
    Bound(Ty, bool),
}

/// The name by which an open type is shown: `T`, `U`, `V` and so on.
pub fn gen_name(index: usize) -> String {
    match "TUVWXYZ".chars().nth(index) {
        Some(letter) => letter.to_string(),
        None => format!("T{}", index),
    }
}

/// Replaces the open types of an inferred type with the given types.
fn fill(ty: &Ty, args: &[Ty]) -> Ty {
    match ty {
        Ty::Gen(index) => args[*index].clone(),
        Ty::Array(element) => Ty::Array(Rc::new(fill(element, args))),
        Ty::Enum(name, enum_args) => Ty::Enum(name.clone(), Rc::new(enum_args.iter().map(|t| fill(t, args)).collect())),
        Ty::Record(fields) => Ty::Record(Rc::new(
            fields
                .iter()
                .map(|(name, field)| (name.clone(), fill(field, args)))
                .collect(),
        )),
        other => other.clone(),
    }
}

/// The longest description of a type that is put in a message, in characters.
/// Combines a tag, a name and the hashes of the parts into the hash of a type, using two differently seeded hashers.
fn combine(tag: u8, name: &str, parts: &[u128]) -> u128 {
    let mut hash = 0u128;
    for seed in [1u8, 2u8] {
        let mut hasher = DefaultHasher::new();
        (seed, tag, name, parts).hash(&mut hasher);
        hash = (hash << 64) | u128::from(hasher.finish());
    }
    hash
}

const SHOW_LIMIT: usize = 120;

/// The widening order of the numeric types.
fn rank(ty: &Ty) -> Option<u8> {
    match ty {
        Ty::Nat => Some(0),
        Ty::Int => Some(1),
        Ty::Float => Some(2),
        _ => None,
    }
}

fn numeric(rank: u8) -> Ty {
    match rank {
        0 => Ty::Nat,
        1 => Ty::Int,
        _ => Ty::Float,
    }
}

struct Checker<'a, 'e> {
    env: &'e Env<'a>,
    widening: bool,
    slots: Vec<Slot>,
    /// The arguments that were left out, each with the position of the constructor, the name of the parameter and its
    /// type. They are checked once the whole value has been, because the other arguments may fix the type of an omitted
    /// one, wherever in the value they are.
    omitted: Vec<(Pos, String, Ty)>,
}

impl Checker<'_, '_> {
    fn fresh(&mut self) -> Ty {
        self.slots.push(Slot::Free);
        Ty::Var(self.slots.len() - 1)
    }

    /// Resolves the outermost variable, if there is one, to the type it is bound to. A free variable stays a variable.
    fn shallow(&self, ty: &Ty) -> Ty {
        let mut ty = ty.clone();
        // A variable can be bound to another variable, as when a reference to a generic const is checked.
        while let Ty::Var(id) = &ty {
            match &self.slots[*id] {
                Slot::Bound(bound, _) => ty = bound.clone(),
                Slot::Free => break,
            }
        }
        ty
    }

    /// Resolves every variable in a type. A variable that is still free becomes an open type, numbered in the order in
    /// which the free variables are met and recorded in `open`. Returns `None` if the type has more than `budget` nodes.
    fn generalise(&self, ty: &Ty, open: &mut Vec<usize>, budget: &mut usize) -> Option<Ty> {
        *budget = budget.checked_sub(1)?;
        Some(match self.shallow(ty) {
            Ty::Var(id) => {
                let index = open.iter().position(|v| *v == id).unwrap_or_else(|| {
                    open.push(id);
                    open.len() - 1
                });
                Ty::Gen(index)
            }
            Ty::Array(element) => Ty::Array(Rc::new(self.generalise(&element, open, budget)?)),
            Ty::Enum(name, args) => {
                let args: Option<Vec<Ty>> = args.iter().map(|t| self.generalise(t, open, budget)).collect();
                Ty::Enum(name, Rc::new(args?))
            }
            Ty::Record(fields) => {
                let fields: Option<Vec<(String, Ty)>> = fields
                    .iter()
                    .map(|(name, field)| Some((name.clone(), self.generalise(field, open, budget)?)))
                    .collect();
                Ty::Record(Rc::new(fields?))
            }
            other => other,
        })
    }

    /// The variable, if it is bound by a number literal and so may still be widened.
    fn flexible_var(&self, ty: &Ty) -> Option<usize> {
        match ty {
            Ty::Var(id) if matches!(self.slots[*id], Slot::Bound(_, true)) => Some(*id),
            _ => None,
        }
    }

    /// Hashes a type by its structure, in 128 bits so that two different types will not be taken for one. The hash of
    /// each shared part is worked out once, which keeps the cost to the size of the type in memory and not to the size
    /// of it written out. `memo` is keyed by the address of the part, and keeps the part so that the address stays its own.
    fn type_hash(&self, ty: &Ty, memo: &mut HashMap<usize, (Ty, u128)>) -> u128 {
        enum Work {
            Enter(Ty),
            Leave(Ty),
        }
        let mut stack = vec![Work::Enter(ty.clone())];
        while let Some(work) = stack.pop() {
            match work {
                Work::Enter(part) => {
                    let part = self.shallow(&part);
                    if Self::address(&part).is_some_and(|address| memo.contains_key(&address)) {
                        continue;
                    }
                    stack.push(Work::Leave(part.clone()));
                    match &part {
                        Ty::Array(element) => stack.push(Work::Enter((**element).clone())),
                        Ty::Enum(_, args) => stack.extend(args.iter().map(|arg| Work::Enter(arg.clone()))),
                        Ty::Record(fields) => stack.extend(fields.iter().map(|(_, ty)| Work::Enter(ty.clone()))),
                        _ => {}
                    }
                }
                Work::Leave(part) => {
                    let Some(address) = Self::address(&part) else { continue };
                    let hash = match &part {
                        Ty::Array(element) => combine(b'[', "", &[self.part_hash(element, memo)]),
                        Ty::Enum(name, args) => {
                            let args: Vec<u128> = args.iter().map(|arg| self.part_hash(arg, memo)).collect();
                            combine(b'E', name, &args)
                        }
                        Ty::Record(fields) => {
                            let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
                            let types: Vec<u128> = fields.iter().map(|(_, ty)| self.part_hash(ty, memo)).collect();
                            combine(b'{', &names.join(","), &types)
                        }
                        _ => continue,
                    };
                    memo.insert(address, (part, hash));
                }
            }
        }
        self.part_hash(ty, memo)
    }

    /// The address of the shared part of a type, which is what a hash is remembered by.
    fn address(ty: &Ty) -> Option<usize> {
        match ty {
            Ty::Array(element) => Some(Rc::as_ptr(element) as usize),
            Ty::Enum(_, args) => Some(Rc::as_ptr(args) as usize),
            Ty::Record(fields) => Some(Rc::as_ptr(fields) as usize),
            _ => None,
        }
    }

    /// The hash of a part whose own parts have been hashed already.
    fn part_hash(&self, ty: &Ty, memo: &HashMap<usize, (Ty, u128)>) -> u128 {
        let ty = self.shallow(ty);
        match &ty {
            Ty::Nat => combine(b'N', "", &[]),
            Ty::Int => combine(b'I', "", &[]),
            Ty::Float => combine(b'F', "", &[]),
            Ty::Str => combine(b'S', "", &[]),
            Ty::Var(id) => combine(b'V', &id.to_string(), &[]),
            Ty::Param(name) => combine(b'P', name, &[]),
            Ty::Gen(id) => combine(b'G', &id.to_string(), &[]),
            Ty::Array(_) | Ty::Enum(..) | Ty::Record(_) => {
                Self::address(&ty).and_then(|a| memo.get(&a)).map_or(0, |m| m.1)
            }
        }
    }

    fn bind(&mut self, free: &Ty, ty: Ty, flexible: bool) {
        if let Ty::Var(id) = free {
            self.slots[*id] = Slot::Bound(ty, flexible);
        }
    }

    /// Describes a type for a message. The description is cut short, because sharing means that a type can be
    /// exponentially larger when written out than it is in memory.
    fn show(&self, ty: &Ty) -> String {
        let mut out = String::new();
        self.show_into(ty, &mut out);
        if out.chars().count() > SHOW_LIMIT {
            out = out.chars().take(SHOW_LIMIT).collect();
            out.push_str("...");
        }
        out
    }

    fn show_into(&self, ty: &Ty, out: &mut String) {
        // Writing stops once there is more than the limit, so that a huge type is never fully traversed.
        if out.len() > SHOW_LIMIT {
            return;
        }
        match self.shallow(ty) {
            Ty::Nat => out.push_str("Nat"),
            Ty::Int => out.push_str("Int"),
            Ty::Float => out.push_str("Float"),
            Ty::Str => out.push_str("String"),
            Ty::Array(elem) => {
                out.push('[');
                self.show_into(&elem, out);
                out.push(']');
            }
            Ty::Enum(name, args) => {
                out.push_str(&name);
                if !args.is_empty() {
                    out.push('<');
                    for (index, arg) in args.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        self.show_into(arg, out);
                    }
                    out.push('>');
                }
            }
            Ty::Record(fields) if fields.is_empty() => out.push_str("{}"),
            Ty::Record(fields) => {
                out.push_str("{ ");
                for (index, (name, field)) in fields.iter().enumerate() {
                    if index > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(name);
                    out.push_str(": ");
                    self.show_into(field, out);
                }
                out.push_str(" }");
            }
            Ty::Var(_) => out.push('_'),
            Ty::Param(name) => out.push_str(&name),
            Ty::Gen(index) => out.push_str(&gen_name(index)),
        }
    }

    fn mismatch<T>(&self, pos: Pos, expected: &Ty, found: &str) -> Result<T> {
        error(pos, format!("Expected {} but found {}", self.show(expected), found))
    }

    fn check(&mut self, value: &Value, original: &Ty) -> Result<()> {
        let expected = self.shallow(original);
        let pos = value.pos;
        match &value.kind {
            ValueKind::Nat(_) => self.check_number(pos, 0, original, &expected, "Nat"),
            ValueKind::Int(_) => self.check_number(pos, 1, original, &expected, "Int"),
            ValueKind::Float(_) => self.check_number(pos, 2, original, &expected, "Float"),
            ValueKind::Str(_) => match expected {
                Ty::Str => Ok(()),
                Ty::Var(_) => {
                    self.bind(&expected, Ty::Str, false);
                    Ok(())
                }
                _ => self.mismatch(pos, &expected, "String"),
            },
            ValueKind::Array(items) => {
                let elem = match &expected {
                    Ty::Array(elem) => (**elem).clone(),
                    Ty::Var(_) => {
                        let elem = self.fresh();
                        self.bind(&expected, Ty::Array(Rc::new(elem.clone())), false);
                        elem
                    }
                    _ => return self.mismatch(pos, &expected, "an array"),
                };
                items.iter().try_for_each(|item| self.check(item, &elem))
            }
            ValueKind::Record(members) => {
                let fields = match &expected {
                    Ty::Record(fields) => fields.clone(),
                    Ty::Var(_) => {
                        // The record decides its own type, so a duplicate field is caught by the check below.
                        let mut fields: Vec<(String, Ty)> = Vec::new();
                        for member in members {
                            if fields.iter().all(|(name, _)| *name != member.name) {
                                fields.push((member.name.clone(), self.fresh()));
                            }
                        }
                        let fields = Rc::new(fields);
                        self.bind(&expected, Ty::Record(Rc::clone(&fields)), false);
                        fields
                    }
                    _ => return self.mismatch(pos, &expected, "a record"),
                };
                self.check_members(pos, "record", members, &fields)
            }
            ValueKind::Tagged { name, body } => match self.env.constructors.get(name.as_str()) {
                Some(&(decl, variant)) => {
                    let found = format!("{} of type {}", name, decl.name);
                    self.check_variant(pos, decl, variant, body.as_ref(), &expected, &found)
                }
                None => match self.env.consts.get(name.as_str()) {
                    Some(constant) if body.is_some() => {
                        error(pos, format!("Const {} is a value and takes no fields", constant.name))
                    }
                    Some(constant) => self.check_const_ref(pos, constant, original),
                    None if name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_') => {
                        error(pos, format!("Unknown const {}", name))
                    }
                    None => error(pos, format!("Unknown constructor {}", name)),
                },
            },
        }
    }

    /// Checks a number literal of the given rank. The original expected type is needed as well as its resolution,
    /// because a flexible variable can be widened by a wider literal.
    fn check_number(&mut self, pos: Pos, lit: u8, original: &Ty, expected: &Ty, found: &str) -> Result<()> {
        match rank(expected) {
            Some(exp) if exp == lit || (self.widening && lit < exp) => Ok(()),
            Some(exp) if self.widening && lit > exp => match self.flexible_var(original) {
                Some(id) => {
                    self.slots[id] = Slot::Bound(numeric(lit), true);
                    Ok(())
                }
                None => self.mismatch(pos, expected, found),
            },
            Some(_) => self.mismatch(pos, expected, found),
            None => match expected {
                Ty::Var(_) => {
                    self.bind(expected, numeric(lit), true);
                    Ok(())
                }
                _ => self.mismatch(pos, expected, found),
            },
        }
    }

    /// Whether the variable occurs in the type. Binding a variable to a type that contains it would make the type
    /// infinite and make `shallow` loop. The parts of a type are shared, so each is visited once however many times it
    /// is used.
    fn occurs(&self, id: usize, ty: &Ty) -> bool {
        self.occurs_in(id, ty, &mut HashSet::new())
    }

    fn occurs_in(&self, id: usize, ty: &Ty, visited: &mut HashSet<*const ()>) -> bool {
        match self.shallow(ty) {
            Ty::Var(other) => other == id,
            Ty::Array(element) => visited.insert(Rc::as_ptr(&element).cast()) && self.occurs_in(id, &element, visited),
            Ty::Enum(_, args) => {
                visited.insert(Rc::as_ptr(&args).cast()) && args.iter().any(|arg| self.occurs_in(id, arg, visited))
            }
            Ty::Record(fields) => {
                visited.insert(Rc::as_ptr(&fields).cast())
                    && fields.iter().any(|(_, field)| self.occurs_in(id, field, visited))
            }
            Ty::Nat | Ty::Int | Ty::Float | Ty::Str | Ty::Param(_) | Ty::Gen(_) => false,
        }
    }

    fn infinite<T>(&self, pos: Pos, found: &str) -> Result<T> {
        error(
            pos,
            format!("Cannot use {}, which would make a type that contains itself", found),
        )
    }

    /// Checks a reference to a const, which has the type that the const declares or that was inferred for it.
    fn check_const_ref(&mut self, pos: Pos, constant: &ConstDecl, original: &Ty) -> Result<()> {
        // Each reference gets its own copy of the type parameters, so that uses of one const may differ.
        let declared = match &constant.ty {
            Some(ty) => {
                let args: Vec<Ty> = constant.params.iter().map(|_| self.fresh()).collect();
                instantiate(ty, &constant.params, &args)
            }
            None => match self.env.inferred.get(constant.name.as_str()) {
                Some(scheme) => {
                    let args: Vec<Ty> = (0..scheme.arity).map(|_| self.fresh()).collect();
                    fill(&scheme.ty, &args)
                }
                // The type could not be inferred, which has been reported, so there is nothing to check against.
                None => return Ok(()),
            },
        };
        let found = format!("const {} of type {}", constant.name, self.show(&declared));
        self.conforms(pos, &declared, original, &found)
    }

    /// Checks that a value of the type `declared` can be used where `original` is expected. This is the same relation
    /// as for a literal: numbers widen, and a variable that is still free takes the declared type. The declared type
    /// has the fresh variables of the const's type parameters in it, which are bound to whatever is expected.
    fn conforms(&mut self, pos: Pos, declared: &Ty, original: &Ty, found: &str) -> Result<()> {
        let declared = &self.shallow(declared);
        let expected = self.shallow(original);
        match (declared, &expected) {
            // Two references to one generic const can leave the same variable on both sides, as when the parameter
            // is used twice in the type. It is unified with itself already, and binding it would make a cycle.
            (Ty::Var(a), Ty::Var(b)) if a == b => Ok(()),
            (Ty::Var(id), _) => {
                if self.occurs(*id, &expected) {
                    return self.infinite(pos, found);
                }
                self.bind(declared, expected.clone(), false);
                Ok(())
            }
            (Ty::Nat | Ty::Int | Ty::Float, _) => {
                let lit = rank(declared).expect("a numeric type has a rank");
                self.check_number(pos, lit, original, &expected, found)
            }
            (_, Ty::Var(id)) => {
                if self.occurs(*id, declared) {
                    return self.infinite(pos, found);
                }
                self.bind(&expected, declared.clone(), false);
                Ok(())
            }
            (Ty::Str, Ty::Str) => Ok(()),
            (Ty::Param(from), Ty::Param(to)) if from == to => Ok(()),
            (Ty::Array(from), Ty::Array(to)) => self.conforms(pos, from, to, found),
            (Ty::Enum(from_name, from_args), Ty::Enum(to_name, to_args))
                if from_name == to_name && from_args.len() == to_args.len() =>
            {
                for (from, to) in from_args.iter().zip(to_args.iter()) {
                    self.conforms(pos, from, to, found)?;
                }
                Ok(())
            }
            (Ty::Record(from), Ty::Record(to))
                if from.len() == to.len() && from.iter().all(|(name, _)| to.iter().any(|(n, _)| n == name)) =>
            {
                for (name, from) in from.iter() {
                    let to = &to.iter().find(|(n, _)| n == name).expect("the field exists").1;
                    self.conforms(pos, from, to, found)?;
                }
                Ok(())
            }
            _ => self.mismatch(pos, &expected, found),
        }
    }

    /// Checks a constructor, with the body it was written with, against the expected type. The type arguments of
    /// the enum come from the expected type when it is known and are fresh variables otherwise.
    fn check_variant(
        &mut self,
        pos: Pos,
        decl: &EnumDecl,
        variant: &Variant,
        body: Option<&TaggedBody>,
        expected: &Ty,
        found: &str,
    ) -> Result<()> {
        let args = match expected {
            Ty::Enum(name, args) if *name == decl.name => args.clone(),
            Ty::Var(_) => {
                let args: Vec<Ty> = decl.params.iter().map(|_| self.fresh()).collect();
                let args = Rc::new(args);
                self.bind(expected, Ty::Enum(decl.name.clone(), Rc::clone(&args)), false);
                args
            }
            _ => return self.mismatch(pos, expected, found),
        };
        let name = &variant.name;
        match (&variant.body, body) {
            (VariantBody::Unit, None) => Ok(()),
            (VariantBody::Unit, Some(_)) => error(pos, format!("Constructor {} takes no fields", name)),
            (VariantBody::Record(fields), Some(TaggedBody::Record(members))) => {
                let fields = instantiate_fields(fields, &decl.params, &args);
                self.check_members(pos, name, members, &fields)
            }
            (VariantBody::Record(_), _) => error(pos, format!("Constructor {} takes a record", name)),
            (VariantBody::Tuple(parameters), Some(TaggedBody::Tuple(arguments))) => {
                let (bound, omitted) = bind_arguments(pos, name, parameters, arguments)?;
                for (argument, index) in arguments.iter().zip(bound) {
                    let ty = instantiate(&parameters[index].ty, &decl.params, &args);
                    let parameter = parameters[index].name.as_deref();
                    self.check(&argument.value, &ty)
                        .map_err(|e| name_parameter(e, argument.value.pos, parameter))?;
                }
                // The omitted arguments are only noted here: they are checked at the end of the value.
                for index in omitted {
                    let ty = instantiate(&parameters[index].ty, &decl.params, &args);
                    let parameter = parameters[index]
                        .name
                        .as_deref()
                        .expect("an omitted parameter is named");
                    self.omitted.push((pos, parameter.to_string(), ty));
                }
                Ok(())
            }
            (VariantBody::Tuple(_), _) => error(pos, format!("Constructor {} takes a tuple", name)),
        }
    }

    /// Checks that the members of a record value are exactly the fields, in any order, and then checks each member.
    fn check_members(
        &mut self,
        pos: Pos,
        what: &str,
        members: &[crate::ast::Member],
        fields: &[(String, Ty)],
    ) -> Result<()> {
        for (index, member) in members.iter().enumerate() {
            if members[..index].iter().any(|m| m.name == member.name) {
                return error(member.pos, format!("Duplicate field {}", member.name));
            }
            if fields.iter().all(|(name, _)| *name != member.name) {
                return error(member.pos, format!("Unknown field {} in {}", member.name, what));
            }
        }
        for (name, _) in fields {
            if members.iter().all(|m| m.name != *name) {
                return error(pos, format!("Missing field {} in {}", name, what));
            }
        }
        for member in members {
            let ty = &fields
                .iter()
                .find(|(name, _)| *name == member.name)
                .expect("the field exists")
                .1;
            self.check(&member.value, ty)?;
        }
        Ok(())
    }

    /// Checks the omitted arguments that have been noted, in the order in which they were met.
    fn check_omitted(&mut self) -> Result<()> {
        for (pos, parameter, ty) in std::mem::take(&mut self.omitted) {
            self.check_default(pos, &parameter, &ty)?;
        }
        Ok(())
    }

    /// Checks that an omitted argument of the given type has a default value that can be used. The default is the
    /// zero of a primitive type, the empty array, the record of the defaults of the fields, and the first constructor
    /// of an enum with the defaults of its fields. Nothing is built: the nodes are only counted against the limit.
    fn check_default(&self, pos: Pos, parameter: &str, ty: &Ty) -> Result<()> {
        let limit = self.env.default_limit;
        let mut remaining = limit;
        match self.default_nodes(ty, &mut remaining) {
            Ok(()) => Ok(()),
            Err(NoDefault::Unknown) => {
                let message = format!(
                    "Cannot work out the default value of argument {}: its type is not known",
                    parameter
                );
                error(pos, message)
            }
            Err(NoDefault::Recursive(found)) => error(pos, format!("Type {} has no default value", self.show(&found))),
            Err(NoDefault::TooLarge) => {
                let message = format!(
                    "The default value of {} has more than {} nodes, which is too large; give argument {} or raise \
                     --default-limit",
                    self.show(ty),
                    limit,
                    parameter
                );
                error(pos, message)
            }
        }
    }

    /// Counts the nodes of the default value of a type, taking them off `remaining`. The traversal is depth-first and
    /// uses an explicit stack, because a chain of declarations that each start with the next one is only as deep as
    /// the number of declarations, which the parser's nesting limit does not bound.
    fn default_nodes(&self, ty: &Ty, remaining: &mut usize) -> std::result::Result<(), NoDefault> {
        enum Step {
            Visit(Ty),
            /// The end of the expansion of an enum, which takes the type off the path.
            Leave(u128),
        }
        // The hashes of the types that are being expanded, to catch a type that contains itself. The type arguments
        // are part of it, so `Box<Box<Nat>>` containing `Box<Nat>` is not a cycle, while a type whose arguments keep
        // changing is stopped by the limit.
        let mut path: HashSet<u128> = HashSet::new();
        let mut memo: HashMap<usize, (Ty, u128)> = HashMap::new();
        let mut stack = vec![Step::Visit(ty.clone())];
        while let Some(step) = stack.pop() {
            let ty = match step {
                Step::Leave(key) => {
                    path.remove(&key);
                    continue;
                }
                Step::Visit(ty) => self.shallow(&ty),
            };
            if matches!(ty, Ty::Var(_) | Ty::Param(_) | Ty::Gen(_)) {
                return Err(NoDefault::Unknown);
            }
            if *remaining == 0 {
                return Err(NoDefault::TooLarge);
            }
            *remaining -= 1;
            // The parts are pushed in reverse, so that they are taken in order.
            match &ty {
                Ty::Record(fields) => {
                    stack.extend(fields.iter().rev().map(|(_, field)| Step::Visit(field.clone())));
                }
                Ty::Enum(name, args) => {
                    let key = self.type_hash(&ty, &mut memo);
                    if !path.insert(key) {
                        return Err(NoDefault::Recursive(ty.clone()));
                    }
                    // Validation has checked that every type that is used is declared, so this is only a defensive check.
                    let Some(&decl) = self.env.declared.get(name.as_str()) else {
                        return Err(NoDefault::Unknown);
                    };
                    // Validation does not insist that an enum has a constructor, and one that has none has no default.
                    let Some(first) = decl.variants.first() else {
                        return Err(NoDefault::Recursive(ty.clone()));
                    };
                    stack.push(Step::Leave(key));
                    let types: Vec<&Type> = match &first.body {
                        VariantBody::Unit => Vec::new(),
                        VariantBody::Record(fields) => fields.iter().map(|f| &f.ty).collect(),
                        VariantBody::Tuple(parameters) => parameters.iter().map(|p| &p.ty).collect(),
                    };
                    for declared in types.into_iter().rev() {
                        stack.push(Step::Visit(instantiate(declared, &decl.params, args)));
                    }
                }
                // The primitive types and the arrays have a default with no parts.
                _ => {}
            }
        }
        Ok(())
    }
}

/// Why a type has no default value that can be used.
enum NoDefault {
    /// The type was met again while working out its own default, so the expansion would never end.
    Recursive(Ty),
    /// The expansion has more nodes than the limit.
    TooLarge,
    /// The type, or a part of it, is not known.
    Unknown,
}

/// Adds the name of the parameter to a mismatch that is for the argument itself, giving "Expected Nat for argument x
/// but found String". A mismatch somewhere inside the argument is left alone, as it is not about the parameter.
fn name_parameter(mut error: TypeError, argument_pos: Pos, parameter: Option<&str>) -> TypeError {
    if let (Some(name), true) = (parameter, error.pos == argument_pos) {
        if let Some(rest) = error.message.strip_prefix("Expected ") {
            if let Some((expected, found)) = rest.split_once(" but found ") {
                error.message = format!("Expected {} for argument {} but found {}", expected, name, found);
            }
        }
    }
    error
}

/// Matches the arguments of a tuple constructor to its parameters, returning the index of the parameter that each
/// argument is for, and the indexes of the named parameters that no argument is for. A parameter can be bound at most
/// once.
fn bind_arguments(
    pos: Pos,
    constructor: &str,
    parameters: &[Parameter],
    arguments: &[Argument],
) -> Result<(Vec<usize>, Vec<usize>)> {
    let mut bound: Vec<Option<usize>> = vec![None; parameters.len()];
    let mut targets = Vec::with_capacity(arguments.len());
    for (index, argument) in arguments.iter().enumerate() {
        let target = match &argument.name {
            None if index < parameters.len() => index,
            None => {
                let message = format!(
                    "Constructor {} takes {} value(s) but has {}",
                    constructor,
                    parameters.len(),
                    arguments.len()
                );
                return error(argument.pos, message);
            }
            Some(name) => match parameters.iter().position(|p| p.name.as_deref() == Some(name)) {
                Some(target) => target,
                None => return error(argument.pos, format!("Unknown argument {} in {}", name, constructor)),
            },
        };
        if bound[target].is_some() {
            let name = parameters[target].name.as_deref();
            let message = match name {
                Some(name) => format!("Argument {} is given more than once in {}", name, constructor),
                None => format!("Argument {} is given more than once in {}", target + 1, constructor),
            };
            return error(argument.pos, message);
        }
        bound[target] = Some(index);
        targets.push(target);
    }
    // An unnamed parameter can only be bound by position, so leaving one out is a count error. A named parameter that
    // is left out takes the default value of its type.
    let unbound: Vec<usize> = (0..parameters.len()).filter(|&i| bound[i].is_none()).collect();
    if unbound.iter().any(|&i| parameters[i].name.is_none()) {
        let message = format!(
            "Constructor {} takes {} value(s) but has {}",
            constructor,
            parameters.len(),
            arguments.len()
        );
        return error(pos, message);
    }
    Ok((targets, unbound))
}

fn instantiate_fields(fields: &[Field], params: &[TypeParam], args: &[Ty]) -> Vec<(String, Ty)> {
    fields
        .iter()
        .map(|f| (f.name.clone(), instantiate(&f.ty, params, args)))
        .collect()
}

/// Turns a declared type into a `Ty`, replacing the type parameters with the given arguments.
fn instantiate(ty: &Type, params: &[TypeParam], args: &[Ty]) -> Ty {
    match ty {
        Type::Ref {
            name, args: type_args, ..
        } => {
            if let Some(index) = params.iter().position(|p| p.name == *name) {
                return args[index].clone();
            }
            match name.as_str() {
                "Nat" => Ty::Nat,
                "Int" => Ty::Int,
                "Float" => Ty::Float,
                "String" => Ty::Str,
                _ => Ty::Enum(
                    name.clone(),
                    Rc::new(type_args.iter().map(|t| instantiate(t, params, args)).collect()),
                ),
            }
        }
        Type::Array(elem) => Ty::Array(Rc::new(instantiate(elem, params, args))),
        Type::Record(fields) => Ty::Record(Rc::new(instantiate_fields(fields, params, args))),
    }
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse, parse_values};

    /// Checks each value of `values` against `types`, and returns the error of each value as `line:col message`.
    fn errors(types: &str, values: &str, widening: bool) -> Vec<String> {
        let file = parse(&format!("{} {}", PRELUDE_TYPES, types)).unwrap();
        let env = Env::new(&file.enums, &file.consts);
        parse_values(values)
            .unwrap()
            .iter()
            .filter_map(|v| check_value(&env, widening, v).err())
            .map(|e| format!("{}:{} {}", e.pos.line, e.pos.col, e.message))
            .collect()
    }

    const PRELUDE_TYPES: &str = "enum Option<T> = None, Some(T); const null<T>: Option<T> = None;";

    const SHAPES: &str = "enum Shape = Circle { r: Float }, Box { w: Float, h: Float }, Dot, Pair(Nat, Int), Unit();";

    #[test]
    fn accepts_well_typed_constructors() {
        let values = "Circle { r: 1.5 }; Box { h: 1.0, w: 2.0 }; Dot; Pair(1, +2); Unit()";
        assert_eq!(errors(SHAPES, values, true), Vec::<String>::new());
    }

    #[test]
    fn the_shape_must_match_the_declaration() {
        let cases = [
            ("Dot()", "1:1 Constructor Dot takes no fields"),
            ("Unit", "1:1 Constructor Unit takes a tuple"),
            ("Circle", "1:1 Constructor Circle takes a record"),
            ("Circle(1.0)", "1:1 Constructor Circle takes a record"),
            ("Pair { a: 1 }", "1:1 Constructor Pair takes a tuple"),
            ("Pair(1)", "1:1 Constructor Pair takes 2 value(s) but has 1"),
            ("Nope", "1:1 Unknown constructor Nope"),
        ];
        for (value, expected) in cases {
            assert_eq!(errors(SHAPES, value, true), vec![expected.to_string()], "{}", value);
        }
    }

    const IMAGES: &str = "enum Image = Image(String, hOffset: Nat, vOffset: Nat), Span(start: Nat, end: Nat);";

    #[test]
    fn arguments_are_bound_by_position_or_by_name() {
        let values = "Image(\"a\", 1, 2); Image(\"a\", vOffset: 2, hOffset: 1); Image(\"a\", 1, vOffset: 2); \
                      Span(end: 2, start: 1); Span(1, end: 2)";
        assert_eq!(errors(IMAGES, values, true), Vec::<String>::new());
    }

    #[test]
    fn arguments_that_cannot_be_bound_are_errors() {
        let cases = [
            (
                "Span(start: 1, start: 2)",
                "1:16 Argument start is given more than once in Span",
            ),
            (
                "Span(1, 2, start: 3)",
                "1:12 Argument start is given more than once in Span",
            ),
            ("Span(1, 2, 3)", "1:12 Constructor Span takes 2 value(s) but has 3"),
            ("Span(start: 1, stop: 2)", "1:16 Unknown argument stop in Span"),
            (
                "Image(hOffset: 1, vOffset: 2)",
                "1:1 Constructor Image takes 3 value(s) but has 2",
            ),
            (
                "Image(src: \"a\", hOffset: 1, vOffset: 2)",
                "1:7 Unknown argument src in Image",
            ),
            (
                "Span(start: 1, end: \"x\")",
                "1:21 Expected Nat for argument end but found String",
            ),
            ("Span(1, \"x\")", "1:9 Expected Nat for argument end but found String"),
            ("Image([1], 1, 2)", "1:7 Expected String but found an array"),
        ];
        for (value, expected) in cases {
            assert_eq!(errors(IMAGES, value, true), vec![expected.to_string()], "{}", value);
        }
    }

    #[test]
    fn a_deeply_nested_finite_type_has_a_default() {
        // The text of the type is cut short, so the outer type and the types inside it start with the same text.
        let ty = format!("{}Nat{}", "Box<".repeat(40), ">".repeat(40));
        let types = format!("enum Box<T> = Box(T); enum Holder = Holder(x: {});", ty);
        assert_eq!(errors(&types, "Holder()", true), Vec::<String>::new());
    }

    #[test]
    fn named_arguments_may_be_omitted() {
        assert_eq!(
            errors(IMAGES, "Span(); Span(1); Span(end: 2); Image(\"a\", vOffset: 2)", true),
            Vec::<String>::new()
        );
        // An unnamed parameter cannot be left out.
        assert_eq!(
            errors(IMAGES, "Image()", true),
            vec!["1:1 Constructor Image takes 3 value(s) but has 0"]
        );
    }

    const DEFAULTS: &str = "enum Bool = False, True; enum Option<T> = None, Some(T); \
                            enum Mixed = Mixed(a: Nat, b: Int, c: Float, d: String, e: [Nat], f: { x: Bool, y: Option<Nat> }); \
                            enum Box<T> = Box(label: String, content: T); \
                            enum Holder = Holder(b: Box<Nat>);";

    #[test]
    fn every_type_has_a_default() {
        assert_eq!(
            errors(DEFAULTS, "Mixed(); Mixed(1, e: [2]); Holder(); Box(\"x\", 1)", true),
            Vec::<String>::new()
        );
        assert_eq!(errors(DEFAULTS, "Holder(Box(\"x\"))", true), Vec::<String>::new());
    }

    #[test]
    fn the_order_of_the_arguments_does_not_matter_to_a_default() {
        let types = "enum Cell<T> = Cell(content: T); enum Pair<T> = Pair(cell: Cell<T>, value: T);";
        let values = "Pair(cell: Cell(), value: 1); Pair(value: 1, cell: Cell()); Pair(Cell(), 1)";
        assert_eq!(errors(types, values, true), Vec::<String>::new());
        // Nothing fixes the type here, wherever it is looked at from.
        assert_eq!(
            errors(types, "Pair(cell: Cell(), value: Cell())", true),
            vec!["1:12 Cannot work out the default value of argument content: its type is not known"]
        );
    }

    #[test]
    fn a_default_needs_its_type_to_be_known() {
        assert_eq!(
            errors(DEFAULTS, "Box(\"x\")", true),
            vec!["1:1 Cannot work out the default value of argument content: its type is not known"]
        );
    }

    #[test]
    fn a_type_inside_itself_with_other_arguments_is_not_recursive() {
        let types =
            "enum Box<T> = Box(T); enum Holder = Holder(x: Box<Box<Nat>>); enum Deep = Deep(x: Box<Box<Box<Nat>>>);";
        assert_eq!(errors(types, "Holder(); Deep()", true), Vec::<String>::new());
        // The arguments of this one change at every level, so it never ends and the limit stops it.
        let nest = "enum Nest<T> = Nest(next: Nest<[T]>); enum Holder = Holder(x: Nest<Nat>);";
        let found = errors(nest, "Holder()", true);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(
            found[0].contains("is too large") || found[0].contains("has no default value"),
            "{:?}",
            found
        );
    }

    #[test]
    fn a_recursive_default_is_an_error_at_the_omission() {
        let list = "enum List<T> = Cons { head: T, tail: List<T> }, Nil; enum Holder = Holder(items: List<Nat>); ";
        assert_eq!(
            errors(list, "Holder(Nil); Holder()", true),
            vec!["1:14 Type List<Nat> has no default value"]
        );
        let nil_first = "enum List<T> = Nil, Cons { head: T, tail: List<T> }; enum Holder = Holder(items: List<Nat>); ";
        assert_eq!(errors(nil_first, "Holder()", true), Vec::<String>::new());
        let mutual = "enum A = A(b: B); enum B = B(a: A); enum Holder = Holder(x: A);";
        assert_eq!(
            errors(mutual, "Holder()", true),
            vec!["1:1 Type A has no default value"]
        );
    }

    #[test]
    fn a_long_chain_of_first_constructors_does_not_overflow_the_stack() {
        // Each declaration is shallow, but the default of the last one goes through all of them.
        let mut types = String::from("enum E0 = E0; ");
        for n in 1..=50_000 {
            types.push_str(&format!("enum E{n} = E{n}(a: E{p}); ", n = n, p = n - 1));
        }
        types.push_str("enum Holder = Holder(x: E50000);");
        let file = parse(&types).unwrap();
        let check = |limit: usize| {
            let env = Env::new(&file.enums, &file.consts).with_default_limit(limit);
            check_value(&env, true, &parse_values("Holder()").unwrap()[0])
                .err()
                .map(|e| e.message)
        };
        assert_eq!(check(1_000_000), None);
        assert!(check(DEFAULT_VALUE_LIMIT).unwrap().contains("is too large"));
    }

    #[test]
    fn a_huge_default_is_refused_but_a_small_one_is_not() {
        let mut types = String::from("enum E0 = E0; ");
        for n in 1..=15 {
            types.push_str(&format!("enum E{n} = E{n}(a: E{p}, b: E{p}); ", n = n, p = n - 1));
        }
        types.push_str("enum Holder = Holder(x: E15, y: E3);");
        let file = parse(&types).unwrap();
        let check = |limit: usize| {
            let env = Env::new(&file.enums, &file.consts).with_default_limit(limit);
            check_value(&env, true, &parse_values("Holder()").unwrap()[0])
                .err()
                .map(|e| e.message)
        };
        assert_eq!(
            check(DEFAULT_VALUE_LIMIT),
            Some(
                "The default value of E15 has more than 10000 nodes, which is too large; give argument x or raise \
                 --default-limit"
                    .to_string()
            )
        );
        // The limit applies to each omitted argument, and E15 has 2^16 - 1 nodes.
        assert_eq!(check(65_535), None);
        assert!(check(65_534).is_some());
    }

    #[test]
    fn records_must_have_exactly_the_declared_fields() {
        let cases = [
            ("Box { w: 1.0 }", "1:1 Missing field h in Box"),
            ("Box { w: 1.0, h: 1.0, d: 1.0 }", "1:23 Unknown field d in Box"),
            ("Box { w: 1.0, w: 2.0, h: 1.0 }", "1:15 Duplicate field w"),
        ];
        for (value, expected) in cases {
            assert_eq!(errors(SHAPES, value, true), vec![expected.to_string()], "{}", value);
        }
    }

    #[test]
    fn reports_the_innermost_failing_term() {
        assert_eq!(
            errors(SHAPES, "Box { w: 1.0, h: \"tall\" }", true),
            vec!["1:18 Expected Float but found String"]
        );
        assert_eq!(
            errors("enum A = A(B); enum B = B(Nat);", "A(B(\"x\"))", true),
            vec!["1:5 Expected Nat but found String"]
        );
        assert_eq!(
            errors("enum A = A(B); enum B = B(Nat);", "A(Dot)", true),
            vec!["1:3 Unknown constructor Dot"]
        );
        assert_eq!(
            errors("enum A = A(B); enum B = B(Nat); enum C = C;", "A(C)", true),
            vec!["1:3 Expected B but found C of type C"]
        );
    }

    #[test]
    fn numbers_widen_unless_disabled() {
        let types = "enum N = N { n: Nat, i: Int, f: Float };";
        assert!(errors(
            types,
            "N { n: 1, i: 1, f: 1 }; N { n: 1, i: +1, f: -1 }; N { n: 1, i: -1, f: 1.5 }",
            true
        )
        .is_empty());
        assert_eq!(
            errors(types, "N { n: +1, i: 1, f: 1 }", true),
            vec!["1:8 Expected Nat but found Int"]
        );
        assert_eq!(
            errors(types, "N { n: 1, i: 1.5, f: 1 }", true),
            vec!["1:14 Expected Int but found Float"]
        );
        assert_eq!(
            errors(types, "N { n: 1, i: 1, f: 1.5 }", false),
            vec!["1:14 Expected Int but found Nat"]
        );
        assert!(errors(types, "N { n: 1, i: +1, f: 1.5 }", false).is_empty());
    }

    #[test]
    fn untagged_values_are_checked_for_consistency() {
        assert!(errors(
            "",
            "3.0; [1, 2, 3]; { x: 1, y: [] }; \"s\"; [[]]; [{ a: 1 }, { a: 2 }]",
            true
        )
        .is_empty());
        assert_eq!(
            errors("", "[1, \"a\"]", true),
            vec!["1:5 Expected Nat but found String"]
        );
        assert_eq!(
            errors("", "[{ a: 1 }, { b: 1 }]", true),
            vec!["1:14 Unknown field b in record"]
        );
        assert_eq!(errors("", "{ a: 1, a: 2 }", true), vec!["1:9 Duplicate field a"]);
    }

    #[test]
    fn a_join_of_numbers_is_the_wider_type() {
        assert!(errors("", "[1, 2.5]; [1, +2]; [1, +2, 3.5, 4]", true).is_empty());
        assert_eq!(errors("", "[1, 2.5]", false), vec!["1:5 Expected Nat but found Float"]);
        assert_eq!(errors("", "[+1, 2.5, 3]", true), Vec::<String>::new());
    }

    #[test]
    fn a_type_parameter_is_the_same_throughout_a_value() {
        let types = "enum List<T> = Nil, Cons { head: T, tail: List<T> };";
        assert!(errors(types, "Cons { head: 1, tail: Cons { head: 2, tail: Nil } }", true).is_empty());
        assert!(errors(types, "Cons { head: 1, tail: Cons { head: 2.5, tail: Nil } }", true).is_empty());
        assert_eq!(
            errors(types, "Cons { head: 1, tail: Cons { head: \"a\", tail: Nil } }", true),
            vec!["1:36 Expected Nat but found String"]
        );
    }

    #[test]
    fn an_expected_type_is_not_widened_by_a_join() {
        let types = "enum Box = Box(Option<Int>);";
        assert!(errors(types, "Box(Some(1)); Box(Some(+1)); Box(None)", true).is_empty());
        assert_eq!(
            errors(types, "Box(Some(1.5))", true),
            vec!["1:10 Expected Int but found Float"]
        );
    }

    #[test]
    fn null_is_a_generic_const_of_option() {
        let types = "enum Config = Config { owner: Option<String> };";
        assert!(errors(
            types,
            "Config { owner: null }; Config { owner: Some(\"me\") }; null; [null, Some(1)]",
            true
        )
        .is_empty());
        assert_eq!(
            errors(types, "Config { owner: Some(null) }", true),
            vec!["1:22 Expected String but found const null of type Option<_>"]
        );
        assert_eq!(
            errors("enum A = A(Nat);", "A(null)", true),
            vec!["1:3 Expected Nat but found const null of type Option<_>"]
        );
    }

    #[test]
    fn each_reference_to_a_generic_const_has_its_own_type() {
        let types = "enum P = P(Option<Nat>, Option<String>);";
        assert!(errors(types, "P(null, null); P(null, Some(\"s\"))", true).is_empty());
        // Within one value the uses of a type variable must still agree.
        assert_eq!(
            errors("", "[Some(1), null, Some(\"s\")]", true),
            vec!["1:22 Expected Nat but found String"]
        );
    }

    #[test]
    fn null_needs_the_const_in_scope() {
        let file = parse("enum A = A;").unwrap();
        let env = Env::new(&file.enums, &file.consts);
        let value = &parse_values("null").unwrap()[0];
        assert_eq!(
            check_value(&env, true, value).unwrap_err().message,
            "Unknown const null"
        );
    }

    /// Checks the consts of a types file, and returns the error of each as `line:col message`.
    fn const_errors(text: &str) -> Vec<String> {
        let file = parse(&format!("{}\n{}", PRELUDE_TYPES, text)).unwrap();
        let mut env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        check_consts(&mut env, true, DEFAULT_INFER_LIMIT, &consts)
            .into_iter()
            .map(|(_, e)| format!("{}:{} {}", e.pos.line, e.pos.col, e.message))
            .collect()
    }

    /// Infers the consts of a types file and returns each as `name: type`, for those whose type could be inferred.
    fn inferred(text: &str, widening: bool) -> Vec<String> {
        let file = parse(&format!("{}\n{}", PRELUDE_TYPES, text)).unwrap();
        let mut env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        let errors = check_consts(&mut env, widening, DEFAULT_INFER_LIMIT, &consts);
        assert!(errors.is_empty(), "{:?}", errors);
        file.consts
            .iter()
            .filter(|c| c.ty.is_none())
            .map(|c| {
                let scheme = env.inferred_type(&c.name).expect("the type was inferred");
                format!("{}/{}: {}", c.name, scheme.arity, scheme.text)
            })
            .collect()
    }

    #[test]
    fn the_value_of_a_generic_const_must_suit_every_type() {
        assert!(const_errors("const none<T>: Option<T> = None; const nil<T>: [T] = [];").is_empty());
        assert!(const_errors("const pair<A, B>: { a: Option<A>, b: [B] } = { a: null, b: [] };").is_empty());
        // The parameter is an unknown type, so no particular value can be of it.
        let errors = const_errors("const bad<T>: Option<T> = Some(1);");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].ends_with("Expected T but found Nat"), "{:?}", errors);
        assert_eq!(const_errors("const bad<T>: T = \"s\";").len(), 1);
        assert_eq!(const_errors("const bad<T>: [T] = [1];").len(), 1);
    }

    #[test]
    fn a_generic_const_may_be_defined_by_another() {
        assert!(const_errors("const a<T>: Option<T> = null; const b<U>: Option<U> = a;").is_empty());
        assert_eq!(
            const_errors("const a<T>: Option<T> = null; const b<U>: Option<Nat> = a;").len(),
            0
        );
        assert_eq!(
            const_errors("const a<T>: [T] = []; const b<U>: Option<U> = a;").len(),
            1
        );
        // A parameter of the second const is not the same type as one chosen for the first.
        assert_eq!(
            const_errors("const a: Option<Nat> = null; const b<U>: Option<U> = a;").len(),
            1
        );
    }

    #[test]
    fn arrays_and_anonymous_records_in_types() {
        let types = "enum Cfg = Cfg { tags: [String], limits: { cpu: Nat, mem: Nat } };";
        assert!(errors(types, "Cfg { tags: [\"a\"], limits: { mem: 1, cpu: 2 } }", true).is_empty());
        assert_eq!(
            errors(types, "Cfg { tags: [1], limits: { mem: 1, cpu: 2 } }", true),
            vec!["1:14 Expected String but found Nat"]
        );
        assert_eq!(
            errors(types, "Cfg { tags: [], limits: { mem: 1 } }", true),
            vec!["1:25 Missing field cpu in record"]
        );
        assert_eq!(
            errors(types, "Cfg { tags: {}, limits: { mem: 1, cpu: 2 } }", true),
            vec!["1:13 Expected [String] but found a record"]
        );
    }

    // A type argument that grows at every level of a value, as in `E<{ a: T, b: T }>`, is exponentially large when
    // written out. These tests would exhaust memory or time if substitution copied the type or if messages listed it.
    const GROWING: &str = "enum E<T> = Stop, Step(E<{ a: T, b: T }>), Value(T);";

    fn nested(depth: usize, innermost: &str) -> String {
        format!("{}{}{}", "Step(".repeat(depth), innermost, ")".repeat(depth))
    }

    #[test]
    fn a_type_argument_that_doubles_at_each_level_is_cheap() {
        assert!(errors(GROWING, &nested(150, "Stop"), true).is_empty());
    }

    #[test]
    fn a_message_about_a_huge_type_is_cut_short() {
        let found = errors(GROWING, &nested(150, "Value(\"x\")"), true);
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("Expected { a: { a: { a:"), "{}", found[0]);
        assert!(found[0].contains("... but found String"), "{}", found[0]);
        assert!(found[0].len() < 250, "{}", found[0].len());
    }

    #[test]
    fn a_short_type_is_described_in_full() {
        assert_eq!(
            errors(
                "enum P<T> = P { xs: [T], pair: { a: T, b: Option<T> } };",
                "P { xs: [1], pair: 3 }",
                true
            ),
            vec!["1:20 Expected { a: Nat, b: Option<Nat> } but found Nat"]
        );
    }

    #[test]
    fn a_record_type_is_closed_and_ignores_the_order_of_its_fields() {
        let types = "const x: { a: Nat, b: String } = { b: \"s\", a: 1 };";
        assert!(const_errors(types).is_empty());
        assert_eq!(
            const_errors("const x: { a: Nat } = { a: 1, b: 2 };"),
            vec!["2:31 Unknown field b in record"]
        );
        assert_eq!(
            const_errors("const x: { a: Nat, b: Nat } = { a: 1 };"),
            vec!["2:31 Missing field b in record"]
        );
        assert_eq!(
            const_errors("const x: { a: Nat } = { a: 1, a: 2 };"),
            vec!["2:31 Duplicate field a"]
        );
    }

    #[test]
    fn record_types_nest_and_hold_arrays_and_generics() {
        assert!(
            const_errors("const x: { a: { b: [Nat] }, c: [{ d: Float }] } = { a: { b: [1, 2] }, c: [{ d: 1 }] };")
                .is_empty()
        );
        assert!(const_errors("const x<T>: { v: Option<T>, w: [T] } = { v: null, w: [] };").is_empty());
        assert_eq!(
            const_errors("const x: { a: { b: Nat } } = { a: { b: \"s\" } };"),
            vec!["2:40 Expected Nat but found String"]
        );
    }

    #[test]
    fn the_empty_record_is_a_type_and_a_value() {
        assert!(const_errors("const x: {} = {}; const y: [{}] = [{}, {}];").is_empty());
        assert_eq!(
            const_errors("const x: {} = { a: 1 };"),
            vec!["2:17 Unknown field a in record"]
        );
        assert_eq!(
            const_errors("const x: { a: Nat } = {};"),
            vec!["2:23 Missing field a in record"]
        );
    }

    #[test]
    fn tagged_and_untagged_records_never_convert() {
        let types = "enum E = Foo { a: Nat }, Pong {}, Bare; ";
        assert_eq!(
            const_errors(&format!("{}const x: {{ a: Nat }} = Foo {{ a: 1 }};", types)),
            vec!["2:63 Expected { a: Nat } but found Foo of type E"]
        );
        assert_eq!(
            const_errors(&format!("{}const x: E = {{ a: 1 }};", types)),
            vec!["2:54 Expected E but found a record"]
        );
        // The empty record is not a bare constructor or an empty tagged record either.
        assert_eq!(
            const_errors(&format!("{}const x: {{}} = Pong {{}};", types)),
            vec!["2:55 Expected {} but found Pong of type E"]
        );
        assert_eq!(
            const_errors(&format!("{}const x: {{}} = Bare;", types)),
            vec!["2:55 Expected {} but found Bare of type E"]
        );
    }

    #[test]
    fn the_record_types_of_a_const_reference_are_compared_by_field() {
        let types = "const a: { x: Nat, y: Float } = { x: 1, y: 2.5 }; ";
        let ok = |def: &str| const_errors(&format!("{}{}", types, def)).is_empty();
        assert!(ok("const b: { y: Float, x: Nat } = a;"));
        // Numbers widen field by field.
        assert!(ok("const b: { x: Int, y: Float } = a;"));
        assert!(!ok("const b: { x: Nat } = a;"));
        assert!(!ok("const b: { x: Nat, y: Float, z: Nat } = a;"));
        assert!(!ok("const b: { x: String, y: Float } = a;"));
    }

    #[test]
    fn infers_the_type_of_a_value() {
        let types = "enum Shape = Circle { r: Float }, Dot;\n";
        let text = format!(
            "{}const a = 0;\nconst b = +0;\nconst c = 0.5;\nconst d = \"s\";\nconst e = Dot;\nconst f = Circle {{ r: 1 }};\n\
             const g = {{ x: 1, y: [\"s\"] }};\nconst h = [Some(1), None];",
            types
        );
        assert_eq!(
            inferred(&text, true),
            vec![
                "a/0: Nat",
                "b/0: Int",
                "c/0: Float",
                "d/0: String",
                "e/0: Shape",
                "f/0: Shape",
                "g/0: { x: Nat, y: [String] }",
                "h/0: [Option<Nat>]",
            ]
        );
    }

    #[test]
    fn numbers_in_an_array_join_unless_widening_is_off() {
        assert_eq!(
            inferred("const a = [1, 2.5];\nconst b = [1, +2];", true),
            vec!["a/0: [Float]", "b/0: [Int]"]
        );
        assert_eq!(const_errors("const a = [1, 2.5];").len(), 0);
        let file = parse(&format!("{}\nconst a = [1, 2.5];", PRELUDE_TYPES)).unwrap();
        let mut env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        let errors = check_consts(&mut env, false, DEFAULT_INFER_LIMIT, &consts);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].1.message, "Expected Nat but found Float");
    }

    #[test]
    fn what_the_value_leaves_open_is_generalised() {
        assert_eq!(
            inferred(
                "const e = [];\nconst n = None;\nconst m = null;\nconst p = { a: [], b: None, c: [] };",
                true
            ),
            vec![
                "e/1: [T]",
                "n/1: Option<T>",
                "m/1: Option<T>",
                "p/3: { a: [T], b: Option<U>, c: [V] }"
            ]
        );
        assert_eq!(
            inferred("const q = { a: Some(null) };", true),
            vec!["q/1: { a: Option<Option<T>> }"]
        );
    }

    #[test]
    fn each_reference_to_a_generalised_const_has_its_own_type() {
        let text = "const e = [];\nconst x: { a: [Nat], b: [String] } = { a: e, b: e };\nconst y = [e, [1]];";
        let file = parse(&format!("{}\n{}", PRELUDE_TYPES, text)).unwrap();
        let mut env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        // The references are not between declared types, so the test of the references is done on the values.
        assert!(check_consts(&mut env, true, DEFAULT_INFER_LIMIT, &consts).is_empty());
        assert_eq!(env.inferred_type("y").unwrap().text, "[[Nat]]");
        let value = &parse_values("[e, [\"s\"]]; Some(e); [e, e]").unwrap();
        for v in value {
            assert!(check_value(&env, true, v).is_ok());
        }
        let bad = &parse_values("[[1], [\"s\"]]").unwrap()[0];
        assert!(check_value(&env, true, bad).is_err());
    }

    #[test]
    fn a_reference_has_the_inferred_type_with_widening() {
        let file = parse(&format!("{}\nconst n = 1;", PRELUDE_TYPES)).unwrap();
        let mut env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        assert!(check_consts(&mut env, true, DEFAULT_INFER_LIMIT, &consts).is_empty());
        for ok in ["[n, +2]", "[n, 1.5]", "Some(n)"] {
            assert!(check_value(&env, true, &parse_values(ok).unwrap()[0]).is_ok(), "{}", ok);
        }
        let error = check_value(&env, true, &parse_values("[n, \"s\"]").unwrap()[0]).unwrap_err();
        assert_eq!(error.message, "Expected Nat but found String");
        // The type is Nat, not whatever the value could also have been.
        let error = check_value(&env, false, &parse_values("[n, +2]").unwrap()[0]).unwrap_err();
        assert_eq!(error.message, "Expected Nat but found Int");
    }

    #[test]
    fn consts_are_inferred_in_dependency_order() {
        let text = "const c = [b, b];\nconst b = Some(a);\nconst a = \"s\";";
        assert_eq!(
            inferred(text, true),
            vec!["c/0: [Option<String>]", "b/0: Option<String>", "a/0: String"]
        );
        // A declared const can come between two inferred ones.
        let text = "const c = d;\nconst d: Option<Nat> = e;\nconst e = null;";
        assert_eq!(inferred(text, true), vec!["c/0: Option<Nat>", "e/1: Option<T>"]);
    }

    #[test]
    fn a_failed_inference_is_reported_once() {
        let errors = const_errors("const a = [1, \"s\"];\nconst b = a;\nconst c = [b, b];");
        assert_eq!(errors, vec!["2:15 Expected Nat but found String"]);
        let errors = const_errors("const a = Missing;\nconst b: Nat = a;");
        assert_eq!(errors, vec!["2:11 Unknown constructor Missing"]);
    }

    #[test]
    fn a_huge_inferred_type_is_refused() {
        // Each const holds two copies of the one before, so the size of its type doubles.
        let mut text = "const x0 = { v: 1 };\n".to_string();
        for i in 1..40 {
            text.push_str(&format!("const x{} = {{ a: x{}, b: x{} }};\n", i, i - 1, i - 1));
        }
        let errors = const_errors(&text);
        // Once a const has failed the ones that refer to it are not checked against it, so the growth starts again.
        assert!(
            errors[0].contains("const x12 has more than 10000 nodes"),
            "{:?}",
            errors
        );
        assert!(
            errors
                .iter()
                .all(|e| e.ends_with("give it a type or raise --infer-limit")),
            "{:?}",
            errors
        );
        assert!(errors.len() < 10, "{:?}", errors);
    }

    #[test]
    fn the_inference_limit_can_be_changed() {
        let text = "const small = [];\nconst big = { a: [], b: [], c: [], d: [] };";
        let file = parse(&format!("{}\n{}", PRELUDE_TYPES, text)).unwrap();
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        let run = |limit: usize| {
            let mut env = Env::new(&file.enums, &file.consts);
            let errors = check_consts(&mut env, true, limit, &consts);
            errors.into_iter().map(|(_, e)| e.message).collect::<Vec<_>>()
        };
        assert!(run(DEFAULT_INFER_LIMIT).is_empty());
        // The type of big has nine nodes: the record and four arrays with an open type in each.
        assert!(run(9).is_empty());
        let errors = run(8);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("The type of const big has more than 8 nodes"),
            "{:?}",
            errors
        );
        // Nothing can be inferred with a limit of nothing, not even a single number.
        assert_eq!(run(0).len(), 2);
    }

    #[test]
    fn a_parameter_used_twice_in_a_type_is_unified_with_itself() {
        // Each reference to p gets its own T, and the two Ts are made equal by the first field and then compared
        // again for the second, which used to bind the variable to itself and loop for ever.
        let text = "const p<T>: { a: [T], b: [T] } = { a: [], b: [] };\nconst ps = [p, p];";
        assert_eq!(inferred(text, true), vec!["ps/1: [{ a: [T], b: [T] }]"]);
        let text = "const p<T>: { a: [T], b: [T], c: [[T]] } = { a: [], b: [], c: [] };\nconst ps = [p, p, p];";
        assert_eq!(inferred(text, true), vec!["ps/1: [{ a: [T], b: [T], c: [[T]] }]"]);
        // A conflict is still found when the repeated parameter is used with two types.
        let errors = const_errors(
            "const p<T>: { a: [T], b: [T] } = { a: [], b: [] };\nconst q: [{ a: [Nat], b: [String] }] = [p];",
        );
        assert_eq!(errors.len(), 1, "{:?}", errors);
    }

    #[test]
    fn a_variable_is_not_bound_to_a_type_that_contains_it() {
        let env = Env::new([], []);
        let mut checker = Checker {
            env: &env,
            widening: true,
            slots: Vec::new(),
            omitted: Vec::new(),
        };
        let pos = Pos { line: 1, col: 1 };
        let v = checker.fresh();
        let list = Ty::Array(Rc::new(v.clone()));
        // The same variable on both sides is already unified.
        assert!(checker.conforms(pos, &v, &v, "const x").is_ok());
        // A variable cannot be a list of itself, whichever side it is on.
        let error = checker.conforms(pos, &list, &v, "const x").unwrap_err();
        assert!(
            error.message.contains("a type that contains itself"),
            "{}",
            error.message
        );
        assert!(checker.conforms(pos, &v, &list, "const x").is_err());
        // The check looks through variables that are bound, and through records and enums.
        let w = checker.fresh();
        checker.bind(&w, Ty::Enum("E".to_string(), Rc::new(vec![v.clone()])), false);
        let nested = Ty::Record(Rc::new(vec![("f".to_string(), w)]));
        assert!(checker.conforms(pos, &nested, &v, "const x").is_err());
        // Nothing was bound by the failures, so the variable is still free and can take a type without it.
        assert!(checker.conforms(pos, &Ty::Nat, &v, "const x").is_ok());
        assert_eq!(checker.show(&v), "Nat");
    }

    /// The time that a check of many consts may take. It takes a fraction of a second, but it took minutes when the
    /// consts that had been visited were kept in a list that was searched in full each time.
    const MANY_CONSTS_LIMIT: std::time::Duration = std::time::Duration::from_secs(10);

    #[test]
    fn many_independent_consts_without_a_type_are_inferred_quickly() {
        let text: String = (0..100_000).map(|i| format!("const c{} = {};\n", i, i)).collect();
        let started = std::time::Instant::now();
        let found = inferred(&text, true);
        assert!(started.elapsed() < MANY_CONSTS_LIMIT, "{:?}", started.elapsed());
        assert_eq!(found.len(), 100_000);
        assert_eq!(found[99_999], "c99999/0: Nat");
    }

    #[test]
    fn a_long_chain_of_consts_without_a_type_is_inferred_in_dependency_order_quickly() {
        // Each const is declared before the one that it refers to, so the whole chain is visited from the first.
        let mut text: String = (1..100_000)
            .rev()
            .map(|i| format!("const c{} = c{};\n", i, i - 1))
            .collect();
        text.push_str("const c0 = \"s\";\n");
        let started = std::time::Instant::now();
        let found = inferred(&text, true);
        assert!(started.elapsed() < MANY_CONSTS_LIMIT, "{:?}", started.elapsed());
        assert_eq!(found.len(), 100_000);
        assert!(found.iter().all(|f| f.ends_with(": String")));
    }

    #[test]
    fn the_order_of_inference_visits_each_const_after_those_it_refers_to() {
        let file = parse("const c = [b, a];\nconst b = a;\nconst a = 1;\nconst d = c;\nconst e: Nat = a;").unwrap();
        let env = Env::new(&file.enums, &file.consts);
        let consts: Vec<(usize, &ConstDecl)> = file.consts.iter().map(|c| (0, c)).collect();
        let order: Vec<&str> = inference_order(&env, &consts)
            .iter()
            .map(|(_, c)| c.name.as_str())
            .collect();
        // The declared const e is left out, and a comes first because the others depend on it.
        assert_eq!(order, vec!["a", "b", "c", "d"]);
    }
}
