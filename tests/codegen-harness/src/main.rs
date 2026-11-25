//! The harness of the codegen functests, which runs code that `constructo codegen --lang=rust` generated and holds it
//! to `constructo check` and `constructo convert`. The functests generate its three modules:
//!
//! - `schema`, from `syntax/examples/values/typed/schema.types`, for the corpus of typed values;
//! - `scenario`, from the project `syntax/examples/codegen/shapes/shapes`, with the adoptions of
//!   `syntax/examples/codegen/shapes/adopt.toml`, whose functions are in `adopt`, of the types of `paint` and of the
//!   crate `foreign`, which stands in for a crate of a third party;
//! - `names`, from the project `syntax/examples/codegen/names-project/app`.
//!
//! It is run as `codegen-harness CONSTRUCTO EXAMPLES OUT`, with the binary of `constructo`, the folder
//! `syntax/examples`, and a folder for the files that it writes, and it prints a line for each test, `PASS NAME` or
//! `FAIL NAME: WHY`, which the functests count.

mod adopt;
mod names;
mod paint;
mod scenario;
mod schema;

use constructo_runtime::codec::{self, Error, Names};
use schema::{Cell, Cells, Config, Holder, Image, Job, JoinList, Linked, List, Shape};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reads a value of a type as the codec of the schema does, and gives what it writes for it, as the project writes
/// names and in full, or why it could not.
type Round = fn(&str) -> Result<(String, String), String>;

/// What the harness expects of a value of the corpus, with the type that it reads it as.
enum Entry {
    /// It is read, and what is written for it is held to `check` and `convert`.
    Read(Round),
    /// It is refused.
    Refused(Round),
    /// `check` refuses it only because it cannot work out a type, which the harness gives, so it is read.
    Accepted(Round),
    /// It has no Rust type, since it is a record without a name that no declaration has.
    Skip,
}

/// Reads a value as a `T`, writes it as the project writes names and in full, and reads both back as the same value.
fn round<T: PartialEq + Debug>(text: &str) -> Result<(String, String), String>
where
    schema::Codec: schema::ReadValue<T> + schema::WriteValue<T>,
{
    let codec = schema::codec();
    let mut full = schema::codec();
    full.set_names(Names::Full);
    let value: T = codec.read(text).map_err(|e| e.to_string())?;
    let written = codec.write(&value).map_err(|e| format!("writing: {}", e))?;
    let in_full = full.write(&value).map_err(|e| format!("writing in full: {}", e))?;
    for text in [&written, &in_full] {
        let again: T = codec
            .read(text)
            .map_err(|e| format!("reading back {}: {}", text.trim_end(), e))?;
        if again != value {
            return Err(format!(
                "{} reads back as {:?}, not {:?}",
                text.trim_end(),
                again,
                value
            ));
        }
    }
    Ok((written, in_full))
}

macro_rules! read {
    ($t:ty) => {
        Entry::Read(round::<$t>)
    };
}

macro_rules! refused {
    ($t:ty) => {
        Entry::Refused(round::<$t>)
    };
}

/// The type of each value of each file of `values/typed/valid`, in order.
fn valid() -> Vec<(&'static str, Vec<Entry>)> {
    vec![
        (
            "config.values",
            vec![read!(Config), read!(Config), read!(Vec<f64>), Entry::Skip],
        ),
        (
            "consts.values",
            vec![read!(Shape), read!(Vec<Shape>), read!(List<Shape>)],
        ),
        (
            "generics.values",
            vec![read!(List<f64>), read!(List<String>), read!(Option<List<bool>>)],
        ),
        (
            "inferred-consts.values",
            vec![read!(Shape), read!(Config), read!(Vec<Vec<String>>)],
        ),
        (
            "named-arguments.values",
            vec![read!(Image), read!(Image), read!(Image), read!(Image)],
        ),
        (
            "omitted-arguments.values",
            vec![
                read!(Image),
                read!(Image),
                read!(Image),
                read!(Job),
                read!(Job),
                read!(Cells),
            ],
        ),
        ("omitted-named-argument.values", vec![read!(Image)]),
        (
            "prelude-consts.values",
            vec![
                read!(Option<u64>),
                read!(bool),
                read!(Vec<bool>),
                read!(Vec<Option<u64>>),
                read!(Option<Option<u64>>),
                read!(Config),
            ],
        ),
        (
            "qualified-names.values",
            vec![read!(Shape), read!(Shape), read!(Shape), read!(f64), read!(List<u64>)],
        ),
        (
            "shapes.values",
            vec![read!(Shape), read!(Shape), read!(Shape), read!(Shape), read!(Shape)],
        ),
        (
            "tagged-arrays.values",
            vec![
                read!(schema::Names),
                read!(schema::Names),
                read!(JoinList<u64>),
                read!(JoinList<u64>),
                read!(JoinList<u64>),
                read!(JoinList<f64>),
                read!(Holder),
                read!(Holder),
            ],
        ),
    ]
}

/// The type of the value of each file of `values/typed/invalid`.
fn invalid() -> Vec<(&'static str, Vec<Entry>)> {
    vec![
        ("argument-given-twice.values", vec![refused!(Image)]),
        ("array-for-bare-constructor.values", vec![refused!(Shape)]),
        ("array-for-record-variant.values", vec![refused!(Shape)]),
        ("array-for-tuple-variant.values", vec![refused!(Shape)]),
        ("array-variant-for-array.values", vec![refused!(Config)]),
        ("array-variant-mixed-generic.values", vec![refused!(JoinList<u64>)]),
        ("array-variant-wrong-element.values", vec![refused!(schema::Names)]),
        (
            "array-variant-wrong-generic-element.values",
            vec![refused!(JoinList<u64>)],
        ),
        ("bare-array-variant.values", vec![refused!(schema::Names)]),
        ("bool-for-nat.values", vec![refused!(Shape)]),
        ("const-wrong-type.values", vec![refused!(Shape)]),
        ("dot-with-parentheses.values", vec![refused!(Shape)]),
        ("inconsistent-array.values", vec![Entry::Skip]),
        ("inferred-const-wrong-type.values", vec![refused!(Shape)]),
        ("int-as-nat.values", vec![refused!(Shape)]),
        ("missing-field.values", vec![refused!(Shape)]),
        ("mixed-list.values", vec![refused!(List<u64>)]),
        ("name-for-unnamed-parameter.values", vec![refused!(Image)]),
        ("named-argument-wrong-type.values", vec![refused!(Image)]),
        ("null-for-nat.values", vec![refused!(Shape)]),
        ("omitted-argument-recursive-default.values", vec![refused!(Linked)]),
        (
            "omitted-argument-unknown-type.values",
            vec![Entry::Accepted(round::<Cell<u64>>)],
        ),
        ("omitted-unnamed-parameter.values", vec![refused!(Image)]),
        ("record-for-array-variant.values", vec![refused!(schema::Names)]),
        ("tuple-for-array-variant.values", vec![refused!(schema::Names)]),
        ("unknown-argument.values", vec![refused!(Image)]),
        ("unknown-const-in-local.values", vec![refused!(Shape)]),
        ("unknown-const.values", vec![refused!(Shape)]),
        ("unknown-constructor-in-local.values", vec![refused!(Shape)]),
        ("unknown-constructor.values", vec![refused!(Shape)]),
        ("unknown-field.values", vec![refused!(Shape)]),
        ("unknown-group.values", vec![refused!(Shape)]),
        ("unknown-project.values", vec![refused!(Shape)]),
        ("wrong-arity.values", vec![refused!(Shape)]),
        ("wrong-field-type.values", vec![refused!(Shape)]),
        ("wrong-type-in-tail.values", vec![refused!(List<u64>)]),
    ]
}

struct Harness {
    constructo: PathBuf,
    examples: PathBuf,
    out: PathBuf,
}

impl Harness {
    fn pass(&self, name: &str) {
        println!("PASS {}", name);
    }

    fn fail(&self, name: &str, why: &str) {
        println!("FAIL {}: {}", name, why.replace('\n', " "));
    }

    /// Passes or fails a test by whether it is true.
    fn expect(&self, name: &str, ok: bool, why: impl FnOnce() -> String) {
        if ok {
            self.pass(name);
        } else {
            self.fail(name, &why());
        }
    }

    /// Runs `constructo` with the arguments, and gives its output if it succeeds, or its error if not.
    fn constructo(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.constructo)
            .args(args)
            .output()
            .map_err(|e| format!("cannot run {}: {}", self.constructo.display(), e))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    /// Writes a file in the output folder, and gives its path.
    fn file(&self, name: &str, text: &str) -> String {
        let path = self.out.join(name);
        std::fs::write(&path, text).unwrap_or_else(|e| panic!("cannot write {}: {}", path.display(), e));
        path.display().to_string()
    }

    fn path(&self, relative: &str) -> String {
        self.examples.join(relative).display().to_string()
    }

    /// Every value of the corpus is read as the table says, and what is written is held to `check` and `convert`.
    fn corpus(&self) {
        let typed = self.examples.join("values/typed");
        let schema = typed.join("schema.types").display().to_string();
        for (kind, table) in [("valid", valid()), ("invalid", invalid())] {
            // The table and the folder have the same files, so that a file that is added is not missed.
            let mut files: Vec<String> = std::fs::read_dir(typed.join(kind))
                .expect("the folder of the corpus")
                .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".values"))
                .collect();
            files.sort();
            let mut listed: Vec<String> = table.iter().map(|(file, _)| file.to_string()).collect();
            listed.sort();
            self.expect(
                &format!("the harness has the types of every file of {}", kind),
                files == listed,
                || format!("the folder has {:?} and the table {:?}", files, listed),
            );
            for (file, entries) in table {
                let text = std::fs::read_to_string(typed.join(kind).join(file)).expect("a file of the corpus");
                let values = codec::parse(&text).expect("a file of the corpus parses");
                if values.len() != entries.len() {
                    let why = format!("it has {} values and the table {}", values.len(), entries.len());
                    self.fail(&format!("corpus {}/{}", kind, file), &why);
                    continue;
                }
                for (at, (value, entry)) in values.into_iter().zip(entries).enumerate() {
                    let name = format!("corpus {}/{} value {}", kind, file, at + 1);
                    let text = codec::write_text(&[value]);
                    match entry {
                        Entry::Skip => {}
                        Entry::Read(round) => match round(&text) {
                            Ok((written, in_full)) => {
                                let base = format!("{}-{}-{}", kind, file.trim_end_matches(".values"), at + 1);
                                self.oracle(&name, &schema, &base, &text, &written, &in_full);
                            }
                            Err(why) => self.fail(&name, &why),
                        },
                        Entry::Refused(round) => match round(&text) {
                            Ok((written, _)) => self.fail(&name, &format!("accepted, as {}", written.trim_end())),
                            Err(_) => self.pass(&name),
                        },
                        Entry::Accepted(round) => match round(&text) {
                            Ok(_) => self.pass(&name),
                            Err(why) => self.fail(&name, &why),
                        },
                    }
                }
            }
        }
    }

    /// What the codec wrote for a value is accepted by `check`, and `convert --to=json-typed` gives the same for it as
    /// for the value, which compares the two with their consts and defaults filled in. JSON cannot hold every value,
    /// as it cannot hold `Some(None)`, and then the codec's reading back of what it wrote is the test.
    fn oracle(&self, name: &str, schema: &str, base: &str, original: &str, written: &str, in_full: &str) {
        let original = self.file(&format!("{}.original.values", base), original);
        let mut why = Vec::new();
        let expected = self.constructo(&["convert", "--to=json-typed", schema, &original]).ok();
        for (form, text) in [("", written), (".full", in_full)] {
            let path = self.file(&format!("{}{}.values", base, form), text);
            if let Err(e) = self.constructo(&["check", schema, &path]) {
                why.push(format!("check refuses {}: {}", text.trim_end(), e));
                continue;
            }
            if let Some(expected) = &expected {
                match self.constructo(&["convert", "--to=json-typed", schema, &path]) {
                    Ok(json) if json == *expected => {}
                    Ok(json) => why.push(format!(
                        "{} is {} in JSON, not {}",
                        text.trim_end(),
                        json.trim(),
                        expected.trim()
                    )),
                    Err(e) => why.push(format!("convert refuses {}: {}", text.trim_end(), e)),
                }
            }
        }
        if why.is_empty() {
            self.pass(name);
        } else {
            self.fail(name, &why.join("; "));
        }
    }

    /// Two codecs of two runs, each with a type called `Shape`, in one program, each bounded by its run.
    fn two_codecs(&self) {
        let a: Result<schema::Shape, Error> = schema::codec().read("Dot");
        let b: Result<scenario::Shape, Error> = scenario::codec().read("Dot");
        self.expect("two codecs read their own Shape", a.is_ok() && b.is_ok(), || {
            format!("{:?} and {:?}", a, b)
        });
        let a = schema::codec().read::<schema::Shape>("Polygon[]");
        let b = scenario::codec().read::<scenario::Shape>("Pair(1, +2)");
        let refused = |r: Result<String, Error>, message: &str| r.is_err_and(|e| e.message() == message);
        self.expect(
            "a codec reads only the constructors of its run",
            refused(a.map(|v| format!("{:?}", v)), "Unknown constructor Polygon")
                && refused(b.map(|v| format!("{:?}", v)), "Unknown constructor Pair"),
            || "a constructor of the other run was read".to_string(),
        );
    }

    /// The scenario's drawing is read, written, and read back, and what is refused is refused at the value.
    fn scenario(&self) {
        let project = self.path("codegen/shapes/shapes");
        let codec = scenario::codec();
        let text = std::fs::read_to_string(self.examples.join("codegen/shapes/shapes/values/drawing.values"))
            .expect("the drawing of the scenario");
        let drawing: scenario::Drawing = match codec.read(&text) {
            Ok(drawing) => drawing,
            Err(e) => return self.fail("the scenario's drawing is read", &e.to_string()),
        };
        self.pass("the scenario's drawing is read");
        let mut full = scenario::codec();
        full.set_names(Names::Full);
        for (form, codec, uuid) in [
            ("", &codec, "ids::Uuid("),
            (" in full", &full, "com.example.ids::Uuid("),
        ] {
            let name = format!("the scenario's drawing is written{} and read back", form);
            let written = match codec.write(&drawing) {
                Ok(written) => written,
                Err(e) => return self.fail(&name, &e.to_string()),
            };
            let again: Result<scenario::Drawing, Error> = codec.read(&written);
            let path = self.file(&format!("drawing{}.values", form.replace(' ', "-")), &written);
            let checked = self.constructo(&["check", &format!("--project={}", project), &path]);
            self.expect(
                &name,
                again.as_ref() == Ok(&drawing) && written.contains(uuid) && checked.is_ok(),
                || format!("{} read back as {:?}, and check gives {:?}", written, again, checked),
            );
        }
        let id = r#"ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8")"#;
        let styled = |item: &str| format!("Styled {{ item: {}, colour: null, offset: {{ dx: 0, dy: 0 }} }}", item);
        let refusals = [
            (
                format!(r#"Drawing({}, "T", [{}])"#, id, styled("Point(1.0, 2.0)")),
                "shapes[0].item: Expected Shape but found Point of type Point",
            ),
            (
                format!(r#"Drawing({}, "T", [{}])"#, id, styled("Hexagon")),
                "shapes[0].item: Unknown constructor Hexagon",
            ),
            (
                r#"Drawing(Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"), "T")"#.to_string(),
                "id: Unknown constructor Uuid",
            ),
            // The function of an adoption refuses a value that has the right form and is not valid, at the value.
            (
                r#"Drawing(ids::Uuid("not-a-uuid"), "T")"#.to_string(),
                r#"id: "not-a-uuid" is not a UUID"#,
            ),
            // And the default of an argument that is left out, which is made by the same function.
            (
                r#"Drawing(title: "T")"#.to_string(),
                r#"id: The default value of argument id is refused: "" is not a UUID"#,
            ),
            (
                format!(r#"Drawing({}, "T", [], 18446744073709551616)"#, id),
                "layer: The number 18446744073709551616 does not fit Nat",
            ),
            (
                format!(r#"Drawing({}, "T", [{}])"#, id, styled("Circle { radius: 1.0e400 }")),
                "shapes[0].item.radius: The number 1.0e400 does not fit Float",
            ),
            (
                format!(r#"Drawing({}, "T", [{}])"#, id, styled("unit_square")),
                "shapes[0].item: Unknown const unit_square",
            ),
            (
                format!(
                    r#"Drawing({}, "T", [Styled {{ item: Dot, colour: null, offset: {{ dx: 0 }} }}])"#,
                    id
                ),
                "shapes[0].offset: Missing field dy in record",
            ),
        ];
        for (text, expected) in refusals {
            let found = codec.read::<scenario::Drawing>(&text);
            let shown = found.as_ref().map_err(|e| format!("{}: {}", e.path(), e.message()));
            self.expect(
                &format!("the scenario refuses {}", expected),
                shown.as_ref().err().map(String::as_str) == Some(expected),
                || format!("{} gives {:?}", text, shown),
            );
        }
        let mut huge = drawing.clone();
        huge.shapes[0].item = scenario::Shape::Circle(scenario::Circle { radius: 1e300 });
        huge.shapes[1].item = scenario::Shape::Circle(scenario::Circle { radius: 1e-7 });
        let back = codec
            .write(&huge)
            .and_then(|text| codec.read::<scenario::Drawing>(&text));
        self.expect(
            "large and small floats are written and read back",
            back.as_ref() == Ok(&huge),
            || format!("{:?}", back),
        );
        self.adopted(&codec, &drawing);
    }

    /// The adopted types are read and written wherever they are: in a `Vec`, in an `Option`, as the argument of a
    /// generic type, and with the arguments of a constructor of two parameters given to the function as a tuple.
    fn adopted(&self, codec: &scenario::Codec, drawing: &scenario::Drawing) {
        let id = r#"ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8")"#;
        let other = r#"com.example.ids::Uuid("00000000-0000-0000-0000-000000000001")"#;
        self.adopted_round::<Vec<foreign::Uuid>>(codec, "a Vec of a foreign type", &format!("[{}, {}]", id, other));
        self.adopted_round::<Option<foreign::Uuid>>(codec, "an Option of a foreign type", &format!("Some({})", id));
        self.adopted_round::<scenario::Styled<foreign::Uuid>>(
            codec,
            "a foreign type as the argument of a generic type",
            &format!(
                "Styled {{ item: {}, colour: Some(Blue), offset: {{ dx: 1, dy: 2 }} }}",
                id
            ),
        );
        self.adopted_round::<scenario::Styled<Vec<foreign::Point>>>(
            codec,
            "a foreign type of two arguments, in a Vec, as the argument of a generic type",
            "Styled { item: [Point(1.0, 2.0), Point(y: 3.0)], colour: null, offset: { dx: 0, dy: 0 } }",
        );
        let mut custom = drawing.clone();
        custom.shapes[0].colour = Some(paint::Colour::Custom(0x336699));
        let written = codec.write(&custom).map_err(|e| e.to_string());
        self.expect(
            "a colour of the program's that has no Constructo form is not written",
            written
                .as_ref()
                .is_err_and(|e| e == "shapes[0].colour.0: The colour #336699 has no Constructo form"),
            || format!("{:?}", written),
        );
    }

    /// Reads a value as a `T`, writes it, and reads that back as the same value.
    fn adopted_round<T: PartialEq + Debug>(&self, codec: &scenario::Codec, name: &str, text: &str)
    where
        scenario::Codec: scenario::ReadValue<T> + scenario::WriteValue<T>,
    {
        let read = codec.read::<T>(text);
        let back = read
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|value| codec.write(value))
            .and_then(|written| codec.read::<T>(&written));
        self.expect(name, matches!((&read, &back), (Ok(a), Ok(b)) if a == b), || {
            format!("{:?}, read back as {:?}", read, back)
        });
    }

    /// The names of a project with a group that renames and hides, and another group, are read in every form that the
    /// project reads, and written as the project writes them.
    fn names(&self) {
        let codec = names::codec();
        let text = "Scene([g::Round, h::Circle, h::Square, example.lib::Triangle, g::ring])";
        let written = codec.read::<names::Scene>(text).and_then(|scene| codec.write(&scene));
        let expected = "Scene([g::Round, g::Round, h::Square, g::Triangle, g::Round]);\n";
        let path = self.file("scene.values", written.as_deref().unwrap_or_default());
        let project = format!("--project={}", self.path("codegen/names-project/app"));
        let checked = self.constructo(&["check", "--dev", &project, &path]);
        self.expect(
            "a project's names are read in every form and written as it writes them",
            written.as_deref() == Ok(expected) && checked.is_ok(),
            || format!("{:?}, and check gives {:?}", written, checked),
        );
        for (name, message) in [
            ("g::Circle", "Unknown constructor g::Circle"),
            ("g::Square", "Unknown constructor g::Square"),
            ("Round", "Unknown constructor Round"),
            ("g::unit", "Unknown const g::unit"),
        ] {
            let found = codec.read::<names::Scene>(&format!("Scene([{}])", name));
            self.expect(
                &format!("a project does not read {}", name),
                found.as_ref().is_err_and(|e| e.message() == message),
                || format!("{:?}", found),
            );
        }
    }

    /// The settings of a codec, and its limits.
    fn limits(&self) {
        let mut codec = schema::codec();
        // `[unit]` is three levels deep through the const: the array, the `Circle` and its radius.
        codec.set_max_depth(3);
        let three = codec.read::<Vec<Shape>>("[unit]");
        codec.set_max_depth(2);
        let two = codec.read::<Vec<Shape>>("[unit]");
        self.expect(
            "the depth of a value is counted through a const",
            three.is_ok()
                && two
                    .as_ref()
                    .is_err_and(|e| e.message() == "Nesting is too deep" && e.consts() == ["unit"]),
            || format!("{:?} and {:?}", three, two),
        );
        codec.set_max_depth(1000);
        self.expect(
            "the depth is at most the parser's limit",
            codec.max_depth() == constructo_runtime::limits::MAX_DEPTH,
            || codec.max_depth().to_string(),
        );
        let chain = |links: usize| {
            (0..links).fold(schema::Chain::End, |next, _| {
                schema::Chain::Link(schema::Link {
                    next: std::boxed::Box::new(next),
                })
            })
        };
        let codec = schema::codec();
        let deep = codec.write(&chain(300));
        let shallow = codec
            .write(&chain(150))
            .and_then(|text| codec.read::<schema::Chain>(&text));
        self.expect(
            "a value deeper than the limit is not written, and one within it is read back",
            deep.as_ref().is_err_and(|e| e.message() == "Nesting is too deep") && shallow == Ok(chain(150)),
            || format!("{:?}", deep.map(|_| "written")),
        );
        let mut codec = schema::codec();
        let roomy = codec.read::<Job>(r#"Job("build")"#);
        codec.set_default_limit(2);
        let tight = codec.read::<Job>(r#"Job("build")"#);
        self.expect(
            "a default larger than the limit is refused where it is left out",
            roomy.is_ok()
                && tight.as_ref().is_err_and(|e| {
                    e.message()
                        .starts_with("The default value of argument limits has more than 2 nodes")
                }),
            || format!("{:?} and {:?}", roomy, tight),
        );
        let mut codec = schema::codec();
        codec.set_widening(false);
        let narrow = codec.read::<Shape>("Circle { radius: 3 }");
        self.expect(
            "without widening a Nat is not read as a Float",
            narrow
                .as_ref()
                .is_err_and(|e| e.message() == "Expected Float but found Nat"),
            || format!("{:?}", narrow),
        );
        let codec = schema::codec();
        let nan = codec.write(&Shape::Circle(schema::Circle { radius: f64::NAN }));
        self.expect(
            "a Float that is not finite is not written",
            nan.as_ref()
                .is_err_and(|e| e.path() == "radius" && e.message().starts_with("The Float NaN")),
            || format!("{:?}", nan),
        );
        let all = codec
            .read_all::<Shape>("Dot; Circle { radius: 1.0 }")
            .and_then(|shapes| codec.write_all(&shapes));
        self.expect(
            "every value of a text is read and written",
            all.as_deref() == Ok("Dot;\nCircle { radius: 1.0 };\n"),
            || format!("{:?}", all),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, constructo, examples, out] = &args[..] else {
        eprintln!("usage: codegen-harness CONSTRUCTO EXAMPLES OUT");
        std::process::exit(2);
    };
    let harness = Harness {
        constructo: PathBuf::from(constructo),
        examples: PathBuf::from(examples),
        out: Path::new(out).to_path_buf(),
    };
    std::fs::create_dir_all(&harness.out).expect("the output folder");
    harness.corpus();
    harness.two_codecs();
    harness.scenario();
    harness.names();
    harness.limits();
}
