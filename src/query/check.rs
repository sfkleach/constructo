//! Checks a query before any data is read.
//!
//! Without the types, a query is still type-checked: what the expression itself fixes is checked here, so that
//! `if v.ok then 1 else "failed" endif` is refused up front, and what comes from the data is checked by the evaluator
//! as each value arrives. This also checks the rules of the language that do not depend on types: where `#values`, a
//! closure and an assignment may be, that the names are bound, and that a closure's parameter must always match except
//! in `filter_match`.

use super::eval::canonical;
use super::number::kind_of;
use super::parser::QUERY_NAME;
use super::syntax::*;
use crate::ast::{Value, ValueKind};
use crate::diagnostics::{Diagnostic, Pos};

/// What the expression fixes about a value's type, as far as its kind can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixed {
    /// A number of a kind. A literal adapts to a wider kind, as `1` does to a `Float`.
    Num(NumKind, bool),
    Str,
    Bool,
    Record,
    Array,
    /// A constructor, which cannot be told from another without the types.
    Ctor,
}

impl Fixed {
    pub fn of(value: &Value) -> Fixed {
        match &value.kind {
            ValueKind::Nat(t) | ValueKind::Int(t) | ValueKind::Float(t) => Fixed::Num(kind_of(t), false),
            ValueKind::Str(_) => Fixed::Str,
            ValueKind::Array(_) => Fixed::Array,
            ValueKind::Record(_) => Fixed::Record,
            ValueKind::Tagged { name, body: None } if matches!(canonical(name), "true" | "false") => Fixed::Bool,
            ValueKind::Tagged { .. } => Fixed::Ctor,
        }
    }

    /// Whether a value, a literal or not, has a type in common with what is fixed.
    pub fn admits(self, value: &Value, lit: bool) -> bool {
        match (self, Fixed::of(value)) {
            (Fixed::Num(k, fixed_lit), Fixed::Num(v, _)) => compatible_numbers(k, fixed_lit, v, lit),
            (a, b) => a == b,
        }
    }

    pub fn describe(self) -> String {
        match self {
            Fixed::Num(NumKind::Int, _) => "an Int".to_string(),
            Fixed::Num(k, _) => format!("a {}", k.name()),
            Fixed::Str => "a String".to_string(),
            Fixed::Bool => "a Bool".to_string(),
            Fixed::Record => "a record".to_string(),
            Fixed::Array => "an array".to_string(),
            Fixed::Ctor => "a constructor".to_string(),
        }
    }

    fn compatible(self, other: Fixed) -> bool {
        match (self, other) {
            (Fixed::Num(a, la), Fixed::Num(b, lb)) => compatible_numbers(a, la, b, lb),
            (a, b) => a == b,
        }
    }
}

fn widens(from: NumKind, to: NumKind) -> bool {
    matches!(
        (from, to),
        (NumKind::Nat, NumKind::Int) | (NumKind::Nat, NumKind::Float) | (NumKind::Int, NumKind::Float)
    )
}

fn compatible_numbers(a: NumKind, a_lit: bool, b: NumKind, b_lit: bool) -> bool {
    a == b || (a_lit && widens(a, b)) || (b_lit && widens(b, a))
}

/// What the check knows of an expression's result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum K {
    /// A value whose type comes from the data.
    Any,
    /// A value whose kind the expression fixes.
    Fixed(Fixed),
    /// An iterator.
    Iter,
}

/// What an expression fixes about its value, on its own, with every name taken to come from the data. The evaluator
/// uses it to check the branch that is taken against those that are not.
pub fn fixed_kind(expr: &Expr) -> Option<Fixed> {
    let mut checker = Checker {
        names: Vec::new(),
        values_used: true,
        closures: 0,
        lenient: true,
        consts: true,
    };
    match checker.expr(expr) {
        Ok(K::Fixed(f)) => Some(f),
        _ => None,
    }
}

/// Checks a whole query. A typed query may refer to the consts of its types, which an untyped one cannot know.
pub fn check_query(expr: &Expr, typed: bool) -> Result<(), Diagnostic> {
    let mut checker = Checker {
        names: Vec::new(),
        values_used: false,
        closures: 0,
        lenient: false,
        consts: typed,
    };
    checker.expr(expr).map(|_| ())
}

struct Checker {
    /// The names that patterns have bound, innermost last.
    names: Vec<String>,
    values_used: bool,
    /// How many closures the expression being checked is inside.
    closures: u32,
    /// Whether names that are not bound are taken to come from the data, for `fixed_kind`.
    lenient: bool,
    /// Whether the query has types, whose consts it may refer to.
    consts: bool,
}

type R<T> = Result<T, Diagnostic>;

fn fail<T>(pos: Pos, message: impl Into<String>) -> R<T> {
    Err(Diagnostic::new(QUERY_NAME, pos, message))
}

fn describe_k(k: K) -> String {
    match k {
        K::Any => "a value".to_string(),
        K::Fixed(f) => f.describe(),
        K::Iter => "an iterator".to_string(),
    }
}

/// The methods of an iterator, and whether each takes a closure, and what it gives.
fn stream_method(name: &str) -> Option<(Arg, K)> {
    let option = K::Fixed(Fixed::Ctor);
    Some(match name {
        "map" | "filter" | "filter_map" | "flat_map" => (Arg::Closure, K::Iter),
        "filter_match" => (Arg::RefutableClosure, K::Iter),
        "take" | "skip" => (Arg::Nat, K::Iter),
        "nth" => (Arg::Nat, option),
        "next" | "last" => (Arg::None, option),
        "count" => (Arg::None, K::Fixed(Fixed::Num(NumKind::Nat, false))),
        "collect" => (Arg::None, K::Fixed(Fixed::Array)),
        "any" | "all" => (Arg::Closure, K::Fixed(Fixed::Bool)),
        "find" => (Arg::Closure, option),
        _ => return None,
    })
}

/// The methods of a value, which the kind of the value decides when it arrives.
fn value_method(name: &str) -> Option<(Arg, K)> {
    let nat = K::Fixed(Fixed::Num(NumKind::Nat, false));
    let boolean = K::Fixed(Fixed::Bool);
    let option = K::Fixed(Fixed::Ctor);
    let string = K::Fixed(Fixed::Str);
    Some(match name {
        "len" => (Arg::None, nat),
        "is_empty" | "is_some" | "is_none" => (Arg::None, boolean),
        "first" | "last" => (Arg::None, option),
        "get" => (Arg::Nat, option),
        "iter" | "char_strings" => (Arg::None, K::Iter),
        "split" => (Arg::Str, K::Iter),
        "contains" | "starts_with" | "ends_with" => (Arg::Str, boolean),
        "trim" | "to_lowercase" | "to_uppercase" => (Arg::None, string),
        "unwrap_or" => (Arg::Value, K::Any),
        "update" => (Arg::Update, K::Any),
        _ => return None,
    })
}

/// Whether a value of a kind has a method. A constructor may have an array body, or be an Option, so it is allowed
/// the methods of either, and the value is checked when it arrives.
fn receiver_has(name: &str, kind: Fixed) -> bool {
    match name {
        "contains" | "starts_with" | "ends_with" | "trim" | "to_lowercase" | "to_uppercase" | "char_strings"
        | "split" => kind == Fixed::Str,
        "len" | "is_empty" => matches!(kind, Fixed::Str | Fixed::Array | Fixed::Ctor),
        "first" | "last" | "get" | "iter" => matches!(kind, Fixed::Array | Fixed::Ctor),
        "is_some" | "is_none" | "unwrap_or" => kind == Fixed::Ctor,
        "update" => matches!(kind, Fixed::Record | Fixed::Ctor),
        _ => true,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arg {
    None,
    Nat,
    Str,
    Value,
    Closure,
    RefutableClosure,
    Update,
}

impl Checker {
    fn bound(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }

    /// Checks an expression that must give one value, not an iterator.
    fn value(&mut self, expr: &Expr) -> R<K> {
        let k = self.expr(expr)?;
        if k == K::Iter {
            return fail(
                expr.pos,
                "This is an iterator, which cannot be a value here; use .collect() to make an array of it",
            );
        }
        Ok(k)
    }

    fn expect(&mut self, expr: &Expr, wanted: Fixed, what: &str) -> R<K> {
        let k = self.value(expr)?;
        if let K::Fixed(f) = k {
            if !f.compatible(wanted) {
                return fail(
                    expr.pos,
                    format!("{} needs {}, and this is {}", what, wanted.describe(), f.describe()),
                );
            }
        }
        Ok(k)
    }

    /// The kind that two values have in common, or an error that says they have none.
    fn common(&self, pos: Pos, a: K, b: K, what: &str) -> R<K> {
        match (a, b) {
            (K::Fixed(x), K::Fixed(y)) if !x.compatible(y) => fail(
                pos,
                format!(
                    "{} {} and {}, which have no common type",
                    what,
                    x.describe(),
                    y.describe()
                ),
            ),
            // A literal gives way to the kind that it meets.
            // Two literals keep the wider of their kinds, which a later value must still fit.
            (K::Fixed(Fixed::Num(a, true)), K::Fixed(Fixed::Num(b, true))) => {
                Ok(K::Fixed(Fixed::Num(if widens(a, b) { b } else { a }, true)))
            }
            (K::Fixed(Fixed::Num(_, true)), K::Fixed(y)) => Ok(K::Fixed(y)),
            (K::Fixed(x), _) | (_, K::Fixed(x)) => Ok(K::Fixed(x)),
            _ => Ok(K::Any),
        }
    }

    fn expr(&mut self, expr: &Expr) -> R<K> {
        let pos = expr.pos;
        let fixed = |f: Fixed| Ok(K::Fixed(f));
        match &expr.kind {
            ExprKind::Values => {
                if self.closures > 0 {
                    return fail(pos, "#values cannot be used inside a closure");
                }
                if self.values_used {
                    return fail(pos, "#values can only be used once, as it is read only once");
                }
                self.values_used = true;
                Ok(K::Iter)
            }
            ExprKind::Var(name) => {
                if self.bound(name) {
                    Ok(K::Any)
                } else if matches!(name.as_str(), "true" | "false") {
                    fixed(Fixed::Bool)
                } else if self.lenient {
                    Ok(K::Any)
                } else if name == "null" {
                    fixed(Fixed::Ctor)
                } else {
                    fail(
                        pos,
                        format!(
                            "{} is not bound by any pattern; if it is a const, supply the types that declare it",
                            name
                        ),
                    )
                }
            }
            // The kind of a const is only known from the types, which the typed check consults.
            ExprKind::Const(name) => {
                if self.consts {
                    Ok(K::Any)
                } else {
                    fail(
                        pos,
                        format!(
                            "{} is a const, which needs the types; supply the types that declare it",
                            name
                        ),
                    )
                }
            }
            ExprKind::Lit(lit) => fixed(match lit {
                Lit::Nat(_) => Fixed::Num(NumKind::Nat, true),
                Lit::Int(_) => Fixed::Num(NumKind::Int, true),
                Lit::Float(_) => Fixed::Num(NumKind::Float, true),
                Lit::Str(_) => Fixed::Str,
            }),
            ExprKind::Ctor { name, body } => {
                match body {
                    None => {}
                    Some(CtorBody::Record { fields, base }) => {
                        for f in fields {
                            self.value(&f.value)?;
                        }
                        if let Some(base) = base {
                            self.expect(base, Fixed::Ctor, "The base of a struct update")?;
                        }
                    }
                    Some(CtorBody::Tuple(args)) => {
                        for a in args {
                            self.value(&a.value)?;
                        }
                    }
                    Some(CtorBody::Array(items)) => self.elements(items)?,
                }
                if body.is_none() && matches!(canonical(name), "true" | "false") {
                    fixed(Fixed::Bool)
                } else {
                    fixed(Fixed::Ctor)
                }
            }
            ExprKind::Record { fields, base } => {
                for f in fields {
                    self.value(&f.value)?;
                }
                if let Some(base) = base {
                    self.expect(base, Fixed::Record, "The base of a struct update")?;
                }
                fixed(Fixed::Record)
            }
            ExprKind::Array(items) => {
                self.elements(items)?;
                fixed(Fixed::Array)
            }
            ExprKind::Field(of, _) | ExprKind::Position(of, _) => {
                let k = self.value(of)?;
                if let K::Fixed(f @ (Fixed::Num(..) | Fixed::Str | Fixed::Bool | Fixed::Array)) = k {
                    return fail(pos, format!("{} has no fields or arguments", capitalise(&f.describe())));
                }
                Ok(K::Any)
            }
            ExprKind::Index(of, index) => {
                let k = self.value(of)?;
                if let K::Fixed(f) = k {
                    if f != Fixed::Array && f != Fixed::Ctor {
                        return fail(pos, format!("{} cannot be indexed", capitalise(&f.describe())));
                    }
                }
                self.expect(index, Fixed::Num(NumKind::Nat, false), "An index")?;
                Ok(K::Any)
            }
            ExprKind::Method { receiver, name, args } => self.method(receiver, name, args, pos),
            ExprKind::Closure(_) => fail(pos, "A closure can only be the argument of a method"),
            ExprKind::Unary(UnOp::Not, operand) => {
                self.expect(operand, Fixed::Bool, "!")?;
                fixed(Fixed::Bool)
            }
            ExprKind::Unary(UnOp::Neg, operand) => {
                let k = self.value(operand)?;
                match k {
                    K::Fixed(Fixed::Num(NumKind::Nat, true)) => fixed(Fixed::Num(NumKind::Int, true)),
                    K::Fixed(Fixed::Num(..)) | K::Any => Ok(k),
                    other => fail(pos, format!("- needs a number, and this is {}", describe_k(other))),
                }
            }
            ExprKind::Binary(op, left, right) => self.binary(*op, left, right, pos),
            ExprKind::Cast(operand, kind) => {
                let k = self.value(operand)?;
                if let K::Fixed(f) = k {
                    if !matches!(f, Fixed::Num(..)) {
                        return fail(pos, format!("Only a number can be cast, and this is {}", f.describe()));
                    }
                }
                fixed(Fixed::Num(*kind, false))
            }
            ExprKind::If { branches, otherwise } => {
                let mut result = K::Any;
                let mut first = true;
                for (cond, then) in branches {
                    self.expect(cond, Fixed::Bool, "A condition")?;
                    let k = self.value(then)?;
                    result = if first {
                        k
                    } else {
                        self.common(then.pos, result, k, "The branches give")?
                    };
                    first = false;
                }
                let k = self.value(otherwise)?;
                self.common(otherwise.pos, result, k, "The branches give")
            }
            ExprKind::Match { scrutinee, arms } => {
                self.value(scrutinee)?;
                let mut result: Option<K> = None;
                for arm in arms {
                    let mark = self.names.len();
                    self.pattern(&arm.pattern)?;
                    if let Some(guard) = &arm.guard {
                        self.expect(guard, Fixed::Bool, "A guard")?;
                    }
                    let k = self.value(&arm.body)?;
                    self.names.truncate(mark);
                    result = Some(match result {
                        None => k,
                        Some(r) => self.common(arm.body.pos, r, k, "The arms give")?,
                    });
                }
                Ok(result.unwrap_or(K::Any))
            }
            ExprKind::Matches(subject, pattern) => {
                self.value(subject)?;
                let mark = self.names.len();
                self.pattern(pattern)?;
                self.names.truncate(mark);
                fixed(Fixed::Bool)
            }
            ExprKind::Assign { .. } => fail(
                pos,
                "An assignment is only allowed as the body of the closure of update",
            ),
        }
    }

    fn elements(&mut self, items: &[Expr]) -> R<()> {
        let mut common = K::Any;
        for item in items {
            let k = self.value(item)?;
            common = self.common(item.pos, common, k, "The elements of an array are")?;
        }
        Ok(())
    }

    fn binary(&mut self, op: BinOp, left: &Expr, right: &Expr, pos: Pos) -> R<K> {
        if matches!(op, BinOp::And | BinOp::Or) {
            self.expect(left, Fixed::Bool, op.symbol())?;
            self.expect(right, Fixed::Bool, op.symbol())?;
            return Ok(K::Fixed(Fixed::Bool));
        }
        let a = self.value(left)?;
        let b = self.value(right)?;
        let both = self.common(pos, a, b, &format!("{} is given", op.symbol()))?;
        if op.is_arithmetic() {
            let allowed = |k: K| match k {
                K::Fixed(Fixed::Num(..)) | K::Any => true,
                K::Fixed(Fixed::Str) => op == BinOp::Add,
                _ => false,
            };
            for (k, e) in [(a, left), (b, right)] {
                if !allowed(k) {
                    return fail(
                        e.pos,
                        format!("{} needs numbers, and this is {}", op.symbol(), describe_k(k)),
                    );
                }
            }
            // A result of two literals is still a literal.
            return Ok(match (a, b, both) {
                (K::Fixed(Fixed::Num(_, true)), K::Fixed(Fixed::Num(_, true)), K::Fixed(Fixed::Num(k, _))) => {
                    K::Fixed(Fixed::Num(k, true))
                }
                (_, _, K::Fixed(Fixed::Num(k, _))) => K::Fixed(Fixed::Num(k, false)),
                (_, _, k) => k,
            });
        }
        if op.is_ordering() {
            for (k, e) in [(a, left), (b, right)] {
                if let K::Fixed(f) = k {
                    if !matches!(f, Fixed::Num(..) | Fixed::Str) {
                        return fail(
                            e.pos,
                            format!(
                                "{} compares numbers or strings, and this is {}",
                                op.symbol(),
                                f.describe()
                            ),
                        );
                    }
                }
            }
        }
        Ok(K::Fixed(Fixed::Bool))
    }

    fn method(&mut self, receiver: &Expr, name: &str, args: &[Expr], pos: Pos) -> R<K> {
        let r = self.expr(receiver)?;
        let found = if r == K::Iter {
            stream_method(name)
        } else {
            value_method(name)
        };
        let Some((arg, result)) = found else {
            let what = match r {
                K::Iter => "An iterator".to_string(),
                K::Fixed(f) => capitalise(&f.describe()),
                K::Any => "A value".to_string(),
            };
            let hint = if name == "chars" && r != K::Iter {
                "; Constructo has no character type yet, so use char_strings(), which gives strings"
            } else if r != K::Iter && stream_method(name).is_some() {
                "; use .iter() first for the elements of an array"
            } else {
                ""
            };
            return fail(pos, format!("{} has no method {}{}", what, name, hint));
        };
        // A receiver whose kind the expression fixes must be one that has the method.
        if let K::Fixed(f) = r {
            if !receiver_has(name, f) {
                return fail(pos, format!("{} has no method {}", capitalise(&f.describe()), name));
            }
        }
        let count = if arg == Arg::None { 0 } else { 1 };
        if args.len() != count {
            return fail(
                pos,
                format!("{} takes {} argument(s), and {} are given", name, count, args.len()),
            );
        }
        match arg {
            Arg::None => {}
            Arg::Nat => {
                self.expect(&args[0], Fixed::Num(NumKind::Nat, false), name)?;
            }
            Arg::Str => {
                self.expect(&args[0], Fixed::Str, name)?;
            }
            Arg::Value => {
                self.value(&args[0])?;
            }
            Arg::Closure | Arg::RefutableClosure | Arg::Update => {
                let ExprKind::Closure(closure) = &args[0].kind else {
                    return fail(args[0].pos, format!("{} takes a closure, as in |v| ...", name));
                };
                return self.closure(closure, name, arg, result, r);
            }
        }
        Ok(result)
    }

    fn closure(&mut self, closure: &Closure, method: &str, arg: Arg, result: K, receiver: K) -> R<K> {
        if closure.params.len() != 1 {
            return fail(closure.pos, format!("The closure of {} takes one parameter", method));
        }
        let param = &closure.params[0];
        if arg != Arg::RefutableClosure && !param.is_irrefutable() {
            return fail(
                param.pos,
                format!(
                    "The parameter of {}'s closure must always match, as in Rust; use filter_match to skip the \
                     values that do not match, or match inside the closure",
                    method
                ),
            );
        }
        let mark = self.names.len();
        self.pattern(param)?;
        self.closures += 1;
        let body = if arg == Arg::Update {
            self.assignment(&closure.body, param)
        } else {
            self.expr(&closure.body)
        };
        self.closures -= 1;
        self.names.truncate(mark);
        let body = body?;
        match method {
            "filter" | "any" | "all" | "find" | "filter_map" if body == K::Iter => {
                let wanted = if method == "filter_map" { "an Option" } else { "a Bool" };
                return fail(
                    closure.body.pos,
                    format!(
                        "The closure of {} must give {}, and this is an iterator",
                        method, wanted
                    ),
                );
            }
            "filter" | "any" | "all" | "find" => {
                if let K::Fixed(f) = body {
                    if f != Fixed::Bool {
                        return fail(
                            closure.body.pos,
                            format!(
                                "The closure of {} must give a Bool, and this is {}",
                                method,
                                f.describe()
                            ),
                        );
                    }
                }
            }
            "filter_map" => {
                if let K::Fixed(f) = body {
                    if f != Fixed::Ctor {
                        return fail(
                            closure.body.pos,
                            format!(
                                "The closure of filter_map must give an Option, and this is {}",
                                f.describe()
                            ),
                        );
                    }
                }
            }
            "flat_map" => {
                if let K::Fixed(f) = body {
                    if f != Fixed::Array {
                        return fail(
                            closure.body.pos,
                            format!(
                                "The closure of flat_map must give an iterator or an array, and this is {}",
                                f.describe()
                            ),
                        );
                    }
                }
            }
            "map" | "filter_match" if body == K::Iter => {
                return fail(
                    closure.body.pos,
                    format!(
                        "The closure of {} gives an iterator; use flat_map, or .collect() to make an array",
                        method
                    ),
                );
            }
            _ => {}
        }
        Ok(if arg == Arg::Update { receiver } else { result })
    }

    /// The body of the closure of `update`: an assignment to a path from the parameter.
    fn assignment(&mut self, body: &Expr, param: &Pattern) -> R<K> {
        let ExprKind::Assign { target, op, value } = &body.kind else {
            return fail(
                body.pos,
                "The closure of update must assign to a path, as in |v| v.x = 1",
            );
        };
        let PatternKind::Bind(param_name) = &param.kind else {
            return fail(
                param.pos,
                "The parameter of update's closure is a name, as in |v| v.x = 1",
            );
        };
        let mut at = &**target;
        let mut depth = 0;
        loop {
            match &at.kind {
                ExprKind::Field(next, _) | ExprKind::Position(next, _) => at = next,
                ExprKind::Index(next, index) => {
                    self.expect(index, Fixed::Num(NumKind::Nat, false), "An index")?;
                    at = next;
                }
                ExprKind::Var(name) if name == param_name && depth > 0 => break,
                ExprKind::Var(name) if name == param_name => {
                    return fail(
                        at.pos,
                        "An assignment changes a part of the value, as in v.x = 1, not the whole of it",
                    )
                }
                _ => {
                    return fail(
                        at.pos,
                        format!(
                            "An assignment changes a path from {}, as in {}.a.b[0]",
                            param_name, param_name
                        ),
                    )
                }
            }
            depth += 1;
        }
        let v = self.value(value)?;
        if let Some(op) = op {
            if let K::Fixed(f) = v {
                let ok = matches!(f, Fixed::Num(..)) || (*op == BinOp::Add && f == Fixed::Str);
                if !ok {
                    return fail(
                        value.pos,
                        format!("{}= needs a number, and this is {}", op.symbol(), f.describe()),
                    );
                }
            }
        }
        Ok(K::Any)
    }

    fn pattern(&mut self, pattern: &Pattern) -> R<()> {
        match &pattern.kind {
            PatternKind::Wild | PatternKind::Lit(_) => Ok(()),
            PatternKind::Bind(name) => {
                self.names.push(name.clone());
                Ok(())
            }
            PatternKind::At(name, inner) => {
                self.pattern(inner)?;
                self.names.push(name.clone());
                Ok(())
            }
            PatternKind::Or(alternatives) => {
                // Every alternative must bind the same names, as in Rust.
                let mark = self.names.len();
                let mut first: Option<Vec<String>> = None;
                for p in alternatives {
                    self.pattern(p)?;
                    let mut bound: Vec<String> = self.names.drain(mark..).collect();
                    bound.sort();
                    match &first {
                        None => first = Some(bound),
                        Some(f) if *f != bound => {
                            return fail(p.pos, "Every alternative of a pattern must bind the same names")
                        }
                        Some(_) => {}
                    }
                }
                self.names.extend(first.unwrap_or_default());
                Ok(())
            }
            PatternKind::Ctor { body, .. } => match body {
                None => Ok(()),
                Some(PatBody::Record { fields, .. }) => fields.iter().try_for_each(|f| self.pattern(&f.pattern)),
                Some(PatBody::Tuple { args, .. }) => args.iter().try_for_each(|a| self.pattern(&a.pattern)),
                Some(PatBody::Array { items, .. }) => items.iter().try_for_each(|p| self.pattern(p)),
            },
            PatternKind::Record { fields, .. } => fields.iter().try_for_each(|f| self.pattern(&f.pattern)),
            PatternKind::Array { items, .. } => items.iter().try_for_each(|p| self.pattern(p)),
        }
    }
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
