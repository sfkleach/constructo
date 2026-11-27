# Scenario: a drawing program that reads and writes shapes

This is an imagined use of `constructo codegen`, worked through to the code that
it would write, to test the design in
[codegen-subcommand-for-rust.md](codegen-subcommand-for-rust.md) against a
program that has to use it.

It is imagined, but it is not untested. The Constructo side is real: the
projects and the values file below are checked by `constructo check`. The
generated code is written by hand, as `codegen` would write it, and it compiles
and runs against the real parser of `constructo`, which stands in for
`constructo-runtime`. The text that it writes is accepted by `check`, and it is
the same, byte for byte, as what `convert --from=json-typed` writes for the same
value. Everything is in the folder [scenario-shapes](scenario-shapes/), and how
to run it is at the end.

Where the design did not say what to do, the code takes a choice and the choice
is listed under [What the scenario found](#what-the-scenario-found).

## The program and its data

A drawing program keeps its drawings as Constructo. A drawing has an id, which is
a UUID, a title, and a list of shapes, each with a colour and an offset. The
program already has a type for colours of its own, `paint::Colour`, which has a
`Custom` colour that the format has no name for.

The ids are a project of their own, `com.example.ids`, since other programs use
them as well
([projects/ids/ids.types](scenario-shapes/projects/ids/ids.types)):

```
// An identifier, written as the text of a UUID.
enum Uuid = Uuid(String);
```

The shapes are `com.example.shapes`, which imports the ids into the group `ids`
([projects/shapes/shapes.types](scenario-shapes/projects/shapes/shapes.types)):

```
enum Shape =
  Circle { radius: Float },
  Box { width: Float, height: Float },
  Dot,
  Polygon[Point];

enum Point = Point(x: Float, y: Float);

enum Colour = Red, Green, Blue;

enum Styled<T> = Styled { item: T, colour: Colour?, offset: { dx: Int, dy: Int } };

enum Drawing = Drawing(id: ids::Uuid, title: String, shapes: [Styled<Shape>], layer: Nat);

const unit_circle = Circle { radius: 1.0 };
```

Between them they have each kind of constructor (a record, a tuple with names,
one with no body, an array), a generic enum, an optional field, a record type
with no name, a const, the prelude, and a type of another project.

A drawing, written by hand
([projects/shapes/values/drawing.values](scenario-shapes/projects/shapes/values/drawing.values)):

```
Drawing(
  ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"),
  "Sketch",
  [
    Styled { item: unit_circle, colour: Some(Red), offset: { dx: 0, dy: 0 } },
    Styled { item: Polygon[Point(0.0, 0.0), Point(x: 4.0), Point(y: 3.0)], colour: null, offset: { dy: -2, dx: +5 } }
  ]
);
```

It uses a const (`unit_circle`), the prelude's `null`, a name through a group
(`ids::Uuid`), arguments by position and by name, arguments that are left out
(`layer`, and `y` and `x` of two points), and the fields of a record in another
order. `check --project=.` accepts it.

The program adopts two types ([adopt.toml](scenario-shapes/adopt.toml)): the
constructor `Uuid` as `uuid::Uuid`, which is a type of another crate, and the
whole enum `Colour` as its own `paint::Colour`.

```toml
[rust.constructor."com.example.ids::Uuid"]
type = "uuid::Uuid"
from = "crate::adopt::uuid_from_string"
to = "crate::adopt::uuid_to_string"

[rust.enum."com.example.shapes::Colour"]
type = "crate::paint::Colour"
from = "crate::adopt::colour_from_generated"
to = "crate::adopt::colour_to_generated"
```

And the code is generated with:

```
constructo codegen --lang=rust --project=shapes --adopt=adopt.toml > src/shapes.rs
```

## What is generated

The whole file is
[rust/app/src/shapes.rs](scenario-shapes/rust/app/src/shapes.rs), about 730
lines. It is the module of the root project, and it is in four parts.

### The types

```rust
pub enum Shape {
    Circle(Circle),
    Box(Box),
    Dot,
    Polygon(Polygon),
}

pub struct Circle { pub radius: f64 }
pub struct Box { pub width: f64, pub height: f64 }
pub struct Polygon(pub ::std::vec::Vec<Point>);

// `enum Point = Point(x: Float, y: Float);` has one constructor of the same name, so it is one struct.
pub struct Point { pub x: f64, pub y: f64 }

// Adopted whole as `crate::paint::Colour`. This enum is the step in between.
pub enum Colour { Red, Green, Blue }

pub struct Styled<T> {
    pub item: T,
    pub colour: ::std::option::Option<crate::paint::Colour>,
    pub offset: dx_dy,
}

// The record type `{ dx: Int, dy: Int }`.
pub struct dx_dy { pub dx: i64, pub dy: i64 }

pub struct Drawing {
    pub id: ::uuid::Uuid,
    pub title: ::std::string::String,
    pub shapes: ::std::vec::Vec<Styled<Shape>>,
    pub layer: u64,
}

// com.example.ids has only `Uuid`, which is adopted, so nothing is generated in it.
pub mod com_example_ids {}
```

The constructor `Box` hides `std::boxed::Box` in this module, which is the case
that the rule of writing std items in full is for.

### The codec

```rust
pub struct Codec { limits: rt::Limits }

pub fn codec() -> Codec { ... }

impl Codec {
    pub fn read_all<T: FromValue>(&self, text: &str) -> R<Vec<T>>;   // every value of a text
    pub fn read<T: FromValue>(&self, text: &str) -> R<T>;            // the one value of a text
    pub fn write<T: ToValue>(&self, value: &T) -> R<String>;
    fn resolve(&self, value: &Value, depth: usize) -> R<Value>;      // names and consts
}
```

It has three tables, each a `match`:

- `full_name`, from each form of a name that com.example.shapes can write to its
  full name (decision 42): `"Circle" | "local::Circle" | "com.example.shapes::Circle" =>
  "com.example.shapes::Circle"`, and `"ids::Uuid" | "com.example.ids::Uuid" =>
  "com.example.ids::Uuid"`. A bare `Uuid` is not there, since the project does
  not have it bare, and `check` refuses it too.
- `written`, from a full name to the form that the project writes: bare
  where it is in scope, and otherwise the full name.
- `constant`, from each form of the name of a const to its value, as text with
  full names: `unit_circle` is `"com.example.shapes::Circle { radius: 1.0 }"`,
  and `null` is `"constructo.prelude::None"`.

Reading parses the text, then `resolve` makes a copy of the tree with every name
in full and every const replaced by its value, and then `FromValue` reads the
copy. Writing is the reverse.

### The traits

`FromValue`, `ToValue` and `DefaultValue` are generated in this file, with
their impls for `u64`, `i64`, `f64`, `String`, `bool`, `Vec<T>` and
`Option<T>`. They are of the program's crate, so they can be implemented for
`uuid::Uuid`, and the spike reads a `Vec<uuid::Uuid>` to show that the orphan
rule allows it.

### The impls

Each enum is a `match` over its constructors, by their full names, which is the
closed set:

```rust
impl FromValue for Shape {
    fn from_value(value: &Value) -> R<Self> {
        let ValueKind::Tagged { name, body } = &value.kind else {
            return Err(rt::expected("Shape", value));
        };
        match (name.as_str(), body) {
            ("com.example.shapes::Circle", Some(TaggedBody::Record(members))) => {
                let [radius] = rt::fields(members, ["radius"], name, value.pos)?;
                Ok(Shape::Circle(Circle { radius: f64::from_value(radius)? }))
            }
            // Box and Polygon are the same.
            ("com.example.shapes::Dot", None) => Ok(Shape::Dot),
            ("com.example.shapes::Circle" | "com.example.shapes::Box", _) => Err(rt::takes(name, "a record", value)),
            // ...
            _ => Err(rt::expected("Shape", value)),
        }
    }
}
```

A tuple with named parameters binds the arguments, and gives an argument that is
left out the default of its type:

```rust
("com.example.shapes::Point", Some(TaggedBody::Tuple(args))) => {
    let [x, y] = rt::arguments(args, [Some("x"), Some("y")], name, value.pos)?;
    Ok(Point { x: argument(x, "x", "Point", value)?, y: argument(y, "y", "Point", value)? })
}
```

An adopted constructor reads its arguments and gives them to the adopted
function, and an adopted enum reads the generated enum and converts it:

```rust
impl FromValue for ::uuid::Uuid {
    fn from_value(value: &Value) -> R<Self> {
        // ...
        ("com.example.ids::Uuid", Some(TaggedBody::Tuple(args))) => {
            let [text] = rt::arguments(args, [None], name, value.pos)?;
            let text: String = FromValue::from_value(text.expect("a parameter without a name is never left out"))?;
            crate::adopt::uuid_from_string(text).map_err(|why| rt::at(value.pos, why))
        }
        // ...
    }
}

impl FromValue for crate::paint::Colour {
    fn from_value(value: &Value) -> R<Self> {
        crate::adopt::colour_from_generated(Colour::from_value(value)?).map_err(|why| rt::at(value.pos, why))
    }
}
```

### The runtime

[rust/runtime/src/lib.rs](scenario-shapes/rust/runtime/src/lib.rs) stands in for
`constructo-runtime`. It re-exports the parser, the printer, the AST and `Pos`
of `constructo`, and adds helpers that need no trait: an `Error` with a
position, `fields` and `arguments`, which bind a record or a tuple with the
messages of `check`, `rewrite`, which copies a tree with its names changed, and
builders for writing. It has no traits.

## The program

The program's own code is the two adopted types and their functions
([adopt.rs](scenario-shapes/rust/app/src/adopt.rs),
[paint.rs](scenario-shapes/rust/app/src/paint.rs)), and its use of the codec
([main.rs](scenario-shapes/rust/app/src/main.rs)):

```rust
let codec = shapes::codec();
let drawing: Drawing = codec.read(&text)?;
let written = codec.write(&drawing)?;
```

## What happens when it runs

It reads the drawing, writes it, and reads what it wrote, which is the same
drawing. What it writes:

```
Drawing(
    id: com.example.ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"),
    title: "Sketch",
    shapes: [
        Styled {
            item: Circle { radius: 1.0 },
            colour: Some(Red),
            offset: { dx: +0, dy: +0 }
        },
        Styled {
            item: Polygon[Point(x: 0.0, y: 0.0), Point(x: 4.0, y: 0.0), Point(x: 0.0, y: 3.0)],
            colour: None,
            offset: { dx: +5, dy: -2 }
        }
    ],
    layer: 0
);
```

The const is written as its value, the arguments that were left out are written
with their defaults, every argument is written by name, and the `Int`s have a
sign. `check --project=.` accepts it.

What it refuses, each with the position of the value:

| The text has                              | The error                                                              |
|-------------------------------------------|------------------------------------------------------------------------|
| `Point(1.0, 2.0)` where a `Shape` is      | `Expected Shape but found Point`                                       |
| `Hexagon`, which is not in the run        | `Unknown constructor Hexagon`                                          |
| a bare `Uuid`, which the project has not  | `Unknown constructor Uuid`, as `check` says                            |
| `ids::Uuid("not-a-uuid")`                 | `"not-a-uuid" is not a UUID: ...`, from the adopted function           |
| `Drawing(title: "T")`, with no id         | `Argument id of Drawing is left out, and its type has no default: ...` |
| a `Nat` of `18446744073709551616`         | `The number 18446744073709551616 does not fit Nat`                     |
| a `Float` of `1.0e400`                    | `The number 1.0e400 does not fit Float`                                |
| `{ dx: 0 }` for `{ dx: Int, dy: Int }`    | `Missing field dy in record`, as `check` says                          |
| `unit_square`, which is not a const       | `Unknown const unit_square`                                            |

Writing a drawing with a `paint::Colour::Custom` colour fails with `The colour
#336699 has no Constructo form`, from the adopted function.

`check` accepts `Drawing(title: "T")`, `ids::Uuid("not-a-uuid")` and `1.0e400`,
which the Rust code refuses. That is what adoption and the choice of `u64`,
`i64` and `f64` are for: the program accepts less than the language does, and
says why.

## What the scenario found

1. **A type and a constructor of the same name clash in Rust.** In Constructo
   they are in separate namespaces (decision 4), and `enum Point = Point(...)` is
   the usual way to write a type with one constructor; four of the five enums of
   the scenario do it. In Rust a type and a `struct` share one namespace. The
   code takes the choice that an enum with one constructor of the same name is
   one `struct`, which is what a Rust programmer would write.
   *Decided since:* an enum with one constructor is one `struct` named after the
   enum, whatever its constructor is called; a constructor called like another
   enum takes the name of its own enum in front; and the names that the
   generator adds (`Codec`, `codec`, `FromValue`, `ToValue`, `DefaultValue`) are
   fixed, and a Constructo name yields to them. The generated file of the spike
   still has the aliases `R` and `rt`, which the decision drops, for brevity.
2. **A tuple whose parameters all have names is a `struct` with named fields.**
   The draft's table says a tuple `struct`. With names the fields are readable,
   and they are what the writer writes. A tuple with some names and some not is
   still to be decided.
   *Decided since:* a tuple with no names is a tuple `struct`, one with names is
   a `struct` with named fields, and in one with some of each the unnamed
   parameters are named by position, `_0`, `_1` and so on.
3. **A constructor with no body is a variant with no data,** as `Shape::Dot`, and
   not a unit `struct` as the table says. Such a constructor has no arguments, so
   there is nothing in it to adopt.
   *Decided since:* a constructor with no body is a variant with no data, and it
   cannot be adopted on its own; its enum can be.
4. **Writing can fail.** A `Float` that is not finite has no text, and an adopted
   type can have a value with no Constructo form, as `Custom` has. The code makes
   `write` return a `Result`, and the `to` functions of both kinds of adoption
   return one too. The draft says that the `to` function of a constructor cannot
   fail.
   *Decided since:* `write` returns a `Result`, the `to` function of both kinds
   of adoption may fail, and an error of writing carries a path in the value,
   such as `shapes[0].colour`. The spike's errors of writing have no path yet.
5. **The full name is written where a group would do.** The project can write
   `ids::Uuid`, and the code writes `com.example.ids::Uuid`, because the draft
   says "bare, or else the full name", which is what `convert` does too. The
   name through a group is shorter and is accepted by `check` in the project. It
   is a candidate for a second choice before the full name.
   *Decided since:* a name is written bare, else through a group, else in
   full, and `convert` is to do the same, in a task of its own.
6. **A Rust `f64` is not always a Constructo `Float`.** Rust writes `1e300`, and a
   Constructo `Float` needs a point, as in `1.0e300`; `1e300` is a syntax error.
   The first version of the spike had this bug, and the writer now puts in the
   `.0`. A test that writes large and small floats and reads them back is in the
   program.
7. **The value of a const is held with full names.** A const of a project that is
   imported is written in that project's names, which the codec of the root may
   not resolve. Holding it with full names, which resolve in any project of the
   run, avoids it. The positions of the value are moved to the reference, so that
   an error in it is reported where the const is used.
8. **The resolution is a pass of its own,** which copies the tree before
   `FromValue` reads it. It is simple, and it costs a copy of each value. Doing
   it as the tree is read is an option for later, if the copy matters.
   *Decided since:* names and consts are resolved as the value is read and
   written, by traits that the codec implements for each type
   (`ReadValue<T>`, `WriteValue<T>`, `DefaultValue<T>`), so `self` is the codec.
   The spike predates this: its traits are on the value types, and it has a pass.
9. **The key between the codec and the impls is the full name.** The first
   version of the spike used the checker's own form, `Circle@com.example.shapes`,
   which is internal and specified nowhere. Any key that is unique in the run
   and the same on both sides would do, since the generator writes both sides,
   and the full name, `com.example.shapes::Circle`, is that and is already part
   of the language (decision 42). The spike now uses it.
10. **Dependencies can leave nothing to generate.** `com.example.ids` has only an
    adopted type, so its module is empty, and the scenario does not test the
    naming of modules or paths that start with `super::`.

## How to run it

From the root of the repository, with the build in `_build/` so that nothing is
written in `docs/`:

```
cd docs/tasks/done/2026-11-17-codegen-subcommand-for-rust/scenario-shapes/projects/shapes
CARGO_TARGET_DIR=../../../../../../_build/scenario cargo run --offline \
    --manifest-path ../../rust/Cargo.toml -- values/drawing.values ../../../../../../_build/scenario/written.values
constructo check --project=. ../../../../../../_build/scenario/written.values
```

`cargo` writes a `Cargo.lock` next to the `Cargo.toml`, which is not kept. The
program needs the `uuid` crate, which `--offline` finds only if it is in the
local cache.

## What is not real

- There is no `codegen`. The generated file is written by hand, and the choices
  in it are those of the list above.
- The runtime is the code of `constructo` behind a few helpers, and not a crate
  of its own.
- `adopt.toml` is not read by anything. It is what the generated code was
  written from.
- The messages for a value that is refused are those of `check` where `check`
  has one, and new ones where it does not. They are not compared with `check`
  by a test.
