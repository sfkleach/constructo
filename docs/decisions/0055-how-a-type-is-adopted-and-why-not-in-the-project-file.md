# 0055 - How a type is adopted, and why not in the project-file, 2026-11-25

## Issue

A program often has a type of its own for a value that a Constructo project declares: a `Uuid` of a crate, a date, a
colour with more than the declared constructors. The generated code of
[decision 51](0051-a-codec-for-each-project-holding-its-names-consts-and-settings.md) writes a type for every
constructor and enum, and a program that wanted its own would have to convert by hand at every use. The question was
how a program has its own type hold the values of a constructor or an enum, without giving up the closed set of
[decision 52](0052-reading-is-constrained-by-the-target-type.md), and who says which types are adopted.

This is a **Leap**, from the draft of the codegen task, written after the code, which the codegen harness runs with the
adoptions of the scenario. A spike, which compiled the generated code by hand with a type of another crate, came first.

## Factors

- The constructors that reading can build are a closed set, fixed by the declarations, and adoption must not open it.
- A value that has the right form can still be one that the program's type cannot hold, such as a string that is not a
  UUID, and a value of the program's type can be one that the declarations have no constructor for.
- The project-file and the types files belong to the author of a project, who is often not the person who uses it, and
  a fetched project could otherwise decide what an importer's program builds from data.
- The adopted type is often of another crate, and the generated code is in modules of the program's crate.
- The generator cannot see inside an adopted type, nor inside the functions that convert it.

## Decision

**What is adopted is named in a file of the person who runs the command,** `--adopt=FILE_PATH`, and not in the
project-file or the types files. It is TOML, with a table for the language, `rust`, and under it `constructor` and
`enum`, each with a table for each adopted name, which is the full name of a constructor or an enum of the run. Each has
`type`, `from` and `to`:

```toml
[rust.constructor."com.example.ids::Uuid"]
type = "uuid::Uuid"
from = "crate::adopt::uuid_from_string"
to = "crate::adopt::uuid_to_string"
```

**The enum stays generated, and a match over its constructors decides each value,** so adoption changes only which Rust
type holds the arguments of a constructor, and the closed set holds as before. The two functions are the one place
where code that is not generated runs on data that was read:

- `from` takes the arguments and returns `Result<T, String>`: the one argument itself, or a tuple of them in the order
  of the declaration if there are several. Its refusal is an error of reading at the value, as
  `id: "not-a-uuid" is not a UUID (line 1, col 9)`.
- `to` takes a reference to the value and returns the arguments in the same form, or a refusal, which is an error of
  writing with the path of the value, as `shapes[0].colour.0: The colour #336699 has no Constructo form`.
- The default of an adopted type, for an argument that is left out, is made by `from` from the defaults of the
  arguments, and its refusal is `The default value of argument id is refused: "" is not a UUID`.

**There are three ways in which a type is adopted:**

- **A constructor of an enum of several constructors:** its variant holds the adopted type, and the arm of the enum's
  reader calls `from`. It has no impl of its own, so one Rust type can be adopted for several constructors.
- **The one constructor of an enum,** as `Uuid(String)`: the enum's Rust type is the adopted type, at every use, and
  the codec's three impls are for it.
- **A whole enum:** the generated enum stays, as the step in between. The value is read into it, and `from` converts
  it, and the other way round. Every use of the enum, its own constructors included, is the adopted type.

**A generic adopted type has the parameters that the generated one would keep,** in their order, and the generator
writes them after the path. The impl that writes it bounds each parameter by `Clone`, since `to` gives the arguments
back owned, and must be able to clone them. Every generated type and every primitive is `Clone`. *Added while it was
built.*

**A path is written so that it does not depend on the module:** one that starts with `crate::` or `::` as it is, and any
other with `::` in front, as the path of another crate, so `uuid::Uuid` is `::uuid::Uuid`. One that starts with `self::`
or `super::`, or is not a path of identifiers, is an error.

**An adopted type is opaque.** Nothing is held through it, or by an adopted constructor, so a cycle through it is not
boxed and is not refused as irregular, and its author boxes what it needs. Whether an enum has a default does not
change, since an adopted type's default is still made from its arguments.

**Every mistake in the file is an error, with exit code 2 and nothing written:** a file that cannot be read or is not
valid TOML; a language, table or key that is not known, or one that is missing; a name that the run does not have; a
constructor with no arguments, adopted on its own, which is adopted with its enum; an enum and one of its constructors,
both adopted; the prelude's `Bool` and `Option`, which are Rust's own; two adoptions that would give one Rust type two
impls; and a path that is not one.

## Consequences

- A program can hold values in its own types and in those of other crates, and the data still can only make the
  constructors that the declarations have.
- The functions are named in one file, where they can be reviewed, and the generator writes only calls to them.
- The generator cannot see that `from` and `to` are inverses. A `from` that makes a UUID lower case changes one that
  was written in upper case. That is the adopter's to get right, as
  [decision 54](0054-a-mapping-never-changes-a-value-except-a-float.md) requires of every mapping.
- The adopted types must exist and have the functions with the types that the generated code calls, which Rust checks
  when the program is compiled, and not the generator.
- Writing an adopted generic type needs its arguments to be `Clone`.

## Pros and Cons of Options

### Where the adoptions are

- **In a file of the person who runs the command.** The choice of which types of a program may be built from data is
  that program's. A fetched project can decide nothing.
- **In the project-file.** One place for a project, but the author of a project would decide what an importer's program
  builds, which is what the closed set is there to prevent.

### How `to` gives the arguments

- **Owned, with a bound of `Clone` on the parameters of a generic adopted type.** A function can make arguments that the
  type does not hold, as a `String` from a UUID's bytes.
- **By reference.** No bound, but a function could only give what the type already holds, which a UUID does not.
