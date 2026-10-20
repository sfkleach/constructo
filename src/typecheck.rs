//! The type-check of data values against data-type declarations.
//!
//! A value is checked against an expected type, which for a top-level value is a fresh type variable. Type
//! parameters of generic enums are instantiated with fresh variables as constructors are met, and the variables are
//! bound by the values that use them, so that one parameter has the same type throughout a top-level value.
//!
//! The errors are reported at the innermost term that fails. Checking a top-level value stops at its first error.

use crate::ast::{EnumDecl, Field, TaggedBody, Type, Value, ValueKind, Variant, VariantBody};
use crate::diagnostics::Pos;
use std::collections::HashMap;
use std::fmt;
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
    constructors: HashMap<&'a str, (&'a EnumDecl, &'a Variant)>,
}

impl<'a> Env<'a> {
    /// Builds the scope from declarations that have already passed validation, so that there are no duplicates.
    pub fn new(decls: impl IntoIterator<Item = &'a EnumDecl>) -> Self {
        let mut env = Env {
            enums: HashMap::new(),
            constructors: HashMap::new(),
        };
        for decl in decls {
            env.enums.entry(decl.name.as_str()).or_insert(decl);
            for variant in &decl.variants {
                env.constructors.entry(variant.name.as_str()).or_insert((decl, variant));
            }
        }
        env
    }
}

/// Checks one top-level value. With `widening`, a Nat fits where an Int or Float is expected and an Int where a Float
/// is expected; otherwise a number must have exactly the type it is expected to have.
pub fn check_value(env: &Env, widening: bool, value: &Value) -> Result<()> {
    let mut checker = Checker {
        env,
        widening,
        slots: Vec::new(),
    };
    let top = checker.fresh();
    checker.check(value, &top)
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
}

enum Slot {
    Free,
    /// A variable bound to a type. A variable that was bound by a number literal is flexible: a later, wider
    /// number can widen it, which is how the join of `[1, 2.5]` becomes `[Float]`.
    Bound(Ty, bool),
}

/// The longest description of a type that is put in a message, in characters.
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
}

impl Checker<'_, '_> {
    fn fresh(&mut self) -> Ty {
        self.slots.push(Slot::Free);
        Ty::Var(self.slots.len() - 1)
    }

    /// Resolves the outermost variable, if there is one, to the type it is bound to. A free variable stays a variable.
    fn shallow(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(id) => match &self.slots[*id] {
                Slot::Bound(bound, _) => bound.clone(),
                Slot::Free => Ty::Var(*id),
            },
            other => other.clone(),
        }
    }

    /// The variable, if it is bound by a number literal and so may still be widened.
    fn flexible_var(&self, ty: &Ty) -> Option<usize> {
        match ty {
            Ty::Var(id) if matches!(self.slots[*id], Slot::Bound(_, true)) => Some(*id),
            _ => None,
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
            ValueKind::Null => {
                let found = self.env.enums.get("Option").and_then(|decl| {
                    decl.variants
                        .iter()
                        .find(|v| v.name == "None")
                        .map(|variant| (*decl, variant))
                });
                match found {
                    Some((decl, variant)) => self.check_variant(pos, decl, variant, None, &expected, "null"),
                    None => error(pos, "Cannot use null because the type Option is not in scope"),
                }
            }
            ValueKind::Tagged { name, body } => match self.env.constructors.get(name.as_str()) {
                Some(&(decl, variant)) => {
                    let found = format!("{} of type {}", name, decl.name);
                    self.check_variant(pos, decl, variant, body.as_ref(), &expected, &found)
                }
                None => error(pos, format!("Unknown constructor {}", name)),
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
                let fields = instantiate_fields(fields, decl, &args);
                self.check_members(pos, name, members, &fields)
            }
            (VariantBody::Record(_), _) => error(pos, format!("Constructor {} takes a record", name)),
            (VariantBody::Tuple(types), Some(TaggedBody::Tuple(values))) => {
                if types.len() != values.len() {
                    let message = format!(
                        "Constructor {} takes {} value(s) but has {}",
                        name,
                        types.len(),
                        values.len()
                    );
                    return error(pos, message);
                }
                for (ty, value) in types.iter().zip(values) {
                    let ty = instantiate(ty, decl, &args);
                    self.check(value, &ty)?;
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
}

fn instantiate_fields(fields: &[Field], decl: &EnumDecl, args: &[Ty]) -> Vec<(String, Ty)> {
    fields
        .iter()
        .map(|f| (f.name.clone(), instantiate(&f.ty, decl, args)))
        .collect()
}

/// Turns a declared type into a `Ty`, replacing the enum's type parameters with the given arguments.
fn instantiate(ty: &Type, decl: &EnumDecl, args: &[Ty]) -> Ty {
    match ty {
        Type::Ref {
            name, args: type_args, ..
        } => {
            if let Some(index) = decl.params.iter().position(|p| p.name == *name) {
                return args[index].clone();
            }
            match name.as_str() {
                "Nat" => Ty::Nat,
                "Int" => Ty::Int,
                "Float" => Ty::Float,
                "String" => Ty::Str,
                _ => Ty::Enum(
                    name.clone(),
                    Rc::new(type_args.iter().map(|t| instantiate(t, decl, args)).collect()),
                ),
            }
        }
        Type::Array(elem) => Ty::Array(Rc::new(instantiate(elem, decl, args))),
        Type::Record(fields) => Ty::Record(Rc::new(instantiate_fields(fields, decl, args))),
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
        let decls = parse(&format!("enum Option<T> = None, Some(T); {}", types)).unwrap();
        let env = Env::new(&decls);
        parse_values(values)
            .unwrap()
            .iter()
            .filter_map(|v| check_value(&env, widening, v).err())
            .map(|e| format!("{}:{} {}", e.pos.line, e.pos.col, e.message))
            .collect()
    }

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
    fn null_is_none_of_option() {
        let types = "enum Config = Config { owner: Option<String> };";
        assert!(errors(
            types,
            "Config { owner: null }; Config { owner: Some(\"me\") }; null",
            true
        )
        .is_empty());
        assert_eq!(
            errors(types, "Config { owner: Some(null) }", true),
            vec!["1:22 Expected String but found null"]
        );
        assert_eq!(
            errors("enum A = A(Nat);", "A(null)", true),
            vec!["1:3 Expected Nat but found null"]
        );
    }

    #[test]
    fn null_needs_option_in_scope() {
        let decls = parse("enum A = A;").unwrap();
        let env = Env::new(&decls);
        let value = &parse_values("null").unwrap()[0];
        assert_eq!(
            check_value(&env, true, value).unwrap_err().message,
            "Cannot use null because the type Option is not in scope"
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
}
