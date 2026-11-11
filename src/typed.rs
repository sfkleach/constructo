//! The encoding of a type-checked value as natural JSON, for `convert --to=json-typed`.
//!
//! The shapes are those that serde writes for the Rust types that these declarations would be: a bare constructor is
//! a string, any other is an object with the constructor for its one key, and `Bool` and `Option` of the prelude are
//! `true`, `false`, `null` and the value itself. The types decide what is written, which is why the value is checked
//! first and the encoder is given what the check found out.

use crate::ast::{TaggedBody, Value, ValueKind, VariantBody};
use crate::convert::Json;
use crate::diagnostics::Pos;
use crate::project::PRELUDE_PROJECT;
use crate::typecheck::{bind_arguments, check_value_as, instantiate, instantiate_fields, Env, Ty, TypeError, Typed};
use std::cell::Cell;

/// The most levels of values, and of the consts that they refer to, that are encoded. The parser limits the nesting of
/// a value to 200, but a const can refer to another, and a long chain of them would otherwise be as deep as it is long.
/// Each level takes a few kilobytes of stack in a debug build, so the limit is chosen to stay well inside the stack of
/// a test thread, which is the smallest that the encoder runs on.
const MAX_NESTING: usize = 300;

/// The most nodes that the encoding of one value may have. Depth is limited by `MAX_NESTING`, but a const that is
/// referred to twice by another const is written out in full each time, so the size can double at every level.
const MAX_NODES: usize = 1_000_000;

/// The names of the types of the prelude that are written specially, which are told by the project that declares them
/// and so are not mistaken for the types of a user that happen to have the same names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prelude {
    Bool,
    Option,
}

/// The name without the project, which is how a declaration is written in the output: `Circle@shapes` is `Circle`.
fn bare(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

fn prelude_kind(enum_name: &str) -> Option<Prelude> {
    let (local, project) = enum_name.split_once('@')?;
    if project != PRELUDE_PROJECT {
        return None;
    }
    match local {
        "Bool" => Some(Prelude::Bool),
        "Option" => Some(Prelude::Option),
        _ => None,
    }
}

fn error<T>(pos: Pos, message: impl Into<String>) -> Result<T, TypeError> {
    Err(TypeError {
        pos,
        message: message.into(),
    })
}

/// The text of a number as it is written in JSON. A `+` is dropped and so are the zeros in front of the whole part,
/// which JSON does not allow, and a number that fills a `Float` slot and shows no point or exponent gets `.0`, so that
/// a typed reader sees a float. Nothing is parsed, so a number of any size is kept as it is.
pub fn number_text(text: &str, float: bool) -> String {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let whole_end = digits.find(|c: char| !c.is_ascii_digit()).unwrap_or(digits.len());
    let (whole, rest) = digits.split_at(whole_end);
    let whole = match whole.trim_start_matches('0') {
        "" => "0",
        trimmed => trimmed,
    };
    let mut out = String::new();
    if negative {
        out.push('-');
    }
    out.push_str(whole);
    out.push_str(rest);
    if float && !rest.contains(['.', 'e', 'E']) {
        out.push_str(".0");
    }
    out
}

/// Encodes a value that has been checked. `widening` and `infer_limit` are the options that it was checked with, which
/// are used again to check the value of a const where it is used.
pub fn encode_typed(typed: &Typed, widening: bool, infer_limit: usize, value: &Value) -> Result<Json, TypeError> {
    Encoder {
        widening,
        infer_limit,
        budget: Cell::new(MAX_NODES),
    }
    .encode(typed, value, 0)
}

struct Encoder {
    widening: bool,
    infer_limit: usize,
    /// The nodes that may still be encoded, which every reference to a const is charged against.
    budget: Cell<usize>,
}

impl Encoder {
    fn encode(&self, typed: &Typed, node: &Value, depth: usize) -> Result<Json, TypeError> {
        if depth > MAX_NESTING {
            return error(node.pos, "The value is nested too deeply to convert");
        }
        let Some(left) = self.budget.get().checked_sub(1) else {
            return error(node.pos, "The value is too large to convert");
        };
        self.budget.set(left);
        let ty = typed.ty(node);
        Ok(match &node.kind {
            ValueKind::Nat(text) | ValueKind::Int(text) | ValueKind::Float(text) => {
                Json::Number(number_text(text, matches!(ty, Some(Ty::Float))))
            }
            ValueKind::Str(text) => Json::Str(text.clone()),
            ValueKind::Array(items) => self.array(typed, items, depth)?,
            ValueKind::Record(members) => {
                // The order is that of the type, which is the declaration for a field that has a declared type and the
                // order written for a record that decided its own type.
                let order: Vec<&str> = match &ty {
                    Some(Ty::Record(fields)) => fields.iter().map(|(name, _)| name.as_str()).collect(),
                    _ => members.iter().map(|m| m.name.as_str()).collect(),
                };
                let mut object = Vec::new();
                for name in order {
                    if let Some(member) = members.iter().find(|m| m.name == name) {
                        object.push((name.to_string(), self.encode(typed, &member.value, depth + 1)?));
                    }
                }
                Json::Object(object)
            }
            // A name that starts with a capital letter is a constructor, and any other name is a const (decision 35).
            ValueKind::Tagged { name, body } if name.starts_with(|c: char| c.is_ascii_uppercase()) => {
                self.constructor(typed, node, name, body.as_ref(), ty, depth)?
            }
            ValueKind::Tagged { name, .. } => self.constant(typed, node, name, depth)?,
        })
    }

    fn array(&self, typed: &Typed, items: &[Value], depth: usize) -> Result<Json, TypeError> {
        let mut out = Vec::with_capacity(items.len());
        for item in items {
            out.push(self.encode(typed, item, depth + 1)?);
        }
        Ok(Json::Array(out))
    }

    /// A reference to a const is written as the value of the const. The value is checked again, against the type that
    /// the reference has, so that it is written for that type: `five` where a `Float` is expected is `5.0`.
    fn constant(&self, typed: &Typed, node: &Value, name: &str, depth: usize) -> Result<Json, TypeError> {
        let Some(constant) = typed.env().constant(name) else {
            return error(node.pos, format!("Unknown const {}", bare(name)));
        };
        let Some(scheme) = typed.scheme(node, self.infer_limit) else {
            return error(
                node.pos,
                format!("The type of const {} is too large to convert", bare(name)),
            );
        };
        let again = check_value_as(typed.env(), self.widening, &constant.value, &scheme).map_err(|e| TypeError {
            pos: node.pos,
            message: format!("Cannot convert const {}: {}", bare(name), e.message),
        })?;
        self.encode(&again, &constant.value, depth + 1)
    }

    fn constructor(
        &self,
        typed: &Typed,
        node: &Value,
        name: &str,
        body: Option<&TaggedBody>,
        ty: Option<Ty>,
        depth: usize,
    ) -> Result<Json, TypeError> {
        let (Some((decl, variant)), Some(Ty::Enum(_, args))) = (typed.env().constructor(name), ty) else {
            return error(
                node.pos,
                format!("Cannot work out the type of constructor {}", bare(name)),
            );
        };
        let local = bare(&variant.name);
        match prelude_kind(&decl.name) {
            Some(Prelude::Bool) => return Ok(Json::Bool(local == "True")),
            Some(Prelude::Option) if local == "None" => return Ok(Json::Null),
            Some(Prelude::Option) => {
                let Some(TaggedBody::Tuple(arguments)) = body else {
                    return error(node.pos, "Some takes a value");
                };
                let inner = self.encode(typed, &arguments[0].value, depth + 1)?;
                // A `null` inside a `Some` would be read back as the `None` outside it, so it cannot be written.
                if inner == Json::Null {
                    return error(
                        node.pos,
                        "Some(None) cannot be written as JSON, since it would read back as None",
                    );
                }
                return Ok(inner);
            }
            None => {}
        }
        let wrap = |inner: Json| Json::Object(vec![(local.to_string(), inner)]);
        match (&variant.body, body) {
            (VariantBody::Unit, _) => Ok(Json::Str(local.to_string())),
            (VariantBody::Record(fields), Some(TaggedBody::Record(members))) => {
                let mut object = Vec::new();
                for (field, _) in instantiate_fields(fields, &decl.params, &args) {
                    let Some(member) = members.iter().find(|m| m.name == field) else {
                        return error(node.pos, format!("Missing field {} in {}", field, local));
                    };
                    object.push((field, self.encode(typed, &member.value, depth + 1)?));
                }
                Ok(wrap(Json::Object(object)))
            }
            (VariantBody::Tuple(parameters), Some(TaggedBody::Tuple(arguments))) => {
                let (targets, omitted) = bind_arguments(node.pos, &variant.name, parameters, arguments)?;
                let mut slots: Vec<Option<Json>> = vec![None; parameters.len()];
                for (argument, &index) in arguments.iter().zip(&targets) {
                    slots[index] = Some(self.encode(typed, &argument.value, depth + 1)?);
                }
                for index in omitted {
                    let ty = instantiate(&parameters[index].ty, &decl.params, &args);
                    slots[index] = Some(self.default(typed, &ty, node.pos)?);
                }
                let mut values: Vec<Json> = slots.into_iter().flatten().collect();
                // A tuple with one parameter is written as its value, as serde writes a newtype variant.
                let inner = if values.len() == 1 {
                    values.remove(0)
                } else {
                    Json::Array(values)
                };
                Ok(wrap(inner))
            }
            (VariantBody::Array(_), Some(TaggedBody::Array(items))) => Ok(wrap(self.array(typed, items, depth)?)),
            _ => error(node.pos, format!("Constructor {} has the wrong form of body", local)),
        }
    }

    /// The default value of a type, which is what an omitted argument is: the zero of a number, the empty string
    /// and the empty array, the record of the defaults of its fields, and the first constructor of an enum with
    /// the defaults of its fields. It is built with a stack of its own, since a chain of enums that each start
    /// with the next can be as deep as the number of declarations. The check has counted the nodes against the
    /// same limit, so this does not fail for that reason, and the limit is only a defensive check here.
    fn default(&self, typed: &Typed, ty: &Ty, pos: Pos) -> Result<Json, TypeError> {
        enum Step {
            Visit(Ty),
            /// Takes the results of the fields, which are on the stack of results, into an object.
            Object(Vec<String>),
            /// Takes the results of the fields into the object of a constructor, as a record or as a tuple.
            Variant(String, Vec<String>, bool),
        }
        let env: &Env = typed.env();
        let mut remaining = env.default_limit();
        let mut results: Vec<Json> = Vec::new();
        let mut stack = vec![Step::Visit(ty.clone())];
        while let Some(step) = stack.pop() {
            match step {
                Step::Visit(ty) => {
                    let ty = typed.shallow(&ty);
                    if remaining == 0 {
                        return error(pos, "The default value of an omitted argument is too large");
                    }
                    remaining -= 1;
                    match &ty {
                        Ty::Nat | Ty::Int => results.push(Json::Number("0".to_string())),
                        Ty::Float => results.push(Json::Number("0.0".to_string())),
                        Ty::Str => results.push(Json::Str(String::new())),
                        Ty::Array(_) => results.push(Json::Array(Vec::new())),
                        Ty::Record(fields) => {
                            stack.push(Step::Object(fields.iter().map(|(name, _)| name.clone()).collect()));
                            stack.extend(fields.iter().rev().map(|(_, field)| Step::Visit(field.clone())));
                        }
                        Ty::Enum(name, args) => {
                            let first = env
                                .declared_enum(name)
                                .and_then(|decl| decl.variants.first().map(|v| (decl, v)));
                            let Some((decl, variant)) = first else {
                                return error(pos, format!("Cannot work out the default value of {}", bare(name)));
                            };
                            let local = bare(&variant.name);
                            match prelude_kind(&decl.name) {
                                Some(Prelude::Bool) => {
                                    results.push(Json::Bool(local == "True"));
                                    continue;
                                }
                                Some(Prelude::Option) => {
                                    results.push(Json::Null);
                                    continue;
                                }
                                None => {}
                            }
                            match &variant.body {
                                VariantBody::Unit => results.push(Json::Str(local.to_string())),
                                VariantBody::Array(_) => {
                                    results.push(Json::Object(vec![(local.to_string(), Json::Array(Vec::new()))]))
                                }
                                VariantBody::Record(fields) => {
                                    let fields = instantiate_fields(fields, &decl.params, args);
                                    let names = fields.iter().map(|(name, _)| name.clone()).collect();
                                    stack.push(Step::Variant(local.to_string(), names, true));
                                    stack.extend(fields.into_iter().rev().map(|(_, field)| Step::Visit(field)));
                                }
                                VariantBody::Tuple(parameters) => {
                                    let types: Vec<Ty> = parameters
                                        .iter()
                                        .map(|p| instantiate(&p.ty, &decl.params, args))
                                        .collect();
                                    stack.push(Step::Variant(
                                        local.to_string(),
                                        vec![String::new(); types.len()],
                                        false,
                                    ));
                                    stack.extend(types.into_iter().rev().map(Step::Visit));
                                }
                            }
                        }
                        _ => return error(pos, "Cannot work out the default value of an omitted argument"),
                    }
                }
                Step::Object(names) => {
                    let values = results.split_off(results.len() - names.len());
                    results.push(Json::Object(names.into_iter().zip(values).collect()));
                }
                Step::Variant(local, names, record) => {
                    let mut values = results.split_off(results.len() - names.len());
                    let inner = if record {
                        Json::Object(names.into_iter().zip(values).collect())
                    } else if values.len() == 1 {
                        values.remove(0)
                    } else {
                        Json::Array(values)
                    };
                    results.push(Json::Object(vec![(local, inner)]));
                }
            }
        }
        Ok(results.pop().expect("the default has been built"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{parse_file, Experimental, Parsed};
    use crate::typecheck::{check_consts, check_value_typed, DEFAULT_INFER_LIMIT};
    use crate::{plain_layout, prelude_source, resolve, Source};

    /// Checks and encodes each value of the text with the declarations, in the way that a run does, with the prelude
    /// unless it is left out, and gives the compact JSON of each, or the message of the first error.
    fn run(types: &str, values: &str, prelude: bool) -> Result<Vec<String>, String> {
        let mut sources = Vec::new();
        if prelude {
            sources.push(prelude_source());
        }
        sources.push(Source {
            name: "types".to_string(),
            text: types.to_string(),
        });
        sources.push(Source {
            name: "values".to_string(),
            text: values.to_string(),
        });
        let mut parsed: Vec<Parsed> = sources
            .iter()
            .map(|s| {
                parse_file(&s.text, Experimental::all())
                    .1
                    .expect("the text should parse")
            })
            .collect();
        let layout = plain_layout(&sources);
        let resolution = resolve::resolve(&sources, &mut parsed, &layout);
        assert!(resolution.diagnostics.is_empty(), "{:?}", resolution.diagnostics);
        assert!(resolution.value_errors.is_empty(), "{:?}", resolution.value_errors);
        let mut enums = Vec::new();
        let mut consts = Vec::new();
        let mut found = Vec::new();
        for parsed in &parsed {
            match parsed {
                Parsed::Types(file) => {
                    enums.extend(&file.enums);
                    consts.extend(&file.consts);
                }
                Parsed::Values(values) => found.extend(values),
            }
        }
        let mut env = Env::new(enums, consts.iter().copied());
        let with_source: Vec<_> = consts.iter().map(|c| (0, *c)).collect();
        let errors = check_consts(&mut env, true, DEFAULT_INFER_LIMIT, &with_source);
        assert!(errors.is_empty(), "{:?}", errors);
        let mut out = Vec::new();
        for value in found {
            let typed = check_value_typed(&env, true, value).map_err(|e| e.message)?;
            let json = encode_typed(&typed, true, DEFAULT_INFER_LIMIT, value).map_err(|e| e.message)?;
            out.push(json.write(true).trim_end().to_string());
        }
        Ok(out)
    }

    fn json(types: &str, values: &str) -> String {
        run(types, values, true).expect("the values should convert").join(" ")
    }

    const SHAPES: &str = "enum Shape = Circle { r: Float }, Dot, Pair(Nat, Nat), Wrapper(Nat), Ping(), Items[Nat], \
                          Tagged(label: String, size: Float);";

    #[test]
    fn numbers_are_written_as_json_numbers() {
        assert_eq!(number_text("5", false), "5");
        assert_eq!(number_text("+5", false), "5");
        assert_eq!(number_text("-5", false), "-5");
        assert_eq!(number_text("007", false), "7");
        assert_eq!(number_text("-007", false), "-7");
        assert_eq!(number_text("000", false), "0");
        assert_eq!(number_text("00.5", true), "0.5");
        assert_eq!(number_text("1.50", true), "1.50");
        assert_eq!(number_text("1.5E+2", true), "1.5E+2");
        assert_eq!(number_text("-2.5e3", true), "-2.5e3");
        assert_eq!(number_text("5", true), "5.0");
        assert_eq!(number_text("-5", true), "-5.0");
        assert_eq!(number_text("+5", true), "5.0");
        // A number is never parsed, so a size that no double holds is kept.
        assert_eq!(
            number_text("123456789012345678901234567890", false),
            "123456789012345678901234567890"
        );
    }

    #[test]
    fn the_basic_values() {
        assert_eq!(json("", "5"), "5");
        assert_eq!(json("", "+5"), "5");
        assert_eq!(json("", "-5"), "-5");
        assert_eq!(json("", "1.50"), "1.50");
        assert_eq!(json("", "\"hi\\n\""), "\"hi\\n\"");
        assert_eq!(json("", "[1, 2]"), "[1,2]");
        assert_eq!(json("", "[]"), "[]");
        assert_eq!(json("", "{ b: 1, a: \"x\" }"), "{\"b\":1,\"a\":\"x\"}");
        assert_eq!(json("", "{}"), "{}");
    }

    #[test]
    fn a_number_is_written_for_the_slot_that_it_fills() {
        assert_eq!(json("", "[1, 2.5]"), "[1.0,2.5]");
        assert_eq!(json(SHAPES, "Circle { r: 1 }"), "{\"Circle\":{\"r\":1.0}}");
        assert_eq!(json(SHAPES, "Circle { r: 007 }"), "{\"Circle\":{\"r\":7.0}}");
        assert_eq!(json("enum I = I(Int);", "I(+5)"), "{\"I\":5}");
    }

    #[test]
    fn bool_and_option_are_json_values() {
        let types = "enum Flags = Flags { on: Bool, name: Option<String> };";
        assert_eq!(json(types, "true"), "true");
        assert_eq!(json(types, "false"), "false");
        assert_eq!(json(types, "True"), "true");
        assert_eq!(json(types, "None"), "null");
        assert_eq!(json(types, "null"), "null");
        assert_eq!(json(types, "Some(1)"), "1");
        assert_eq!(
            json(types, "Flags { on: true, name: None }"),
            "{\"Flags\":{\"on\":true,\"name\":null}}"
        );
        assert_eq!(
            json(types, "Flags { name: Some(\"x\"), on: False }"),
            "{\"Flags\":{\"on\":false,\"name\":\"x\"}}"
        );
    }

    #[test]
    fn an_option_inside_an_option_is_written_unwrapped_unless_it_would_be_lost() {
        assert_eq!(json("", "Some(Some(1))"), "1");
        let error = run("", "Some(None)", true).unwrap_err();
        assert_eq!(
            error,
            "Some(None) cannot be written as JSON, since it would read back as None"
        );
        // The `None` may come from a const.
        assert!(run("", "Some(null)", true).is_err());
        assert!(run("", "Some(Some(None))", true).is_err());
    }

    #[test]
    fn the_shapes_of_the_constructors() {
        assert_eq!(json(SHAPES, "Dot"), "\"Dot\"");
        assert_eq!(json(SHAPES, "Circle { r: 1.0 }"), "{\"Circle\":{\"r\":1.0}}");
        assert_eq!(json(SHAPES, "Pair(1, 2)"), "{\"Pair\":[1,2]}");
        assert_eq!(json(SHAPES, "Wrapper(5)"), "{\"Wrapper\":5}");
        assert_eq!(json(SHAPES, "Ping()"), "{\"Ping\":[]}");
        assert_eq!(json(SHAPES, "Items[1, 2]"), "{\"Items\":[1,2]}");
        assert_eq!(json(SHAPES, "Items[]"), "{\"Items\":[]}");
        assert_eq!(json("enum Speed = Fast, Slow;", "[Fast, Slow]"), "[\"Fast\",\"Slow\"]");
    }

    #[test]
    fn arguments_are_written_in_the_order_of_the_declaration() {
        assert_eq!(
            json(SHAPES, "Tagged(size: 2, label: \"a\")"),
            "{\"Tagged\":[\"a\",2.0]}"
        );
        assert_eq!(
            json("enum Box = Box { w: Nat, h: Nat };", "Box { h: 1, w: 2 }"),
            "{\"Box\":{\"w\":2,\"h\":1}}"
        );
    }

    #[test]
    fn an_omitted_argument_is_written_as_its_default() {
        let types = "enum Speed = Fast, Slow;
                     enum Inner = In { n: Nat, m: Float };
                     enum Pt = Pt(x: Nat, y: Nat);
                     enum P = P(a: Nat, b: Int, c: Float, d: String, e: [Nat], f: Option<Nat>, g: Bool, h: Speed, \
                     i: { x: Float, s: String }, j: Inner, k: Pt);";
        assert_eq!(
            json(types, "P()"),
            "{\"P\":[0,0,0.0,\"\",[],null,false,\"Fast\",{\"x\":0.0,\"s\":\"\"},{\"In\":{\"n\":0,\"m\":0.0}},\
             {\"Pt\":[0,0]}]}"
        );
        assert!(json(types, "P(a: 7, d: \"z\")").starts_with("{\"P\":[7,0,0.0,\"z\",[]"));
    }

    #[test]
    fn the_default_of_an_enum_with_one_parameter_is_unwrapped_and_an_array_variant_is_empty() {
        let types = "enum Item = Item[Nat]; enum One = One(Nat); enum W = W(a: One, b: Item, c: Ping2); \
                     enum Ping2 = Ping2();";
        assert_eq!(json(types, "W()"), "{\"W\":[{\"One\":0},{\"Item\":[]},{\"Ping2\":[]}]}");
    }

    #[test]
    fn a_const_is_written_as_its_value() {
        let types = "enum Box = Box(Float); enum Two = Two(Option<Nat>, Option<String>); const five = 5; \
                     const names = [\"a\", \"b\"]; const origin = Box(five);";
        assert_eq!(json(types, "five"), "5");
        assert_eq!(json(types, "names"), "[\"a\",\"b\"]");
        // The value is written for the type of the place where it is used.
        assert_eq!(json(types, "Box(five)"), "{\"Box\":5.0}");
        assert_eq!(json(types, "origin"), "{\"Box\":5.0}");
        assert_eq!(json(types, "[five, 2.5]"), "[5.0,2.5]");
    }

    #[test]
    fn a_generic_const_is_written_for_each_use() {
        let types = "enum Two = Two(Option<Nat>, Option<String>);";
        assert_eq!(json(types, "Two(null, null)"), "{\"Two\":[null,null]}");
        let types = "enum Two = Two(Option<Nat>, Option<Float>); const maybe<T>: Option<T> = None; \
                     const some = Some(3);";
        assert_eq!(json(types, "Two(maybe, maybe)"), "{\"Two\":[null,null]}");
        assert_eq!(json(types, "Two(some, some)"), "{\"Two\":[3,3.0]}");
    }

    #[test]
    fn a_const_may_refer_to_another() {
        // Whether a const may refer to another is decided by the validity checks, which are not made here.
        let types = "enum Box = Box(Float); const a = 1; const b = Box(a); const c = [b, b];";
        assert_eq!(json(types, "b"), "{\"Box\":1.0}");
        assert_eq!(json(types, "c"), "[{\"Box\":1.0},{\"Box\":1.0}]");
    }

    #[test]
    fn a_long_chain_of_consts_is_refused_and_not_followed_to_the_end_of_the_stack() {
        let mut types = String::new();
        for i in 0..1500 {
            types.push_str(&format!("const c{} = c{};\n", i, i + 1));
        }
        types.push_str("const c1500 = 1;");
        let error = run(&types, "c0", true).unwrap_err();
        assert_eq!(error, "The value is nested too deeply to convert");
        // A chain that is not too long is followed.
        let mut types = String::new();
        for i in 0..100 {
            types.push_str(&format!("const c{} = c{};\n", i, i + 1));
        }
        types.push_str("const c100 = 1;");
        assert_eq!(json(&types, "c0"), "1");
    }

    #[test]
    fn the_types_of_a_user_that_are_called_bool_and_option_are_ordinary() {
        let types = "enum Bool = False, True; enum Option<T> = None, Some(T);";
        let out = run(types, "True; None; Some(5)", false).unwrap();
        assert_eq!(out, ["\"True\"", "\"None\"", "{\"Some\":5}"]);
    }

    #[test]
    fn the_prelude_types_are_told_by_their_project_and_not_by_their_names() {
        // A type of the user with the name of one of the prelude is a clash when the prelude is loaded, so it is
        // declared in a project of its own, with the prelude kept out of the group. Here the point is only that the
        // test above does not depend on the names, so the names are checked directly.
        assert_eq!(prelude_kind("Bool@constructo.prelude"), Some(Prelude::Bool));
        assert_eq!(prelude_kind("Option@constructo.prelude"), Some(Prelude::Option));
        assert_eq!(prelude_kind("Bool@shapes"), None);
        assert_eq!(prelude_kind("Bool"), None);
        assert_eq!(prelude_kind("Shape@constructo.prelude"), None);
    }

    #[test]
    fn a_long_chain_of_defaults_does_not_overflow_the_stack() {
        // Each enum starts with a constructor that has the next one as its default, so the default is as deep as the
        // chain is long, which is as long as the declarations are many.
        let depth = 2000;
        let mut types = String::new();
        for i in 0..depth {
            types.push_str(&format!("enum E{} = A{}(n: E{});\n", i, i, i + 1));
        }
        types.push_str(&format!("enum E{} = Last;\nenum Top = Top(e: E0);", depth));
        let out = json(&types, "Top()");
        assert!(out.starts_with("{\"Top\":{\"A0\":{\"A1\":"), "{}", &out[..40]);
        // One level for each enum of the chain, and one for Top.
        assert!(
            out.ends_with(&format!("\"Last\"{}", "}".repeat(depth + 1))),
            "{}",
            &out[out.len() - 20..]
        );
    }

    #[test]
    fn the_names_are_those_that_the_declarations_give() {
        assert_eq!(bare("Circle@com.example.shapes"), "Circle");
        assert_eq!(bare("Circle"), "Circle");
    }
}
