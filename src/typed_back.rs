//! The reader of `json-typed`: it turns the tree of a JSON document, as `json.rs` parses it, back into a value of a
//! type that is known.
//!
//! It is the reverse of the encoding in `typed.rs`, which is decision 37. The type says what the JSON means: a string
//! is a bare constructor for an enum and a string for `String`, an object with one key is a constructor with a body,
//! and `true`, `false` and `null` are `Bool` and `Option` of the prelude. The value that comes out is made of the
//! types that were read, and is checked once more against the type before it is given back, which is the proof that
//! what is written is what `check` accepts.

use crate::ast::{Argument, Member, TaggedBody, Value, ValueKind, VariantBody};
use crate::diagnostics::{Diagnostic, Pos};
use crate::json::{Kind, Member as JsonMember, Node};
use crate::project::PRELUDE_PROJECT;
use crate::typecheck::{check_value_as, instantiate, instantiate_fields, Scheme, Ty};
use crate::ReadContext;

/// How deep a type is shown in a message before the rest is left out.
const SHOWN_DEPTH: usize = 4;

/// Reads one value of the target type of the context from a JSON node. `file` is the name that diagnostics give, and
/// each part of the value has the position of the node that it came from. The value is checked against the type
/// before it is returned, with `widening` as the option of the run, so a problem that the shape of the JSON cannot
/// show, such as an omitted argument that has no default, is reported at the value.
pub fn read_typed(ctx: &ReadContext, node: &Node, file: &str, widening: bool) -> Result<Value, Diagnostic> {
    let mut value = Reader { ctx, file }.read(&ctx.target, node)?;
    let scheme = Scheme {
        ty: ctx.target.clone(),
        arity: 0,
        text: String::new(),
    };
    check_value_as(ctx.env, widening, &value, &scheme).map_err(|e| Diagnostic::new(file, e.pos, e.message))?;
    write_names(ctx, &mut value);
    Ok(value)
}

/// Changes the canonical names of the constructors, which the check needs, to the names as the root project writes
/// them. The names of the prelude are always written bare, as `Some`, `None`, `true` and `false`.
fn write_names(ctx: &ReadContext, value: &mut Value) {
    match &mut value.kind {
        ValueKind::Array(items) => items.iter_mut().for_each(|item| write_names(ctx, item)),
        ValueKind::Record(members) => members
            .iter_mut()
            .for_each(|member| write_names(ctx, &mut member.value)),
        ValueKind::Tagged { name, body } => {
            *name = if name.ends_with(&format!("@{PRELUDE_PROJECT}")) {
                bare(name).to_string()
            } else {
                (ctx.show)(name)
            };
            match body {
                Some(TaggedBody::Record(members)) => members
                    .iter_mut()
                    .for_each(|member| write_names(ctx, &mut member.value)),
                Some(TaggedBody::Tuple(arguments)) => arguments
                    .iter_mut()
                    .for_each(|argument| write_names(ctx, &mut argument.value)),
                Some(TaggedBody::Array(items)) => items.iter_mut().for_each(|item| write_names(ctx, item)),
                None => {}
            }
        }
        ValueKind::Nat(_) | ValueKind::Int(_) | ValueKind::Float(_) | ValueKind::Str(_) => {}
    }
}

/// The prelude types that are written specially, told by the project that declares them.
enum Prelude {
    Bool,
    Option,
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

/// The name without the project: `Circle@shapes` is `Circle`.
fn bare(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

struct Reader<'c, 'a, 'e> {
    ctx: &'c ReadContext<'a, 'e>,
    file: &'c str,
}

impl Reader<'_, '_, '_> {
    fn fail(&self, pos: Pos, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(self.file, pos, message.into())
    }

    /// A type as it is written in a message, with the names as the root project writes them.
    fn show(&self, ty: &Ty) -> String {
        self.show_to(ty, SHOWN_DEPTH)
    }

    fn show_to(&self, ty: &Ty, depth: usize) -> String {
        if depth == 0 {
            return "...".to_string();
        }
        match ty {
            Ty::Nat => "Nat".to_string(),
            Ty::Int => "Int".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::Str => "String".to_string(),
            Ty::Array(element) => format!("[{}]", self.show_to(element, depth - 1)),
            Ty::Record(fields) => {
                let fields: Vec<String> = fields
                    .iter()
                    .map(|(name, field)| format!("{}: {}", name, self.show_to(field, depth - 1)))
                    .collect();
                format!("{{ {} }}", fields.join(", "))
            }
            Ty::Enum(name, args) => {
                let name = (self.ctx.show)(name);
                if args.is_empty() {
                    name
                } else {
                    let args: Vec<String> = args.iter().map(|arg| self.show_to(arg, depth - 1)).collect();
                    format!("{}<{}>", name, args.join(", "))
                }
            }
            Ty::Var(_) | Ty::Param(_) | Ty::Gen(_) => "a type that is not known".to_string(),
        }
    }

    /// What the JSON is, for a message: `a string`, `an object`.
    fn found(node: &Node) -> &'static str {
        match node.kind {
            Kind::Null => "null",
            Kind::Bool(_) => "a boolean",
            Kind::Number(_) => "a number",
            Kind::Str(_) => "a string",
            Kind::Array(_) => "an array",
            Kind::Object(_) => "an object",
        }
    }

    fn expected(&self, node: &Node, what: &str, ty: &Ty) -> Diagnostic {
        self.fail(
            node.pos,
            format!("Expected {} for {}, found {}", what, self.show(ty), Self::found(node)),
        )
    }

    fn read(&self, ty: &Ty, node: &Node) -> Result<Value, Diagnostic> {
        let pos = node.pos;
        let kind = match ty {
            Ty::Nat | Ty::Int | Ty::Float => self.number(ty, node)?,
            Ty::Str => match &node.kind {
                Kind::Str(text) => ValueKind::Str(text.clone()),
                _ => return Err(self.expected(node, "a string", ty)),
            },
            Ty::Array(element) => match &node.kind {
                Kind::Array(items) => ValueKind::Array(self.items(element, items)?),
                _ => return Err(self.expected(node, "an array", ty)),
            },
            Ty::Record(fields) => match &node.kind {
                Kind::Object(members) => ValueKind::Record(self.members(fields, members, node, "the record")?),
                _ => return Err(self.expected(node, "an object", ty)),
            },
            Ty::Enum(name, args) => return self.enum_value(ty, name, args, node),
            Ty::Var(_) | Ty::Param(_) | Ty::Gen(_) => {
                return Err(self.fail(pos, "The type to read as is not known"));
            }
        };
        Ok(Value { pos, kind })
    }

    fn items(&self, element: &Ty, items: &[Node]) -> Result<Vec<Value>, Diagnostic> {
        items.iter().map(|item| self.read(element, item)).collect()
    }

    /// The members of an object as the fields of a record: each key is one of the fields, once, and every field is
    /// there. The members are in the order of the JSON.
    fn members(
        &self,
        fields: &[(String, Ty)],
        members: &[JsonMember],
        object: &Node,
        what: &str,
    ) -> Result<Vec<Member>, Diagnostic> {
        let mut out: Vec<Member> = Vec::new();
        for member in members {
            let Some((_, ty)) = fields.iter().find(|(name, _)| *name == member.key) else {
                let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
                let known = if names.is_empty() {
                    "it has no fields".to_string()
                } else {
                    format!("the fields are {}", names.join(", "))
                };
                return Err(self.fail(
                    member.key_pos,
                    format!("Unknown key \"{}\" for {}; {}", member.key, what, known),
                ));
            };
            if out.iter().any(|earlier| earlier.name == member.key) {
                return Err(self.fail(member.key_pos, format!("Repeated key \"{}\"", member.key)));
            }
            out.push(Member {
                name: member.key.clone(),
                pos: member.key_pos,
                value: self.read(ty, &member.value)?,
            });
        }
        if let Some((name, _)) = fields.iter().find(|(name, _)| out.iter().all(|m| m.name != *name)) {
            return Err(self.fail(object.pos, format!("Missing field \"{}\" in {}", name, what)));
        }
        Ok(out)
    }

    /// A number takes the text that it was written with, made into the literal of the type that it fills: `5` in a
    /// slot for `Int` is `+5`, and in one for `Float` is `5.0`. The digits are never parsed, so a number of any size is
    /// kept. A number that the type cannot hold is an error at the number.
    fn number(&self, ty: &Ty, node: &Node) -> Result<ValueKind, Diagnostic> {
        let Kind::Number(text) = &node.kind else {
            return Err(self.expected(node, "a number", ty));
        };
        let whole = !text.contains(['.', 'e', 'E']);
        let negative = text.starts_with('-');
        match ty {
            Ty::Nat if whole && !negative => Ok(ValueKind::Nat(text.clone())),
            Ty::Int if whole => Ok(ValueKind::Int(if negative { text.clone() } else { format!("+{text}") })),
            Ty::Float => Ok(ValueKind::Float(float_text(text))),
            _ => {
                let message = format!("The number {} does not fit {}", text, self.show(ty));
                Err(self.fail(node.pos, message))
            }
        }
    }

    fn enum_value(&self, ty: &Ty, name: &str, args: &[Ty], node: &Node) -> Result<Value, Diagnostic> {
        let pos = node.pos;
        let Some(decl) = self.ctx.env.declared_enum(name) else {
            return Err(self.fail(pos, format!("The type {} is not declared", self.show(ty))));
        };
        // The names are the canonical ones, which is what the check expects. They are written out afterwards.
        let tagged = |name: &str, body: Option<TaggedBody>| Value {
            pos,
            kind: ValueKind::Tagged {
                name: name.to_string(),
                body,
            },
        };
        let prelude = |local: &str, body: Option<TaggedBody>| tagged(&format!("{local}@{PRELUDE_PROJECT}"), body);
        match prelude_kind(&decl.name) {
            Some(Prelude::Bool) => {
                return match node.kind {
                    Kind::Bool(true) => Ok(prelude("true", None)),
                    Kind::Bool(false) => Ok(prelude("false", None)),
                    _ => Err(self.expected(node, "true or false", ty)),
                };
            }
            Some(Prelude::Option) => {
                if node.kind == Kind::Null {
                    return Ok(prelude("None", None));
                }
                let inner = self.read(&args[0], node)?;
                let argument = Argument {
                    name: None,
                    pos,
                    value: inner,
                };
                return Ok(prelude("Some", Some(TaggedBody::Tuple(vec![argument]))));
            }
            None => {}
        }
        // A string is a constructor with no body, and an object with one key is one with a body.
        let (key, key_pos, inner) = match &node.kind {
            Kind::Str(key) => (key.as_str(), pos, None),
            Kind::Object(members) if members.len() == 1 => {
                (members[0].key.as_str(), members[0].key_pos, Some(&members[0].value))
            }
            Kind::Object(_) => {
                let message = format!(
                    "Expected an object with one key, the name of a constructor of {}",
                    self.show(ty)
                );
                return Err(self.fail(pos, message));
            }
            _ => return Err(self.expected(node, "a constructor name or an object", ty)),
        };
        let Some(variant) = decl.variants.iter().find(|variant| bare(&variant.name) == key) else {
            let names: Vec<&str> = decl.variants.iter().map(|variant| bare(&variant.name)).collect();
            let message = format!(
                "\"{}\" is not a constructor of {}; the constructors are {}",
                key,
                self.show(ty),
                names.join(", ")
            );
            return Err(self.fail(key_pos, message));
        };
        let body = match (&variant.body, inner) {
            (VariantBody::Unit, None) => None,
            (VariantBody::Unit, Some(_)) => {
                return Err(self.fail(key_pos, format!("{} has no body, so write it as \"{}\"", key, key)));
            }
            (_, None) => {
                return Err(self.fail(pos, format!("{} has a body, so write {{\"{}\": ...}}", key, key)));
            }
            (VariantBody::Record(fields), Some(body)) => {
                let Kind::Object(members) = &body.kind else {
                    return Err(self.fail(body.pos, format!("Expected an object for the fields of {}", key)));
                };
                let fields = instantiate_fields(fields, &decl.params, args);
                Some(TaggedBody::Record(self.members(&fields, members, body, key)?))
            }
            (VariantBody::Array(element), Some(body)) => {
                let element = instantiate(element, &decl.params, args);
                let Kind::Array(items) = &body.kind else {
                    return Err(self.fail(body.pos, format!("Expected an array for the elements of {}", key)));
                };
                Some(TaggedBody::Array(self.items(&element, items)?))
            }
            (VariantBody::Tuple(parameters), Some(body)) => {
                let types: Vec<Ty> = parameters
                    .iter()
                    .map(|p| instantiate(&p.ty, &decl.params, args))
                    .collect();
                // A tuple with one parameter is written as its value, and any other tuple as an array.
                let nodes: Vec<&Node> = if types.len() == 1 {
                    vec![body]
                } else {
                    let Kind::Array(items) = &body.kind else {
                        let message = format!("Expected an array of {} values for the tuple {}", types.len(), key);
                        return Err(self.fail(body.pos, message));
                    };
                    items.iter().collect()
                };
                if nodes.len() > types.len() {
                    let message = format!("{} takes {} value(s) but has {}", key, types.len(), nodes.len());
                    return Err(self.fail(nodes[types.len()].pos, message));
                }
                let mut arguments = Vec::new();
                for (index, node) in nodes.iter().enumerate() {
                    arguments.push(Argument {
                        name: parameters[index].name.clone(),
                        pos: node.pos,
                        value: self.read(&types[index], node)?,
                    });
                }
                // The parameters that have a name and are not given are left out, and take their defaults. A tuple that
                // has too few values for the parameters without a name is for the check to refuse.
                Some(TaggedBody::Tuple(arguments))
            }
        };
        Ok(tagged(&variant.name, body))
    }
}

/// The text of a number as a float literal, which has a point before an exponent.
fn float_text(text: &str) -> String {
    match text.find(['e', 'E']) {
        Some(at) if !text[..at].contains('.') => format!("{}.0{}", &text[..at], &text[at..]),
        Some(_) => text.to_string(),
        None if text.contains('.') => text.to_string(),
        None => format!("{text}.0"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse_document;
    use crate::{convert, convert_typed_back, plain_layout, prelude_source, CheckOptions, Source};

    /// Reads the JSON as a value of the type, in a project of the one types file, and gives the text of the value, or
    /// the position and the message of the problem.
    fn read(types: &str, ty: &str, json: &str) -> Result<String, String> {
        let mut sources = vec![prelude_source()];
        sources.push(Source {
            name: "types".to_string(),
            text: types.to_string(),
        });
        let layout = plain_layout(&sources);
        let mut text = None;
        let mut problem = None;
        let result = convert_typed_back(&sources, &layout, &CheckOptions::default(), ty, &mut |context| {
            let node = parse_document(json).expect("The JSON of the test is well formed.");
            match read_typed(context, &node, "data.json", true) {
                Ok(value) => text = Some(crate::values_text::render(&value, true)),
                Err(e) => problem = Some(format!("{}:{} {}", e.pos.line, e.pos.col, e.message)),
            }
            Ok(())
        });
        if let Err(convert::LinesError::Invalid(diagnostics)) = result {
            return Err(format!("types: {}", diagnostics[0].message));
        }
        match (text, problem) {
            (Some(text), _) => Ok(text),
            (_, Some(problem)) => Err(problem),
            _ => panic!("nothing was read"),
        }
    }

    const SHAPES: &str = "enum Shape = Circle { r: Float }, Square(Float), Rect(Float, Float), Dot, Ping(), \
         Many[Nat], Named(x: Nat, y: Nat);\nenum Pair<T> = Pair(T, T);";

    #[test]
    fn scalars_are_read_as_their_type() {
        assert_eq!(read("", "String", "\"hi\\n\"").unwrap(), "\"hi\\n\";\n");
        assert_eq!(read("", "[Nat]", "[1, 2]").unwrap(), "[1, 2];\n");
        assert_eq!(
            read("", "{ a: Nat, b: [String] }", "{\"b\": [\"x\"], \"a\": 1}").unwrap(),
            "{ b: [\"x\"], a: 1 };\n"
        );
        assert_eq!(read("", "Bool", "true").unwrap(), "true;\n");
        assert_eq!(read("", "Option<Nat>", "null").unwrap(), "None;\n");
        assert_eq!(read("", "Option<Nat>", "5").unwrap(), "Some(5);\n");
    }

    #[test]
    fn a_number_takes_the_text_of_its_slot() {
        assert_eq!(read("", "Nat", "5").unwrap(), "5;\n");
        assert_eq!(read("", "Int", "5").unwrap(), "+5;\n");
        assert_eq!(read("", "Int", "-5").unwrap(), "-5;\n");
        assert_eq!(read("", "Float", "5").unwrap(), "5.0;\n");
        assert_eq!(read("", "Float", "-5").unwrap(), "-5.0;\n");
        assert_eq!(read("", "Float", "1.50").unwrap(), "1.50;\n");
        assert_eq!(read("", "Float", "1e3").unwrap(), "1.0e3;\n");
        assert_eq!(read("", "Float", "2E+3").unwrap(), "2.0E+3;\n");
        assert_eq!(read("", "Float", "2.5e-3").unwrap(), "2.5e-3;\n");
        assert_eq!(
            read("", "Nat", "123456789012345678901234567890").unwrap(),
            "123456789012345678901234567890;\n"
        );
    }

    #[test]
    fn a_number_that_the_type_cannot_hold_is_an_error_at_the_number() {
        assert_eq!(
            read("", "Nat", "1.5").unwrap_err(),
            "1:1 The number 1.5 does not fit Nat"
        );
        assert_eq!(read("", "Nat", "-1").unwrap_err(), "1:1 The number -1 does not fit Nat");
        assert_eq!(
            read("", "Int", "1e3").unwrap_err(),
            "1:1 The number 1e3 does not fit Int"
        );
        assert_eq!(
            read("", "[Nat]", "[1, 2.5]").unwrap_err(),
            "1:5 The number 2.5 does not fit Nat"
        );
        assert_eq!(
            read("", "Nat", "\"5\"").unwrap_err(),
            "1:1 Expected a number for Nat, found a string"
        );
    }

    #[test]
    fn the_wrong_shape_is_an_error_that_says_what_was_expected() {
        assert_eq!(
            read("", "String", "5").unwrap_err(),
            "1:1 Expected a string for String, found a number"
        );
        assert_eq!(
            read("", "[Nat]", "{}").unwrap_err(),
            "1:1 Expected an array for [Nat], found an object"
        );
        assert_eq!(
            read("", "Bool", "1").unwrap_err(),
            "1:1 Expected true or false for Bool, found a number"
        );
        assert_eq!(
            read("", "{ a: Nat }", "[]").unwrap_err(),
            "1:1 Expected an object for { a: Nat }, found an array"
        );
    }

    #[test]
    fn a_record_has_its_fields_once_and_only_those() {
        let ty = "{ a: Nat, b: Nat }";
        assert_eq!(
            read("", ty, "{\"a\": 1}").unwrap_err(),
            "1:1 Missing field \"b\" in the record"
        );
        assert_eq!(
            read("", ty, "{\"a\": 1, \"c\": 2, \"b\": 3}").unwrap_err(),
            "1:10 Unknown key \"c\" for the record; the fields are a, b"
        );
        assert_eq!(
            read("", ty, "{\"a\": 1, \"a\": 2, \"b\": 3}").unwrap_err(),
            "1:10 Repeated key \"a\""
        );
    }

    #[test]
    fn the_shapes_of_an_enum_are_read_by_the_declaration() {
        let shape = |json: &str| read(SHAPES, "Shape", json);
        assert_eq!(shape("\"Dot\"").unwrap(), "Dot;\n");
        assert_eq!(shape("{\"Circle\": {\"r\": 1}}").unwrap(), "Circle { r: 1.0 };\n");
        assert_eq!(shape("{\"Square\": 2.5}").unwrap(), "Square(2.5);\n");
        assert_eq!(shape("{\"Rect\": [1, 2]}").unwrap(), "Rect(1.0, 2.0);\n");
        assert_eq!(shape("{\"Ping\": []}").unwrap(), "Ping();\n");
        assert_eq!(shape("{\"Many\": [1, 2]}").unwrap(), "Many[1, 2];\n");
        assert_eq!(shape("{\"Many\": []}").unwrap(), "Many[];\n");
        assert_eq!(shape("{\"Named\": [1, 2]}").unwrap(), "Named(x: 1, y: 2);\n");
    }

    #[test]
    fn a_generic_enum_is_read_with_its_arguments() {
        assert_eq!(
            read(SHAPES, "Pair<Int>", "{\"Pair\": [1, -2]}").unwrap(),
            "Pair(+1, -2);\n"
        );
        assert_eq!(
            read(SHAPES, "[Pair<Float>]", "[{\"Pair\": [1, 2]}]").unwrap(),
            "[Pair(1.0, 2.0)];\n"
        );
        assert_eq!(
            read(SHAPES, "Option<Pair<Nat>>", "{\"Pair\": [1, 2]}").unwrap(),
            "Some(Pair(1, 2));\n"
        );
    }

    #[test]
    fn a_constructor_that_does_not_fit_is_an_error_at_the_key() {
        let shape = |json: &str| read(SHAPES, "Shape", json);
        assert_eq!(
            shape("\"Cube\"").unwrap_err(),
            "1:1 \"Cube\" is not a constructor of Shape; \
             the constructors are Circle, Square, Rect, Dot, Ping, Many, Named"
        );
        assert!(shape("{\"Cube\": 1}")
            .unwrap_err()
            .starts_with("1:2 \"Cube\" is not a constructor"));
        assert_eq!(
            shape("{\"Dot\": 1}").unwrap_err(),
            "1:2 Dot has no body, so write it as \"Dot\""
        );
        assert_eq!(
            shape("\"Circle\"").unwrap_err(),
            "1:1 Circle has a body, so write {\"Circle\": ...}"
        );
        assert!(shape("{\"Dot\": 1, \"Ping\": []}")
            .unwrap_err()
            .starts_with("1:1 Expected an object with one key"));
        assert!(shape("5")
            .unwrap_err()
            .starts_with("1:1 Expected a constructor name or an object for Shape"));
        assert_eq!(
            shape("{\"Rect\": [1, 2, 3]}").unwrap_err(),
            "1:17 Rect takes 2 value(s) but has 3"
        );
        assert_eq!(
            shape("{\"Rect\": 1}").unwrap_err(),
            "1:10 Expected an array of 2 values for the tuple Rect"
        );
        assert_eq!(
            shape("{\"Circle\": {\"r\": 1, \"s\": 2}}").unwrap_err(),
            "1:21 Unknown key \"s\" for Circle; the fields are r"
        );
        assert_eq!(
            shape("{\"Circle\": {}}").unwrap_err(),
            "1:12 Missing field \"r\" in Circle"
        );
    }

    #[test]
    fn a_named_parameter_that_is_left_out_takes_its_default() {
        assert_eq!(read(SHAPES, "Shape", "{\"Named\": [1]}").unwrap(), "Named(x: 1);\n");
    }

    #[test]
    fn an_unnamed_parameter_that_is_left_out_is_refused_by_the_check() {
        let e = read(SHAPES, "Shape", "{\"Rect\": [1]}").unwrap_err();
        assert!(e.starts_with("1:1 Constructor Rect takes 2 value(s) but has 1"), "{e}");
    }

    #[test]
    fn the_types_decide_what_null_is() {
        assert_eq!(read("", "Option<Option<Nat>>", "null").unwrap(), "None;\n");
        assert_eq!(read("", "Option<Option<Nat>>", "3").unwrap(), "Some(Some(3));\n");
        assert_eq!(read("", "[Option<Nat>]", "[null, 1]").unwrap(), "[None, Some(1)];\n");
    }

    #[test]
    fn a_type_with_a_problem_is_reported_before_anything_is_read() {
        assert_eq!(read("", "Missing", "1").unwrap_err(), "types: Unknown type Missing");
    }

    /// As `read`, in the project `example.app`, which imports `example.a` into the group `a` and `example.b` into the
    /// group `b`, with the prelude as `local`, and has the types file `app`.
    fn read_in_projects(a: &str, b: &str, app: &str, ty: &str, json: &str) -> Result<String, String> {
        use crate::{ImportInfo, Layout, ProjectInfo};
        let info = |name: &str, imports: Vec<ImportInfo>, builtin: bool| ProjectInfo {
            name: name.to_string(),
            imports,
            builtin,
        };
        let layout = Layout {
            projects: vec![
                info("constructo.prelude", vec![], true),
                info("example.a", vec![ImportInfo::new(Some("local"), 0)], false),
                info("example.b", vec![ImportInfo::new(Some("local"), 0)], false),
                info(
                    "example.app",
                    vec![
                        ImportInfo::new(Some("local"), 0),
                        ImportInfo::new(Some("a"), 1),
                        ImportInfo::new(Some("b"), 2),
                    ],
                    false,
                ),
            ],
            owners: vec![0, 1, 2, 3],
            root: 3,
        };
        let sources: Vec<Source> = [
            ("p", crate::PRELUDE),
            ("a.types", a),
            ("b.types", b),
            ("app.types", app),
        ]
        .iter()
        .map(|(name, text)| Source {
            name: name.to_string(),
            text: text.to_string(),
        })
        .collect();
        let mut text = None;
        let result = convert_typed_back(&sources, &layout, &CheckOptions::default(), ty, &mut |context| {
            let node = parse_document(json).expect("The JSON of the test is well formed.");
            text = Some(
                read_typed(context, &node, "data.json", true)
                    .map(|value| crate::values_text::render(&value, true))
                    .map_err(|e| e.message),
            );
            Ok(())
        });
        if let Err(convert::LinesError::Invalid(diagnostics)) = result {
            return Err(format!("types: {}", diagnostics[0]));
        }
        text.expect("nothing was read")
    }

    #[test]
    fn a_name_is_written_bare_where_it_is_in_scope_and_in_full_where_it_is_not() {
        let a = "enum Shape = Circle, Middle;";
        let b = "enum Shape = Square, Middle;";
        let app = "enum Local = Home;";
        let read = |ty: &str, json: &str| read_in_projects(a, b, app, ty, json);
        assert_eq!(read("a::Shape", "\"Middle\"").unwrap(), "example.a::Middle;\n");
        assert_eq!(read("b::Shape", "\"Middle\"").unwrap(), "example.b::Middle;\n");
        assert_eq!(read("a::Shape", "\"Circle\"").unwrap(), "example.a::Circle;\n");
        assert_eq!(
            read("[b::Shape]", "[\"Square\", \"Middle\"]").unwrap(),
            "[example.b::Square, example.b::Middle];\n"
        );
        assert_eq!(read("Local", "\"Home\"").unwrap(), "Home;\n");
        assert_eq!(
            read("Option<a::Shape>", "\"Middle\"").unwrap(),
            "Some(example.a::Middle);\n"
        );
        assert_eq!(read("Bool", "true").unwrap(), "true;\n");
        assert_eq!(read("Option<Nat>", "null").unwrap(), "None;\n");
    }

    #[test]
    fn a_type_is_found_only_by_the_name_that_the_project_gives_it() {
        let a = "enum Shape = Circle;";
        assert_eq!(
            read_in_projects(a, "", "", "a::Shape", "\"Circle\"").unwrap(),
            "example.a::Circle;\n"
        );
        let e = read_in_projects(a, "", "", "Shape", "\"Circle\"").unwrap_err();
        assert!(e.starts_with("types:"), "{e}");
    }

    /// The JSON that `--to=json-typed` writes for the values, read back as the type and written as values again,
    /// converts to the same JSON, which shows that the reader is the inverse of the encoder, and that the values that
    /// it makes are the ones that `check` accepts, since the conversion checks them.
    #[test]
    fn what_to_json_typed_writes_is_read_back_to_the_same_json() {
        let schema = include_str!("../syntax/examples/convert/typed/schema.types");
        let cases = [
            ("Shape", "Circle { r: 1.5 }"),
            ("Shape", "Circle { r: 1 }"),
            ("Shape", "Dot"),
            ("Shape", "Pair(1, 2)"),
            ("Shape", "Wrapper(7)"),
            ("Shape", "Ping()"),
            ("Shape", "Items[1, 2, 3]"),
            ("Shape", "Items[]"),
            ("Shape", "Tagged(label: \"x\", size: 2)"),
            ("Shape", "Tagged(\"x\")"),
            ("[Shape]", "[Dot, Circle { r: 2.0 }, Pair(3, 4)]"),
            ("Option<Shape>", "Some(Dot)"),
            ("Option<Shape>", "None"),
            ("Option<Option<Nat>>", "Some(Some(3))"),
            ("Option<Option<Nat>>", "None"),
            ("Bool", "true"),
            ("Nat", "five"),
            ("[String]", "names"),
            (
                "Config",
                "Config { name: Some(\"n\"), on: false, speed: Slow, tags: [\"a\", \"b\"], size: { w: 1, h: 2.5 } }",
            ),
            (
                "Config",
                "Config { name: None, on: true, speed: Fast, tags: [], size: { w: 0, h: 0 } }",
            ),
            ("Defaults", "Defaults()"),
            ("Defaults", "Defaults(a: 3, e: Some(4))"),
            ("Int", "-5"),
            ("Float", "1.0e3"),
            ("{ a: Int, b: [Float] }", "{ a: 5, b: [1, 2.5] }"),
        ];
        for (ty, value) in cases {
            let sources = |last: &str| {
                vec![
                    prelude_source(),
                    Source {
                        name: "schema.types".to_string(),
                        text: schema.to_string(),
                    },
                    Source {
                        name: "value.values".to_string(),
                        text: last.to_string(),
                    },
                ]
            };
            let to_json = |text: &str| {
                let sources = sources(text);
                let layout = plain_layout(&sources);
                crate::convert_typed(&sources, &layout, &CheckOptions::default(), false, true)
                    .unwrap_or_else(|d| panic!("{ty} {text}: {}", d[0]))
                    .output
            };
            let json = to_json(&format!("{value};"));
            let mut read_back = None;
            let types: Vec<Source> = sources("").into_iter().take(2).collect();
            let layout = plain_layout(&types);
            let result = convert_typed_back(&types, &layout, &CheckOptions::default(), ty, &mut |context| {
                let node = parse_document(&json).expect("The JSON that is written is well formed.");
                let value =
                    read_typed(context, &node, "data.json", true).unwrap_or_else(|d| panic!("{ty} {json}: {d}"));
                read_back = Some(crate::values_text::render(&value, true));
                Ok(())
            });
            assert!(result.is_ok(), "{ty} {value}");
            let again = to_json(&read_back.expect("The reader ran."));
            assert_eq!(again, json, "{ty} {value}");
        }
    }

    /// A reader that gives two good lines, a bad one, and then lines for ever, which is only read as far as the bad
    /// line if a lines input is read a line at a time and is not read whole.
    struct Endless {
        sent: usize,
    }

    impl std::io::Read for Endless {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            const LINES: [&str; 3] = ["\"Dot\"\n", "\"Dot\"\n", "\"Cube\"\n"];
            let line = LINES.get(self.sent).copied().unwrap_or("\"Dot\"\n");
            self.sent += 1;
            let bytes = line.as_bytes();
            buffer[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        }
    }

    #[test]
    fn a_lines_input_is_read_a_line_at_a_time() {
        let mut sources = vec![prelude_source()];
        sources.push(Source {
            name: "types".to_string(),
            text: SHAPES.to_string(),
        });
        let layout = plain_layout(&sources);
        let mut out = Vec::new();
        let mut result = None;
        convert_typed_back(&sources, &layout, &CheckOptions::default(), "Shape", &mut |context| {
            let mut reader = std::io::BufReader::new(Endless { sent: 0 });
            result = Some(convert::convert_from_typed_lines(
                context,
                "in.jsonl",
                &mut reader,
                true,
                true,
                &mut out,
            ));
            Ok(())
        })
        .unwrap();
        match result.expect("The reader ran.") {
            Err(convert::LinesError::Invalid(diagnostics)) => {
                assert!(
                    diagnostics[0].to_string().contains("in.jsonl, line 3, col 1"),
                    "{}",
                    diagnostics[0]
                );
            }
            _ => panic!("The third line is not a constructor of Shape."),
        }
        assert_eq!(String::from_utf8(out).unwrap(), "Dot;\nDot;\n");
    }

    /// Reads the text as the lines of `jsonl-typed` with the type, and gives what was written and the error if the
    /// reading ended with one.
    fn lines(ty: &str, text: &str) -> (String, Option<String>) {
        let mut sources = vec![prelude_source()];
        sources.push(Source {
            name: "types".to_string(),
            text: SHAPES.to_string(),
        });
        let layout = plain_layout(&sources);
        let mut out = Vec::new();
        let mut error = None;
        convert_typed_back(&sources, &layout, &CheckOptions::default(), ty, &mut |context| {
            let mut reader = text.as_bytes();
            match convert::convert_from_typed_lines(context, "in.jsonl", &mut reader, true, true, &mut out) {
                Ok(None) => {}
                Ok(Some(e)) => error = Some(e.to_string()),
                Err(convert::LinesError::Invalid(diagnostics)) => error = Some(diagnostics[0].to_string()),
                Err(convert::LinesError::Write(e)) => panic!("{e}"),
            }
            Ok(())
        })
        .unwrap();
        (String::from_utf8(out).unwrap(), error)
    }

    #[test]
    fn a_line_is_a_value_and_the_last_line_needs_no_new_line() {
        assert_eq!(
            lines("Shape", "\"Dot\"\n{\"Square\": 2}"),
            ("Dot;\nSquare(2.0);\n".to_string(), None)
        );
        assert_eq!(lines("Shape", "\"Dot\"\n"), ("Dot;\n".to_string(), None));
    }

    #[test]
    fn blank_lines_are_skipped_and_a_line_may_end_in_carriage_return_and_new_line() {
        let (out, error) = lines("Shape", "\n  \t\n\"Dot\"\r\n\r\n\"Dot\"\r\n");
        assert_eq!((out.as_str(), error), ("Dot;\nDot;\n", None));
    }

    #[test]
    fn a_file_with_no_lines_or_only_blank_ones_has_no_values() {
        for text in ["", "\n", "  \n\t\r\n\n"] {
            assert_eq!(lines("Shape", text), (String::new(), None), "{text:?}");
        }
    }

    #[test]
    fn an_array_on_a_line_is_a_value_only_if_the_type_is_an_array_type() {
        assert_eq!(lines("[Nat]", "[1, 2]\n[]\n"), ("[1, 2];\n[];\n".to_string(), None));
        let (out, error) = lines("Shape", "\"Dot\"\n[1, 2]\n");
        assert_eq!(out, "Dot;\n");
        let error = error.expect("an array is not a Shape");
        assert!(
            error.contains("Expected a constructor name or an object for Shape, found an array"),
            "{error}"
        );
        assert!(error.contains("in.jsonl, line 2, col 1"), "{error}");
    }

    #[test]
    fn a_problem_is_at_the_line_of_the_file_and_the_lines_before_it_are_written() {
        let (out, error) = lines("Shape", "\"Dot\"\n\n  \"Cube\"\n\"Dot\"\n");
        assert_eq!(out, "Dot;\n");
        let error = error.expect("Cube is not a constructor");
        assert!(error.contains("in.jsonl, line 3, col 3"), "{error}");
        let (out, error) = lines("Shape", "\"Dot\"\n{\"Square\": \n");
        assert_eq!(out, "Dot;\n");
        assert!(
            error
                .expect("the line stops half way")
                .contains("in.jsonl, line 2, col"),
            "half way"
        );
    }
}
