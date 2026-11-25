# The file of adoptions

The file of adoptions names the constructors and enums whose values a program holds in types of its own, in place of
the types that [the codegen command](codegen-command.md) would generate, with the functions that convert between the
two. It is named by `--adopt=FILE_PATH`, and belongs to the person who runs the command, and not to the project, so
that a project that is fetched can never decide what a program builds from data. This document says what the file
holds, what is generated for it, and its errors. See
[decision 55](../decisions/0055-how-a-type-is-adopted-and-why-not-in-the-project-file.md).

## Example

The shapes scenario, `syntax/examples/codegen/shapes/adopt.toml`, adopts a constructor of one argument, a constructor
of two, and a whole enum:

```toml
[rust.constructor."com.example.ids::Uuid"]
type = "foreign::Uuid"
from = "crate::adopt::uuid_from_string"
to = "crate::adopt::uuid_to_string"

[rust.constructor."com.example.shapes::Point"]
type = "foreign::Point"
from = "crate::adopt::point_from"
to = "crate::adopt::point_to"

[rust.enum."com.example.shapes::Colour"]
type = "crate::paint::Colour"
from = "crate::adopt::colour_from_generated"
to = "crate::adopt::colour_to_generated"
```

The program has the functions that it names, as in `tests/codegen-harness/src/adopt.rs`:

```rust
pub fn uuid_from_string(text: String) -> Result<foreign::Uuid, String> { ... }
pub fn uuid_to_string(id: &foreign::Uuid) -> Result<String, String> { ... }
pub fn point_from((x, y): (f64, f64)) -> Result<foreign::Point, String> { ... }
pub fn point_to(point: &foreign::Point) -> Result<(f64, f64), String> { ... }
pub fn colour_from_generated(colour: scenario::Colour) -> Result<Colour, String> { ... }
pub fn colour_to_generated(colour: &Colour) -> Result<scenario::Colour, String> { ... }
```

## Format

The file is in [TOML](https://toml.io).

- **The top-level table is the language.** `rust` is the only one, and any other is an error. A file with no table
  adopts nothing.
- **Under it are `constructor` and `enum`,** and no other table. Each has a table for each adopted name, which is the
  full name of a constructor or an enum of the run, as `com.example.ids::Uuid`, in quotes, since it holds `.` and `::`.
  For types files without a project, a name has no project, and is the bare name, as `Uuid`.
- **Each adopted name has the keys `type`, `from` and `to`,** which are strings, all required, and no other:
  - `type` is the Rust type that holds the values, as a path without generic arguments;
  - `from` is the function from the arguments, or from the generated enum, to the type;
  - `to` is the function from the type back.

The adoptions are kept in the order in which they are written.

### Paths

The three values are Rust paths, which the generator writes into the generated code. The code is in the module of a
project, which may be a submodule, so a path is written so that it does not depend on the module that it is in:

- **A path that starts with `crate::` or `::` is written as it is.**
- **Any other is written with `::` in front,** as the path of another crate, so `uuid::Uuid` is `::uuid::Uuid`. A
  module of the program is named from `crate::`.
- **A path that starts with `self::` or `super::` is an error,** since it would mean something else in each module.
- **A path is a sequence of identifiers joined by `::`.** A segment may be a raw identifier, as `r#type`, and may not be
  `_`. A path with generic arguments, an empty segment or any other character is an error.

## What is generated

**The generated reader still decides each value.** The enum's reader is a `match` over the full names of its
constructors, as for any other enum, so the constructors that the data can make are those that the declarations have,
and adoption changes only which Rust type holds them. The two functions are the one place where code that is not
generated runs on data that is read.

### The functions

- **`from` takes the arguments and returns `Result<T, String>`,** where `T` is the adopted type. The arguments are the
  one argument itself if the constructor has one field or parameter, and a tuple of them in the order of the declaration
  if it has several. An array is one argument, a `Vec`. A field or parameter has the Rust type that it would have in the
  generated `struct`, so `Point(x: Float, y: Float)` is `(f64, f64)`.
- **`to` takes a reference to the value and returns the arguments in the same form,** as `Result<A, String>`. It gives
  them owned, so that it can make arguments that the type does not hold, as a `String` from the bytes of a UUID.
- **For a whole enum,** `from` takes the generated enum, and `to` returns it.
- **A refusal of `from` is an error of reading** at the value, with the function's message, as
  `id: "not-a-uuid" is not a UUID`, followed by the position.
- **A refusal of `to` is an error of writing,** with the path of the value, as
  `shapes[0].colour.0: The colour #336699 has no Constructo form`. The path goes through `Some`, as `.0`, since the
  value is written `Some(...)`.
- **The default of an adopted type,** for an argument that is left out, is made by `from` from the defaults of the
  arguments, and its refusal is an error where the argument is left out: `The default value of argument id is refused:
  "" is not a UUID`.

The generator cannot see that `from` and `to` are inverses. That is the adopter's to get right, as
[decision 54](../decisions/0054-a-mapping-never-changes-a-value-except-a-float.md) requires of every mapping. Rust
checks, when the program is compiled, that the types and functions exist and have the types that the code calls.

### The three ways of adopting

- **A constructor of an enum of several constructors.** Its variant holds the adopted type, in place of the generated
  `struct`, which is not written: `Circle(crate::geometry::Circle)`. The arm of the enum's reader reads the arguments
  and calls `from`, and the arm of its writer calls `to`. The adopted type gets no impl of its own, so one Rust type can
  be adopted for several constructors.
- **The one constructor of an enum,** as `enum Uuid = Uuid(String)`. The enum's Rust type is the adopted type, at every
  use, and nothing is generated for it but a comment in its module, which says that it is adopted and as what. The
  codec's three impls are for the adopted type, so a
  `Vec<foreign::Uuid>`, an `Option<foreign::Uuid>` and a `Styled<foreign::Uuid>` are read and written as well.
- **A whole enum.** The generated enum stays, as the step in between: a value is read into it and given to `from`, and
  the result of `to` is written. Every use of the enum, in its own constructors as well, is the adopted type, and the
  codec's three impls are for the adopted type.

**A generic adopted type has the type parameters that the generated type would keep,** in their order, and the
generator writes them after the path, so a constructor of `Tagged<T>` adopted as `crate::my::Tagged` is
`crate::my::Tagged<T>`. The impl that writes it bounds each parameter by `Clone`, since `to` gives the arguments back
owned. Every generated type and every primitive is `Clone`.

### Adopted types are opaque

The generator cannot see inside an adopted type. Nothing is held through it, or by an adopted constructor, so:

- **a cycle through an adopted type is not boxed,** and its author boxes what it needs;
- **a type that holds itself with other arguments through an adopted type is not refused,** since it is no cycle of the
  generated types;
- **whether an enum has a default does not change,** since an adopted type's default is still made from its arguments.

A `struct` that is not generated, as that of an adopted constructor, gets no warning for a change of its name, since the
program never sees it.

## Errors

Every mistake in the file is an error, with exit code 2, and nothing is written. The message names the file and the
place in it, as the errors of [the configuration file](config-file.md) do:

```text
ERROR: The run has no constructor com.example.ids::Uid (adopt.toml, line 2, col 1)
```

**The file is read before the inputs,** and its first mistake ends the run:

- a file that cannot be read: `ERROR: Cannot read FILE_PATH: REASON`;
- a file that is not valid TOML, or a table or key that is not known, or one that is missing or not a string, in the
  words of the TOML reader, as for the configuration file: ``unclosed table, expected `]` ``,
  ``unknown field `python`, expected `rust` ``, ``unknown field `type`, expected `constructor` or `enum` ``,
  ``unknown field `into`, expected one of `type`, `from`, `to` ``, ``missing field `to` ``, and
  ``invalid type: integer `1`, expected a string``;
- a path that is not one: `The type of com.example.ids::Uuid is "foreign::Uuid<T>", which is not a Rust path, such as
  crate::paint::Colour or uuid::Uuid, without generic arguments`, and `The type of com.example.ids::Uuid is
  self::Uuid, which would mean something else in each module of the generated code; start it with crate:: or with the
  name of a crate`. The function `from` and the function `to` are named so in place of `type`.

**The adoptions are checked against the run** once the inputs have been read and checked, and every mistake is
reported, in the order of the file:

- a name that the run does not have: `The run has no constructor com.example.ids::Uid`, and `The run has no enum
  com.example.shapes::Circle, but it has a constructor of that name, which is adopted under [rust.constructor]`, and
  the same the other way round;
- the prelude's `Bool` and `Option`, which are Rust's own: `The enum constructo.prelude::Option is Rust's own, and
  cannot be adopted`, and `The constructor constructo.prelude::Some is of constructo.prelude::Option, which is Rust's
  own, and cannot be adopted`;
- an enum and one of its constructors, both adopted: `The constructor com.example.shapes::Circle and its enum
  com.example.shapes::Shape cannot both be adopted`;
- a constructor with no body, adopted on its own, which is adopted with its enum: `The constructor
  com.example.shapes::Red has no arguments to convert, so it cannot be adopted on its own; adopt its enum
  com.example.shapes::Colour`;
- two adoptions that would give one Rust type two impls of the codec, which are two enums, or enums of one constructor,
  with the same `type`, at the later of the two: `com.example.ids::Uuid and com.example.shapes::Colour are both adopted
  as crate::Same, which would give it two impls of the codec`. The paths are compared as they are written, so two paths
  to one type are left to Rust's error.

The functests have a file for each of these, in `syntax/examples/codegen/adopt-invalid/`.

## Related

- [The codegen command](codegen-command.md), for the code that is generated and its codec.
- [The configuration file](config-file.md), the other TOML file that the commands read.
- Decisions 52, 54 and 55, in `docs/decisions/`.
