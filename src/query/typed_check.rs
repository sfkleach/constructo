//! Checks a typed query against the types before any value is read.
//!
//! The untyped check has already checked what the expression itself fixes. This pass adds what the types make
//! knowable: a field or parameter that no constructor of a type has, a pattern whose constructor is not of the type it
//! is matched against, a new value that cannot fit where `update` puts it, and a `match` that does not cover every
//! constructor of its enum, as in Rust. It only knows a type where the types give one: from `--type`, through the
//! fields, parameters and elements of a known type, and from the constructor in a pattern. Where no type is known it
//! says nothing, and the evaluator checks each value as it arrives.

use super::eval::canonical;
use super::parser::QUERY_NAME;
use super::syntax::*;
use super::types::{known, Types};
use crate::ast::VariantBody;
use crate::diagnostics::{Diagnostic, Pos};
use crate::typecheck::Ty;

type R<T> = Result<T, Diagnostic>;

fn fail<T>(pos: Pos, message: impl Into<String>) -> R<T> {
    Err(Diagnostic::new(QUERY_NAME, pos, message))
}

/// What the pass knows of an expression: one value, or an iterator, each with the type if it is known.
#[derive(Clone)]
enum TK {
    Val(Option<Ty>),
    Iter(Option<Ty>),
}

fn val(ty: Option<Ty>) -> R<TK> {
    Ok(TK::Val(ty.filter(known)))
}

/// A name as a message shows it, without its project.
fn shown(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

fn ty_text(ty: &Ty) -> String {
    match ty {
        Ty::Nat => "Nat".to_string(),
        Ty::Int => "Int".to_string(),
        Ty::Float => "Float".to_string(),
        Ty::Str => "String".to_string(),
        Ty::Array(element) => format!("[{}]", ty_text(element)),
        Ty::Record(_) => "a record".to_string(),
        Ty::Enum(name, _) => shown(name).to_string(),
        _ => "a type that is not known".to_string(),
    }
}

/// Checks a typed query, whose constructor names have been resolved, given the type of every value if it is known.
pub fn check_typed(expr: &Expr, types: &Types, target: Option<&Ty>) -> R<()> {
    let mut checker = Checker {
        types,
        target: target.cloned(),
        scope: Vec::new(),
    };
    checker.expr(expr).map(|_| ())
}

struct Checker<'t> {
    types: &'t Types,
    target: Option<Ty>,
    scope: Vec<(String, Option<Ty>)>,
}

impl Checker<'_> {
    fn lookup(&self, name: &str) -> Option<Ty> {
        self.scope
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .and_then(|(_, ty)| ty.clone())
    }

    /// The type of a field of a value of a type. It is an error if no constructor of the type has the field. The type
    /// is known if every constructor that has the field gives it the same type.
    fn field(&self, ty: &Ty, name: &str, pos: Pos) -> R<Option<Ty>> {
        match ty {
            Ty::Record(fields) => match fields.iter().find(|(n, _)| n == name) {
                Some((_, t)) => Ok(Some(t.clone())),
                None => fail(pos, format!("The record has no field {}", name)),
            },
            Ty::Enum(enum_name, _) => {
                let Some(decl) = self.types.enum_decl(enum_name) else {
                    return Ok(None);
                };
                let mut found: Vec<Ty> = Vec::new();
                for variant in &decl.variants {
                    let t = match &variant.body {
                        VariantBody::Record(_) => self
                            .types
                            .ctor_fields(&variant.name, Some(ty))
                            .and_then(|fs| fs.into_iter().find(|(n, _)| n == name).map(|(_, t)| t)),
                        VariantBody::Tuple(_) => {
                            self.types.tuple_params(&variant.name, Some(ty)).and_then(|(ps, ts)| {
                                ps.iter()
                                    .position(|p| p.name.as_deref() == Some(name))
                                    .map(|i| ts[i].clone())
                            })
                        }
                        _ => None,
                    };
                    found.extend(t);
                }
                if found.is_empty() {
                    return fail(
                        pos,
                        format!("No constructor of {} has a field or a parameter {}", ty_text(ty), name),
                    );
                }
                let first = found[0].clone();
                Ok(found.iter().all(|t| same(t, &first)).then_some(first))
            }
            Ty::Nat | Ty::Int | Ty::Float | Ty::Str | Ty::Array(_) => {
                fail(pos, format!("A value of {} has no field {}", ty_text(ty), name))
            }
            _ => Ok(None),
        }
    }

    /// The type of a parameter, by position, of a value of a type, as `field` does for a name.
    fn position(&self, ty: &Ty, n: usize, pos: Pos) -> R<Option<Ty>> {
        let Ty::Enum(enum_name, _) = ty else {
            if known(ty) {
                return fail(pos, format!("A value of {} has no argument {}", ty_text(ty), n));
            }
            return Ok(None);
        };
        let Some(decl) = self.types.enum_decl(enum_name) else {
            return Ok(None);
        };
        let mut found: Vec<Ty> = Vec::new();
        for variant in &decl.variants {
            if let Some((_, ts)) = self.types.tuple_params(&variant.name, Some(ty)) {
                found.extend(ts.get(n).cloned());
            }
        }
        if found.is_empty() {
            return fail(pos, format!("No constructor of {} has an argument {}", ty_text(ty), n));
        }
        let first = found[0].clone();
        Ok(found.iter().all(|t| same(t, &first)).then_some(first))
    }

    fn element(&self, ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Array(element) => Some((**element).clone()),
            Ty::Enum(enum_name, _) => {
                let decl = self.types.enum_decl(enum_name)?;
                let [variant] = &decl.variants[..] else { return None };
                self.types.ctor_element(&variant.name, Some(ty))
            }
            _ => None,
        }
    }

    fn value(&mut self, expr: &Expr) -> R<Option<Ty>> {
        match self.expr(expr)? {
            TK::Val(ty) => Ok(ty),
            TK::Iter(_) => Ok(None),
        }
    }

    fn expr(&mut self, expr: &Expr) -> R<TK> {
        let pos = expr.pos;
        match &expr.kind {
            ExprKind::Values => Ok(TK::Iter(self.target.clone())),
            ExprKind::Var(name) => val(self.lookup(name)),
            ExprKind::Lit(lit) => val(Some(match lit {
                Lit::Str(_) => Ty::Str,
                // A number literal adapts to the kind it meets, so it fixes no type here.
                _ => return val(None),
            })),
            ExprKind::Ctor { name, body } => {
                match body {
                    None => {}
                    Some(CtorBody::Record { fields, base }) => {
                        let declared = self.types.ctor_fields(name, None);
                        for f in fields {
                            if let Some(declared) = &declared {
                                if !declared.iter().any(|(n, _)| *n == f.name) {
                                    return fail(f.pos, format!("{} has no field {}", shown(name), f.name));
                                }
                            }
                            self.value(&f.value)?;
                        }
                        if let Some(base) = base {
                            self.value(base)?;
                        }
                    }
                    Some(CtorBody::Tuple(args)) => {
                        let params = self.types.tuple_params(name, None);
                        for a in args {
                            if let (Some(n), Some((ps, _))) = (&a.name, &params) {
                                if !ps.iter().any(|p| p.name.as_deref() == Some(n.as_str())) {
                                    return fail(a.pos, format!("{} has no parameter {}", shown(name), n));
                                }
                            }
                            self.value(&a.value)?;
                        }
                    }
                    Some(CtorBody::Array(items)) => {
                        for item in items {
                            self.value(item)?;
                        }
                    }
                }
                val(self.types.ctor_type(name, None))
            }
            ExprKind::Record { fields, base } => {
                for f in fields {
                    self.value(&f.value)?;
                }
                if let Some(base) = base {
                    self.value(base)?;
                }
                val(None)
            }
            ExprKind::Array(items) => {
                for item in items {
                    self.value(item)?;
                }
                val(None)
            }
            ExprKind::Field(of, name) => match self.value(of)? {
                Some(ty) => val(self.field(&ty, name, pos)?),
                None => val(None),
            },
            ExprKind::Position(of, n) => match self.value(of)? {
                Some(ty) => val(self.position(&ty, *n, pos)?),
                None => val(None),
            },
            ExprKind::Index(of, index) => {
                let ty = self.value(of)?;
                self.value(index)?;
                val(ty.and_then(|t| self.element(&t)))
            }
            ExprKind::Method { receiver, name, args } => self.method(receiver, name, args),
            ExprKind::Closure(_) => val(None),
            ExprKind::Unary(_, operand) | ExprKind::Cast(operand, _) => {
                self.value(operand)?;
                val(None)
            }
            ExprKind::Binary(_, a, b) => {
                self.value(a)?;
                self.value(b)?;
                val(None)
            }
            ExprKind::If { branches, otherwise } => {
                for (cond, then) in branches {
                    self.value(cond)?;
                    self.value(then)?;
                }
                self.value(otherwise)?;
                val(None)
            }
            ExprKind::Match { scrutinee, arms } => {
                let ty = self.value(scrutinee)?;
                for arm in arms {
                    let mark = self.scope.len();
                    self.pattern(&arm.pattern, ty.as_ref())?;
                    if let Some(guard) = &arm.guard {
                        self.value(guard)?;
                    }
                    self.value(&arm.body)?;
                    self.scope.truncate(mark);
                }
                if let Some(ty) = &ty {
                    self.exhaustive(ty, arms, pos)?;
                }
                val(None)
            }
            ExprKind::Matches(subject, pattern) => {
                let ty = self.value(subject)?;
                let mark = self.scope.len();
                self.pattern(pattern, ty.as_ref())?;
                self.scope.truncate(mark);
                val(None)
            }
            ExprKind::Assign { .. } => val(None),
        }
    }

    fn method(&mut self, receiver: &Expr, name: &str, args: &[Expr]) -> R<TK> {
        let r = self.expr(receiver)?;
        let closure = args.first().and_then(|a| match &a.kind {
            ExprKind::Closure(c) => Some(c.clone()),
            _ => None,
        });
        if closure.is_none() {
            for a in args {
                self.value(a)?;
            }
        }
        match r {
            TK::Iter(element) => {
                let body = match &closure {
                    Some(c) => self.closure(c, element.as_ref())?,
                    None => None,
                };
                Ok(match name {
                    "map" | "filter_match" => TK::Iter(body),
                    "flat_map" => TK::Iter(body.and_then(|t| self.element(&t))),
                    "filter" | "take" | "skip" | "filter_map" => {
                        TK::Iter(if name == "filter_map" { None } else { element })
                    }
                    "collect" => TK::Val(element.map(|t| Ty::Array(std::rc::Rc::new(t)))),
                    "count" => TK::Val(Some(Ty::Nat)),
                    _ => TK::Val(None),
                })
            }
            TK::Val(ty) => match name {
                "iter" => Ok(TK::Iter(ty.and_then(|t| self.element(&t)))),
                "char_strings" | "split" => Ok(TK::Iter(Some(Ty::Str))),
                "len" => Ok(TK::Val(Some(Ty::Nat))),
                "update" => {
                    if let Some(c) = &closure {
                        self.update(c, ty.as_ref())?;
                    }
                    Ok(TK::Val(ty))
                }
                _ => {
                    if let Some(c) = &closure {
                        self.closure(c, None)?;
                    }
                    Ok(TK::Val(None))
                }
            },
        }
    }

    /// Checks a closure whose parameter has a type, if it is known, and gives the type of its body.
    fn closure(&mut self, closure: &Closure, param: Option<&Ty>) -> R<Option<Ty>> {
        let mark = self.scope.len();
        if let Some(p) = closure.params.first() {
            self.pattern(p, param)?;
        }
        let body = self.value(&closure.body);
        self.scope.truncate(mark);
        body
    }

    /// Checks the closure of `update`: the path must be in the type, and the new value must fit there.
    fn update(&mut self, closure: &Closure, ty: Option<&Ty>) -> R<()> {
        let ExprKind::Assign { target, op, value } = &closure.body.kind else {
            return Ok(());
        };
        let mark = self.scope.len();
        if let Some(p) = closure.params.first() {
            self.pattern(p, ty)?;
        }
        let at = self.value(target)?;
        let new = self.value(value)?;
        self.scope.truncate(mark);
        let Some(at) = at else { return Ok(()) };
        if let Some(op) = op {
            let ok = matches!(at, Ty::Nat | Ty::Int | Ty::Float) || (*op == BinOp::Add && matches!(at, Ty::Str));
            if !ok {
                return fail(
                    closure.body.pos,
                    format!("{}= needs a number, and this is {}", op.symbol(), ty_text(&at)),
                );
            }
        }
        if !self.fits(value, &at) {
            let what = match (&value.kind, &new) {
                (ExprKind::Lit(Lit::Nat(_)), _) => "a Nat".to_string(),
                (ExprKind::Lit(Lit::Int(_)), _) => "an Int".to_string(),
                (ExprKind::Lit(Lit::Float(_)), _) => "a Float".to_string(),
                (ExprKind::Record { .. }, _) => "a record".to_string(),
                (ExprKind::Array(_), _) => "an array".to_string(),
                (_, Some(new)) => ty_text(new),
                (_, None) => "a value".to_string(),
            };
            return fail(
                value.pos,
                format!(
                    "The new value is {}, which does not fit where it goes, which is of type {}",
                    what,
                    super::types::type_name(&at)
                ),
            );
        }
        Ok(())
    }

    /// Whether the value of an expression can fit where a type is expected, as far as the expression shows: a literal
    /// by its kind, an array, a record or a constructor by its parts, and anything else by the type it has, if that is
    /// known. Where nothing is known it is taken to fit, and the value is checked when it is made.
    fn fits(&mut self, expr: &Expr, at: &Ty) -> bool {
        if !known(at) {
            return true;
        }
        match (&expr.kind, at) {
            (ExprKind::Lit(Lit::Nat(_)), _) => matches!(at, Ty::Nat | Ty::Int | Ty::Float),
            (ExprKind::Lit(Lit::Int(_)), _) => matches!(at, Ty::Int | Ty::Float),
            (ExprKind::Lit(Lit::Float(_)), _) => matches!(at, Ty::Float),
            (ExprKind::Lit(Lit::Str(_)), _) => matches!(at, Ty::Str),
            (ExprKind::Array(items), Ty::Array(element)) => items.iter().all(|item| self.fits(item, element)),
            (ExprKind::Array(_), _) => false,
            (ExprKind::Record { fields, base }, Ty::Record(declared)) => {
                let named = fields
                    .iter()
                    .all(|f| declared.iter().any(|(n, t)| *n == f.name && self.fits(&f.value, t)));
                named && (base.is_some() || fields.len() == declared.len())
            }
            (ExprKind::Record { .. }, _) => false,
            (ExprKind::Var(name), Ty::Enum(enum_name, _)) if matches!(name.as_str(), "true" | "false") => {
                canonical(shown(enum_name)) == "Bool"
            }
            (ExprKind::Ctor { name, body }, Ty::Enum(enum_name, _)) => {
                match self.types.ctor(name) {
                    Some((decl, _)) if decl.name != *enum_name => return false,
                    Some(_) => {}
                    None => return true,
                }
                match body {
                    None => true,
                    Some(CtorBody::Record { fields, base }) => match self.types.ctor_fields(name, Some(at)) {
                        Some(declared) => {
                            let named = fields
                                .iter()
                                .all(|f| declared.iter().any(|(n, t)| *n == f.name && self.fits(&f.value, t)));
                            named && (base.is_some() || fields.len() == declared.len())
                        }
                        None => false,
                    },
                    Some(CtorBody::Array(items)) => match self.types.ctor_element(name, Some(at)) {
                        Some(element) => items.iter().all(|item| self.fits(item, &element)),
                        None => false,
                    },
                    Some(CtorBody::Tuple(args)) => match self.types.tuple_params(name, Some(at)) {
                        Some((params, tys)) => {
                            let mut position = 0;
                            args.iter().all(|a| {
                                let index = match &a.name {
                                    Some(n) => params.iter().position(|p| p.name.as_deref() == Some(n.as_str())),
                                    None => {
                                        position += 1;
                                        Some(position - 1).filter(|i| *i < params.len())
                                    }
                                };
                                index.is_some_and(|i| self.fits(&a.value, &tys[i]))
                            })
                        }
                        None => false,
                    },
                }
            }
            (ExprKind::Ctor { .. }, _) => false,
            _ => match self.value(expr) {
                Ok(Some(new)) => compatible(&new, at),
                _ => true,
            },
        }
    }

    /// Binds the names of a pattern with their types, and checks that a constructor in it is of the type it is
    /// matched against.
    fn pattern(&mut self, pattern: &Pattern, ty: Option<&Ty>) -> R<()> {
        match &pattern.kind {
            PatternKind::Wild | PatternKind::Lit(_) => Ok(()),
            PatternKind::Bind(name) => {
                self.scope.push((name.clone(), ty.cloned()));
                Ok(())
            }
            PatternKind::At(name, inner) => {
                self.pattern(inner, ty)?;
                self.scope.push((name.clone(), ty.cloned()));
                Ok(())
            }
            PatternKind::Or(alternatives) => {
                // Every alternative binds the same names, which the untyped check made sure of. A name keeps a type
                // only if every alternative gives it the same one; otherwise the value is checked when it arrives.
                let before = self.scope.len();
                let mut merged: Vec<(String, Option<Ty>)> = Vec::new();
                for (i, p) in alternatives.iter().enumerate() {
                    self.pattern(p, ty)?;
                    let bound: Vec<(String, Option<Ty>)> = self.scope.drain(before..).collect();
                    if i == 0 {
                        merged = bound;
                    } else {
                        for (name, t) in &mut merged {
                            let other = bound.iter().find(|(n, _)| n == name).and_then(|(_, t)| t.as_ref());
                            if !matches!((&*t, other), (Some(x), Some(y)) if same(x, y)) {
                                *t = None;
                            }
                        }
                    }
                }
                self.scope.extend(merged);
                Ok(())
            }
            PatternKind::Ctor { name, body } => {
                if crate::convert::is_const_name(name) {
                    return Ok(());
                }
                if let (Some(Ty::Enum(enum_name, _)), Some((decl, _))) = (ty, self.types.ctor(name)) {
                    if decl.name != *enum_name {
                        return fail(
                            pattern.pos,
                            format!(
                                "{} is a constructor of {}, so it never matches a value of {}",
                                shown(name),
                                shown(&decl.name),
                                shown(enum_name)
                            ),
                        );
                    }
                }
                let ty = self.types.ctor_type(name, ty);
                match body {
                    None => Ok(()),
                    Some(PatBody::Record { fields, .. }) => {
                        let declared = self.types.ctor_fields(name, ty.as_ref());
                        for f in fields {
                            let field_ty = match &declared {
                                Some(declared) => match declared.iter().find(|(n, _)| *n == f.name) {
                                    Some((_, t)) => Some(t.clone()),
                                    None => return fail(f.pos, format!("{} has no field {}", shown(name), f.name)),
                                },
                                None => None,
                            };
                            self.pattern(&f.pattern, field_ty.as_ref())?;
                        }
                        Ok(())
                    }
                    Some(PatBody::Tuple { args, .. }) => {
                        let params = self.types.tuple_params(name, ty.as_ref());
                        let mut position = 0;
                        for a in args {
                            let arg_ty = match (&a.name, &params) {
                                (Some(n), Some((ps, ts))) => match ps.iter().position(|p| p.name.as_deref() == Some(n))
                                {
                                    Some(i) => Some(ts[i].clone()),
                                    None => return fail(a.pos, format!("{} has no parameter {}", shown(name), n)),
                                },
                                (None, Some((_, ts))) => {
                                    position += 1;
                                    ts.get(position - 1).cloned()
                                }
                                _ => None,
                            };
                            self.pattern(&a.pattern, arg_ty.as_ref())?;
                        }
                        Ok(())
                    }
                    Some(PatBody::Array { items, .. }) => {
                        let element = self.types.ctor_element(name, ty.as_ref());
                        items.iter().try_for_each(|p| self.pattern(p, element.as_ref()))
                    }
                }
            }
            PatternKind::Record { fields, .. } => {
                for f in fields {
                    let field_ty = match ty {
                        Some(Ty::Record(declared)) => {
                            declared.iter().find(|(n, _)| *n == f.name).map(|(_, t)| t.clone())
                        }
                        _ => None,
                    };
                    self.pattern(&f.pattern, field_ty.as_ref())?;
                }
                Ok(())
            }
            PatternKind::Array { items, .. } => {
                let element = ty.and_then(|t| self.element(t));
                items.iter().try_for_each(|p| self.pattern(p, element.as_ref()))
            }
        }
    }

    /// Checks that the arms of a `match` on a value of an enum cover every constructor of it, as in Rust. An arm covers
    /// a constructor if its pattern is that constructor with a body that always matches, and has no guard. An arm that
    /// always matches covers them all.
    fn exhaustive(&self, ty: &Ty, arms: &[Arm], pos: Pos) -> R<()> {
        let Ty::Enum(enum_name, _) = ty else { return Ok(()) };
        let Some(decl) = self.types.enum_decl(enum_name) else {
            return Ok(());
        };
        let mut covered: Vec<String> = Vec::new();
        for arm in arms.iter().filter(|a| a.guard.is_none()) {
            if covers(&arm.pattern, &mut covered) {
                return Ok(());
            }
        }
        let missing: Vec<&str> = decl
            .variants
            .iter()
            .filter(|v| !covered.iter().any(|c| canonical(c) == canonical(&v.name)))
            .map(|v| shown(&v.name))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        fail(
            pos,
            format!(
                "The match does not cover {} of {}; add an arm for it, or _ for the rest",
                missing.join(", "),
                shown(enum_name)
            ),
        )
    }
}

/// Whether a pattern always matches, or else adds the constructors that it covers.
fn covers(pattern: &Pattern, covered: &mut Vec<String>) -> bool {
    match &pattern.kind {
        PatternKind::Wild | PatternKind::Bind(_) => true,
        PatternKind::At(_, inner) => covers(inner, covered),
        PatternKind::Or(all) => all.iter().any(|p| covers(p, covered)),
        PatternKind::Ctor { name, body } => {
            let whole = match body {
                None => true,
                Some(PatBody::Record { fields, .. }) => fields.iter().all(|f| f.pattern.is_irrefutable()),
                Some(PatBody::Tuple { args, .. }) => args.iter().all(|a| a.pattern.is_irrefutable()),
                Some(PatBody::Array { items, rest }) => *rest && items.is_empty(),
            };
            if whole {
                covered.push(name.clone());
            }
            false
        }
        _ => false,
    }
}

/// Whether two types are the same, as far as their outermost form tells.
fn same(a: &Ty, b: &Ty) -> bool {
    agree(a, b, false)
}

/// Whether two types agree, comparing their arguments, elements and fields. A part that is not known agrees with
/// anything when `lenient`, and with nothing otherwise.
fn agree(a: &Ty, b: &Ty, lenient: bool) -> bool {
    if !known(a) || !known(b) {
        return lenient;
    }
    match (a, b) {
        (Ty::Enum(x, xs), Ty::Enum(y, ys)) => {
            x == y && xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(p, q)| agree(p, q, lenient))
        }
        (Ty::Array(x), Ty::Array(y)) => agree(x, y, lenient),
        (Ty::Record(xs), Ty::Record(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys.iter())
                    .all(|((n, p), (m, q))| n == m && agree(p, q, lenient))
        }
        (x, y) => std::mem::discriminant(x) == std::mem::discriminant(y),
    }
}

/// Whether a value of one type can go where another is expected, with the widening of numbers.
fn compatible(new: &Ty, at: &Ty) -> bool {
    match (new, at) {
        (Ty::Nat, Ty::Int | Ty::Float) | (Ty::Int, Ty::Float) => true,
        (x, y) if !known(x) || !known(y) => true,
        (x, y) => agree(x, y, true),
    }
}
