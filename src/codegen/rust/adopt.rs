//! The file of adoptions, which `--adopt` names: for each constructor or enum that a program adopts, the Rust type that
//! holds its values in place of the one that would be generated, and the two functions that convert to and from it.
//!
//! The file is TOML. Its top-level table is the language, and `rust` is the only one. Under it, `constructor` and
//! `enum` each have a table for each adopted name, which is the full name of a constructor or an enum of the run, with
//! the keys `type`, `from` and `to`. Every problem with the file is an error, as in the settings file, so that a
//! misspelt name is never taken for an adoption.

use super::super::model::{Body, FullName, Model};
use super::super::recursion::Opaque;
use crate::diagnostics::{Diagnostic, Pos};
use crate::PRELUDE_PROJECT;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use toml::Spanned;

/// The adoptions of a file, in the order in which they are written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Adoptions {
    /// The name of the file, as it is shown in a message.
    pub file: String,
    pub constructors: Vec<Adoption>,
    pub enums: Vec<Adoption>,
}

/// The adoption of one constructor or enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoption {
    /// The full name of the constructor or enum, as the file writes it.
    pub name: String,
    /// Where the name is in the file.
    pub at: Pos,
    /// The Rust type, without its generic arguments.
    pub ty: RustPath,
    /// The function from the arguments, or from the generated enum, to the type.
    pub from: RustPath,
    /// The function from the type back.
    pub to: RustPath,
}

/// A path to a Rust item, as it is written into the generated code, which does not depend on the module that it is
/// written in: one that starts with `crate::` or `::` as the file writes it, and any other with `::` in front, as the
/// path of another crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustPath {
    pub path: String,
    /// Where the path is in the file.
    pub at: Pos,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    rust: Option<Language>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Language {
    #[serde(default)]
    constructor: BTreeMap<String, Spanned<Entry>>,
    #[serde(default, rename = "enum")]
    enums: BTreeMap<String, Spanned<Entry>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    #[serde(rename = "type")]
    ty: Spanned<String>,
    from: Spanned<String>,
    to: Spanned<String>,
}

impl Adoptions {
    /// Reads the adoptions from the text of a file. `name` is the name of the file in diagnostics.
    pub fn parse(text: &str, name: &str) -> Result<Adoptions, Diagnostic> {
        let text = crate::lexer::strip_bom(text);
        let at = |offset: usize| Pos::at(text, offset);
        let file: File = toml::from_str(text).map_err(|e| {
            let offset = e.span().map_or(0, |span| span.start);
            Diagnostic::new(name, at(offset), e.message())
        })?;
        let Some(rust) = file.rust else {
            return Ok(Adoptions {
                file: name.to_string(),
                ..Adoptions::default()
            });
        };
        let adoptions = |table: BTreeMap<String, Spanned<Entry>>| -> Result<Vec<Adoption>, Diagnostic> {
            let mut list: Vec<(usize, Adoption)> = Vec::new();
            for (adopted, entry) in table {
                let start = entry.span().start;
                let entry = entry.into_inner();
                let path = |what: &str, value: Spanned<String>| {
                    let pos = at(value.span().start);
                    rust_path(value.get_ref())
                        .map(|path| RustPath { path, at: pos })
                        .map_err(|why| Diagnostic::new(name, pos, format!("The {} of {} {}", what, adopted, why)))
                };
                let adoption = Adoption {
                    ty: path("type", entry.ty)?,
                    from: path("function from", entry.from)?,
                    to: path("function to", entry.to)?,
                    name: adopted,
                    at: at(start),
                };
                list.push((start, adoption));
            }
            // The tables are read in the order of their names, and kept in the order in which they are written.
            list.sort_by_key(|(start, _)| *start);
            Ok(list.into_iter().map(|(_, adoption)| adoption).collect())
        };
        Ok(Adoptions {
            file: name.to_string(),
            constructors: adoptions(rust.constructor)?,
            enums: adoptions(rust.enums)?,
        })
    }

    /// Whether the file adopts nothing.
    pub fn is_empty(&self) -> bool {
        self.constructors.is_empty() && self.enums.is_empty()
    }

    /// The errors of the adoptions against the run: a name that it does not have, a constructor with no arguments
    /// adopted on its own, an enum and one of its constructors both adopted, the prelude's `Bool` and `Option`, which
    /// are Rust's own, and two adoptions that would give one Rust type two impls.
    pub fn check(&self, model: &Model) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        let error = |at: Pos, message: String| Diagnostic::new(&self.file, at, message);
        let enums: HashMap<String, usize> = model
            .enums
            .iter()
            .enumerate()
            .map(|(at, decl)| (decl.name.to_string(), at))
            .collect();
        let constructors: HashMap<String, (usize, usize)> = model
            .enums
            .iter()
            .enumerate()
            .flat_map(|(at, decl)| {
                decl.constructors
                    .iter()
                    .enumerate()
                    .map(move |(c, constructor)| (constructor.name.to_string(), (at, c)))
            })
            .collect();
        let rusts_own = |name: &FullName| name.project == PRELUDE_PROJECT && ["Bool", "Option"].contains(&&*name.local);
        // The adoptions that give a Rust type the impls of the codec, by their type.
        let mut impls: HashMap<&str, &Adoption> = HashMap::new();
        let mut give_impls: Vec<&Adoption> = Vec::new();
        for adoption in &self.enums {
            let Some(&at) = enums.get(&adoption.name) else {
                let hint = if constructors.contains_key(&adoption.name) {
                    ", but it has a constructor of that name, which is adopted under [rust.constructor]".to_string()
                } else {
                    String::new()
                };
                errors.push(error(
                    adoption.at,
                    format!("The run has no enum {}{}", adoption.name, hint),
                ));
                continue;
            };
            let decl = &model.enums[at];
            if rusts_own(&decl.name) {
                errors.push(error(
                    adoption.at,
                    format!("The enum {} is Rust's own, and cannot be adopted", adoption.name),
                ));
                continue;
            }
            give_impls.push(adoption);
        }
        for adoption in &self.constructors {
            let Some(&(at, c)) = constructors.get(&adoption.name) else {
                let hint = if enums.contains_key(&adoption.name) {
                    ", but it has an enum of that name, which is adopted under [rust.enum]".to_string()
                } else {
                    String::new()
                };
                errors.push(error(
                    adoption.at,
                    format!("The run has no constructor {}{}", adoption.name, hint),
                ));
                continue;
            };
            let decl = &model.enums[at];
            let constructor = &decl.constructors[c];
            if rusts_own(&decl.name) {
                errors.push(error(
                    adoption.at,
                    format!(
                        "The constructor {} is of {}, which is Rust's own, and cannot be adopted",
                        adoption.name, decl.name
                    ),
                ));
                continue;
            }
            if let Some(both) = self.enums.iter().find(|e| e.name == decl.name.to_string()) {
                errors.push(error(
                    adoption.at,
                    format!(
                        "The constructor {} and its enum {} cannot both be adopted",
                        adoption.name, both.name
                    ),
                ));
                continue;
            }
            if constructor.body == Body::None {
                errors.push(error(
                    adoption.at,
                    format!(
                        "The constructor {} has no arguments to convert, so it cannot be adopted on its own; adopt its \
                         enum {}",
                        adoption.name, decl.name
                    ),
                ));
                continue;
            }
            // The one constructor of an enum makes the enum's type the adopted type, which has the impls.
            if decl.constructors.len() == 1 {
                give_impls.push(adoption);
            }
        }
        // The later of two in the file is the error, as for a duplicate anywhere else.
        give_impls.sort_by_key(|adoption| adoption.at);
        for adoption in give_impls {
            match impls.get(adoption.ty.path.as_str()) {
                Some(first) => errors.push(error(
                    adoption.ty.at,
                    format!(
                        "{} and {} are both adopted as {}, which would give it two impls of the codec",
                        first.name, adoption.name, adoption.ty.path
                    ),
                )),
                None => {
                    impls.insert(&adoption.ty.path, adoption);
                }
            }
        }
        errors.sort_by_key(|e| e.pos);
        errors
    }
}

impl Adoptions {
    /// The types that the adoptions make opaque to the generator: every constructor that is adopted, and every enum
    /// whose uses are of an adopted type, which is one that is adopted whole or whose one constructor is.
    pub(crate) fn opaque(&self, model: &Model) -> Opaque {
        let mut opaque = Opaque::default();
        for adoption in &self.enums {
            if let Some(decl) = model.enums.iter().find(|e| e.name.to_string() == adoption.name) {
                opaque.enums.insert(decl.name.clone());
            }
        }
        for adoption in &self.constructors {
            for decl in &model.enums {
                if let Some(constructor) = decl.constructors.iter().find(|c| c.name.to_string() == adoption.name) {
                    opaque.constructors.insert(constructor.name.clone());
                    if decl.constructors.len() == 1 {
                        opaque.enums.insert(decl.name.clone());
                    }
                }
            }
        }
        opaque
    }
}

/// The path as it is written into the generated code, or why it is not a path that can be: it starts with `self::` or
/// `super::`, which would mean something else in each module, or it is not a path of identifiers.
fn rust_path(text: &str) -> Result<String, String> {
    let (prefix, rest) = if let Some(rest) = text.strip_prefix("::") {
        ("::", rest)
    } else if let Some(rest) = text.strip_prefix("crate::") {
        ("crate::", rest)
    } else {
        ("::", text)
    };
    let segments: Vec<&str> = rest.split("::").collect();
    if matches!(segments.first(), Some(&"self") | Some(&"super")) {
        return Err(format!(
            "is {}, which would mean something else in each module of the generated code; start it with crate:: or \
             with the name of a crate",
            text
        ));
    }
    let identifier = |segment: &str| {
        let segment = segment.strip_prefix("r#").unwrap_or(segment);
        segment.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && segment.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && segment != "_"
    };
    if segments.iter().all(|segment| identifier(segment)) {
        Ok(format!("{}{}", prefix, rest))
    } else {
        Err(format!(
            "is {:?}, which is not a Rust path, such as crate::paint::Colour or uuid::Uuid, without generic arguments",
            text
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Adoptions, String> {
        Adoptions::parse(text, "adopt.toml").map_err(|d| d.to_string())
    }

    const UUID: &str = r#"
[rust.constructor."com.example.ids::Uuid"]
type = "uuid::Uuid"
from = "crate::adopt::uuid_from_string"
to = "::crate_b::uuid_to_string"
"#;

    #[test]
    fn an_adoption_has_a_type_and_two_functions_whose_paths_do_not_depend_on_the_module() {
        let adoptions = parse(UUID).unwrap();
        assert!(adoptions.enums.is_empty());
        let [uuid] = &adoptions.constructors[..] else {
            panic!("{:?}", adoptions)
        };
        assert_eq!(uuid.name, "com.example.ids::Uuid");
        assert_eq!(uuid.at, Pos { line: 2, col: 1 });
        assert_eq!(uuid.ty.path, "::uuid::Uuid");
        assert_eq!(uuid.ty.at, Pos { line: 3, col: 8 });
        assert_eq!(uuid.from.path, "crate::adopt::uuid_from_string");
        assert_eq!(uuid.to.path, "::crate_b::uuid_to_string");
    }

    #[test]
    fn an_empty_file_adopts_nothing_and_the_tables_keep_their_order() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("[rust]\n").unwrap().is_empty());
        let text = r#"
[rust.enum."b::Z"]
type = "crate::Z"
from = "crate::z_from"
to = "crate::z_to"

[rust.enum."a::Y"]
type = "crate::Y"
from = "crate::y_from"
to = "crate::y_to"
"#;
        let names: Vec<String> = parse(text).unwrap().enums.into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["b::Z", "a::Y"]);
    }

    #[test]
    fn a_mistake_in_the_toml_is_an_error_at_it() {
        let error = |text: &str| parse(text).unwrap_err();
        assert!(error("[rust.constructor\n").contains("line 1"));
        let unknown = error("[python.constructor.\"a::B\"]\n");
        assert!(
            unknown.contains("unknown field `python`") && unknown.contains("`rust`"),
            "{}",
            unknown
        );
        let unknown = error("[rust.type.\"a::B\"]\n");
        assert!(
            unknown.contains("unknown field `type`") && unknown.contains("`enum`"),
            "{}",
            unknown
        );
        let missing = error(&UUID.replace("to = \"::crate_b::uuid_to_string\"\n", ""));
        assert!(missing.contains("missing field `to`"), "{}", missing);
        let extra = error(&format!("{}into = \"x\"\n", UUID));
        assert!(extra.contains("unknown field `into`"), "{}", extra);
        let wrong = error(&UUID.replace("\"uuid::Uuid\"", "1"));
        assert!(wrong.contains("expected a string"), "{}", wrong);
    }

    /// The errors of the adoptions against the model of types files without a project, whose full names are their
    /// local names, as they are shown.
    fn errors(types: &str, adoptions: &str) -> Vec<String> {
        let sources = vec![
            crate::prelude_source(),
            crate::Source {
                name: "app.types".into(),
                text: types.into(),
            },
        ];
        let layout = crate::plain_layout(&sources);
        let model = crate::build_model(&sources, &layout, &crate::CheckOptions::default()).unwrap();
        parse(adoptions)
            .unwrap()
            .check(&model)
            .iter()
            .map(|d| d.to_string())
            .collect()
    }

    /// A table of adoptions of the kind, `constructor` or `enum`, for the name and the Rust type.
    fn table(kind: &str, name: &str, ty: &str) -> String {
        format!(
            "[rust.{}.\"{}\"]\ntype = \"{}\"\nfrom = \"crate::from\"\nto = \"crate::to\"\n",
            kind, name, ty
        )
    }

    const TYPES: &str = "enum Shape = Circle { radius: Float }, Dot; enum Uuid = Uuid(String); enum Unit = Unit;
                         enum Point = Point(x: Float, y: Float);";

    #[test]
    fn adoptions_of_the_run_are_accepted() {
        let file = [
            table("constructor", "Circle", "crate::Round"),
            table("constructor", "Uuid", "uuid::Uuid"),
            table("enum", "Unit", "crate::Unit"),
            table("enum", "Point", "crate::Point"),
        ]
        .concat();
        assert_eq!(errors(TYPES, &file), Vec::<String>::new());
        // One Rust type can hold several constructors of enums of several constructors, which have no impls of their
        // own.
        let file = [
            table("constructor", "Circle", "crate::Round"),
            table("constructor", "Uuid", "crate::Round"),
        ]
        .concat();
        assert_eq!(
            errors(
                "enum Shape = Circle { radius: Float }, Dot; enum Other = Other(Float), Circle2(Float);
                           enum Uuid = Uuid(String);",
                &file
            ),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_name_that_the_run_does_not_have_is_an_error() {
        assert_eq!(
            errors(TYPES, &table("constructor", "Square", "crate::Square")),
            ["ERROR: The run has no constructor Square (adopt.toml, line 1, col 1)"]
        );
        assert_eq!(
            errors(TYPES, &table("enum", "Circle", "crate::Circle")),
            [
                "ERROR: The run has no enum Circle, but it has a constructor of that name, which is adopted under \
              [rust.constructor] (adopt.toml, line 1, col 1)"
            ]
        );
        assert_eq!(
            errors(
                "enum Shape = Dot, Ring(Float); enum Colour = Red;",
                &table("constructor", "Colour", "crate::C")
            ),
            [
                "ERROR: The run has no constructor Colour, but it has an enum of that name, which is adopted under \
              [rust.enum] (adopt.toml, line 1, col 1)"
            ]
        );
    }

    #[test]
    fn a_constructor_without_arguments_an_enum_with_its_constructor_and_the_prelude_are_errors() {
        assert_eq!(
            errors(TYPES, &table("constructor", "Dot", "crate::Dot")),
            ["ERROR: The constructor Dot has no arguments to convert, so it cannot be adopted on its own; adopt its \
              enum Shape (adopt.toml, line 1, col 1)"]
        );
        let both = [
            table("enum", "Shape", "crate::Shape"),
            table("constructor", "Circle", "crate::Round"),
        ]
        .concat();
        assert_eq!(
            errors(TYPES, &both),
            ["ERROR: The constructor Circle and its enum Shape cannot both be adopted (adopt.toml, line 5, col 1)"]
        );
        assert_eq!(
            errors(TYPES, &table("enum", "constructo.prelude::Option", "crate::Maybe")),
            [
                "ERROR: The enum constructo.prelude::Option is Rust's own, and cannot be adopted \
              (adopt.toml, line 1, col 1)"
            ]
        );
        assert_eq!(
            errors(TYPES, &table("constructor", "constructo.prelude::Some", "crate::Just")),
            ["ERROR: The constructor constructo.prelude::Some is of constructo.prelude::Option, which is Rust's own, \
              and cannot be adopted (adopt.toml, line 1, col 1)"]
        );
    }

    #[test]
    fn two_adoptions_that_would_give_one_type_two_impls_are_an_error() {
        let file = [
            table("constructor", "Uuid", "crate::Same"),
            table("enum", "Unit", "crate::Same"),
        ]
        .concat();
        assert_eq!(
            errors(TYPES, &file),
            [
                "ERROR: Uuid and Unit are both adopted as crate::Same, which would give it two impls of the codec \
              (adopt.toml, line 6, col 8)"
            ]
        );
    }

    #[test]
    fn a_path_that_depends_on_the_module_or_is_not_a_path_is_an_error() {
        let path = |ty: &str| parse(&UUID.replace("uuid::Uuid", ty)).map(|a| a.constructors[0].ty.path.clone());
        assert_eq!(path("Uuid"), Ok("::Uuid".to_string()));
        assert_eq!(path("r#type::Uuid"), Ok("::r#type::Uuid".to_string()));
        assert_eq!(path("::uuid::Uuid"), Ok("::uuid::Uuid".to_string()));
        for bad in ["self::Uuid", "super::ids::Uuid"] {
            let error = path(bad).unwrap_err();
            assert!(
                error.starts_with(&format!(
                    "ERROR: The type of com.example.ids::Uuid is {}, which would mean",
                    bad
                )),
                "{}",
                error
            );
        }
        for bad in [
            "uuid::Uuid<T>",
            "uuid::::Uuid",
            "",
            "uuid::",
            "crate::",
            "1uuid::Uuid",
            "uuid Uuid",
        ] {
            let error = path(bad).unwrap_err();
            assert!(error.contains("which is not a Rust path"), "{}: {}", bad, error);
            assert!(error.contains("(adopt.toml, line 3, col 8)"), "{}", error);
        }
    }
}
