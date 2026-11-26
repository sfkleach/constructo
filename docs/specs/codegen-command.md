# The codegen command

`constructo codegen` writes the code that a program in another language uses to read and write the values of a set of
types: a type in that language for each Constructo type, and a codec that reads the text of data-values into them and
writes them back. This document says what the command reads, what it writes for Rust, what the generated code does,
and what the command prints when something is wrong. Rust is the only language so far.

```text
constructo codegen --lang=rust [--out-file=FILE_PATH] [--adopt=FILE_PATH] [--project=FOLDER] [OPTIONS] [FILES]...
```

For example, a program that keeps the project in `shapes/` and its own code in `src/` writes the file with:

```text
constructo codegen --lang=rust --project=shapes --out-file=src/shapes.rs
```

## Inputs and checking

The inputs are those of [the check command](check-command.md), in the same order: the types files of the project named
by `--project=FOLDER`, then the `FILES`, then the standard input if `--stdin` is given. A run needs at least one of
`--project`, a file and `--stdin`. The `FILES` may hold values files as well as types files, and every types file must
come before every values file. The prelude is loaded unless `--no-prelude` is given.

**The inputs are checked as `check` checks them,** syntax, names, the validity checks, and the types of the consts and
of the values, with `--type-check` and without `--continue`. Any error is reported as `check` reports it, with exit code
1, and nothing is written. A values file gives no code, so values are only checked, as examples that the types must
accept. A project that imports a folder needs its lock, unless `--dev` is given, as for `check`.

**The run is the root project and the projects that it imports.** The root project is the one of `--project`, or the
project with no name of the `FILES` without it. The generated code has the types of every project of the run, and its
codec reads and writes the names and consts of the run and no others.

### The options

| Option                            | Meaning                                                                        |
|-----------------------------------|--------------------------------------------------------------------------------|
| `--lang=LANG`                     | The language to write. It is required, and `rust` is the only one.             |
| `--out-file=FILE_PATH`            | The file to write, for a language that writes one file.                        |
| `--out-dir=FOLDER`                | The folder to write, for a language that writes a folder. Rust does not.       |
| `--adopt=FILE_PATH`               | The file of adoptions: the program's own types for some constructors or enums. |
| `--stdin`                         | Also read the standard input, as one more file, after the others.              |
| `--project=FOLDER`                | The project whose types files come first.                                      |
| `--prelude`, `--no-prelude`       | Whether the prelude is loaded.                                                 |
| `--widening`, `--no-widening`     | Whether a `Nat` fits where an `Int` or `Float` is, and the codec's default.    |
| `--allow-refs`, `--no-allow-refs` | Whether the value of a const may name another const.                           |
| `--infer-limit=N`                 | The most nodes that the type of a const may have.                              |
| `--default-limit=N`               | The most nodes in an omitted argument's default, and the codec's default.      |
| `--dev`, `--no-dev`               | Whether imported folders are left out of the lock check.                       |
| `-x`, `--no-experimental`         | The experimental features of the syntax.                                       |

The options for the types mean what they mean for [the check command](check-command.md), and the keys of `[check]` of a
settings file with the same names, `prelude`, `widening`, `allow-refs`, `infer-limit`, `default-limit` and `dev`, set
them, as for `convert` (see [the configuration file](config-file.md)). `--widening` and `--default-limit` also give the
codec the defaults of its settings, so that it reads what `check` reads with the same options. `--verbose`,
`--print-config`, `--no-type-check` and `--continue` are not accepted. The global options `--config` and `--cache` are.

### Where the code is written

- **A language writes one file or a folder,** and the option says which is meant: `--out-file` or `--out-dir`. Rust
  writes one file, so `--out-dir` is a usage error, exit code 2, which names the right option: `ERROR: Rust writes one
  file: use --out-file=FILE_PATH, or neither for the standard output`. With `--out-file` as well, the message starts
  `ERROR: --out-file and --out-dir cannot both be given.` See
  [decision 50](../decisions/0050-a-target-writes-one-file-or-a-folder.md).
- **Without `--out-file`, the code is written to the standard output.** If it cannot be written, the run ends with exit
  code 2, as for `convert`, and with no message if the reader has gone.
- **The file of `--out-file` is replaced as a whole.** The code is written to a new file beside it, which then takes
  its place by a rename, so that the file is always the old one or the new one, and never part of either. A file that
  cannot be written is `ERROR: Cannot write FILE_PATH: REASON`, with exit code 2.
- **Nothing is written if there is an error,** in the inputs, in the file of adoptions, or in a type that the language
  cannot hold, since the code is made in memory first. A file of `--out-file` is then left as it was.
- **Warnings are written whatever happens to the output,** to the standard error, and do not change the exit code.

The language and the output options are checked before anything is read, then the file of adoptions is read, then the
inputs are read and checked, then the adoptions are checked against the run, and then the code is made.

## What is written for Rust

The file is a module of the program's crate, which the program declares with `mod shapes;` and uses as
`shapes::codec()`. It starts with a header that says where it came from and what it needs:

```rust
//! Generated by constructo codegen from the project com.example.shapes. Do not edit.
//! It needs the crate constructo-runtime 0.1.
#![allow(non_camel_case_types, non_snake_case, dead_code, clippy::all)]
```

Without a project, the first line says `from types files`. The version is the major and minor version of `constructo`,
since the two are released together, and a program adds `constructo-runtime` of that version to its own `Cargo.toml`.
The code compiles with Rust 2021 and Rust 2024.

### Modules

**The file is the module of the root project,** and every other project of the run is a module in it, named by the
parts of its full name joined with `_`, with their case kept, so `com.example.ids` is `com_example_ids`. The prelude has
no module, since its `Bool` and `Option` are Rust's own. A project with no types has an empty module, so that a path to
it does not depend on what it declares. See
[decision 49](../decisions/0049-how-the-modules-and-names-in-rust-are-chosen.md).

**Every path in the file is written in full,** as `::std::vec::Vec`, `::std::primitive::u64`, `self::Shape` or
`super::com_example_ids::Uuid`, so that no name of the declarations can hide one that the file needs. A type parameter
is the only name that is written bare.

### The mapping of the types

| A Constructo type                     | Is in Rust                                                             |
|---------------------------------------|------------------------------------------------------------------------|
| `Nat`                                 | `u64`                                                                  |
| `Int`                                 | `i64`                                                                  |
| `Float`                               | `f64`                                                                  |
| `String`                              | `String`                                                               |
| `[T]`                                 | `Vec<T>`                                                               |
| `Bool`                                | `bool`                                                                 |
| `Option<T>`, `T?`                     | `Option<T>`                                                            |
| an enum of one constructor            | a `struct`, named after the enum, with the body of the constructor     |
| an enum of several constructors       | an `enum`, with a variant for each constructor, in the order declared |
| a record type without a name          | a `struct`, named after its fields                                     |
| a type parameter                      | a type parameter                                                       |

- **A variant of a constructor that has a body** holds a `struct` of the constructor, which has its name and is in the
  same module: `Circle(self::Circle)`. A constructor with no body is a variant with no data, as `Dot`, and an enum of
  one such constructor is a `struct` with no fields.
- **The body of a constructor** is a `struct` with named fields for a record, and for a tuple that has a named
  parameter, in which a parameter with no name is a field named by its position, `_0`, `_1` and so on. A tuple whose
  parameters have no names is a tuple `struct`, and an array is a tuple `struct` of one `Vec`.
- **Every type derives `Debug`, `Clone` and `PartialEq`,** and every field is `pub`. Each has a doc comment that names
  the Constructo type, as `/// The enum `com.example.shapes::Shape`.`
- **The order of the constructors is kept,** since the first constructor of an enum is its default.
- **A record type without a name** is one `struct` for each distinct type, named by its field names, sorted and joined
  with `_`, or `empty` for `{}`, and numbered `_2`, `_3` and so on where two have the same name. It is written in the
  module of each project that uses it. So `{ dx: Int, dy: Int }` is `dx_dy`. See
  [decision 48](../decisions/0048-the-model-of-the-declarations-and-how-a-record-type-without-a-name-is-named.md).
- **A type parameter that no field holds is left out** of the Rust type, and out of every use of it, so
  `enum Phantom<_T> = Phantom` is `struct Phantom`, and `Phantom<Nat>` is `Phantom`. A parameter is kept if a field
  holds it, directly, in an array, or as the argument of a kept parameter of another type.

### Numbers

Each number has the range of its Rust type, and a value outside it is refused, when it is read and when it is written,
as [decision 54](../decisions/0054-a-mapping-never-changes-a-value-except-a-float.md) says:

- **A `Nat` that does not fit a `u64`, and an `Int` that does not fit an `i64`,** are refused when they are read:
  `The number 18446744073709551616 does not fit Nat`. Every `u64` and `i64` is written exactly.
- **A `Float` is the nearest `f64`,** which is the one change that a mapping is allowed. A finite number too large for
  an `f64`, such as `1.0e400`, is refused, and one too small to be held other than as zero is zero. There is no negative
  zero, so `-0.0` is read and written as `0.0`.
- **A `Float` is written as the shortest text that reads back as the same `f64`,** with `.0` put in where it has no
  point, as `1.0` and `1.0e300`. A value that is not finite has no text yet, so writing one is an error: `The Float NaN
  cannot be written, since Constructo has no text for it yet`.
- **An `Int` that is not negative is written with `+`,** as `+5`, so that it reads back as an `Int` without widening.

### Recursion

**A field or parameter that holds its own type by value, on a cycle, is a `Box`,** at every site, so that the type is
finite: the `struct` of `Neg(Expr)`, in `enum Expr`, holds a `Box<Expr>`. A boxed `Option` has the box inside, as
`Option<Box<T>>`. An array holds nothing by value, since a `Vec` keeps its elements apart, so `Polygon[Point]` has no
box.
Every field on a cycle is boxed, so that the type of a field depends only on whether it is on a cycle.

**A type that holds itself with other arguments is refused,** since Rust cannot hold it, though `check` accepts it.
The error is at its declaration, with exit code 1, and nothing is written: `Rust cannot hold the enum Nested, since it
holds itself with other arguments`. A type that holds itself with the same arguments, as `List<T>` holds a `List<T>`,
is written.

### Names

**Names are kept where they can be, and every change is a warning at the declaration,** as
[decision 49](../decisions/0049-how-the-modules-and-names-in-rust-are-chosen.md) says:

- A keyword of Rust 2021, or a word reserved by it or by Rust 2024, is a raw identifier, as `r#gen`, which is not a
  change. A name that cannot be raw, such as `self`, `Self` and `_`, gets a `_` at the end, repeated while it is taken.
- A named parameter that has a name of the form `_0` gets a `_`, since those name the parameters that have none.
- A constructor that has the name of an enum of its project, its own included, takes the name of its own enum in front,
  as `ShapeCircle`, and a number after that if that is taken too. An enum keeps its name.
- A type with a name that the generated code adds, `Codec`, `ReadValue`, `WriteValue` or `DefaultValue`, gets a `_`.
- A module whose name is taken, by another module or a type of the root, is numbered, and the other keeps its name.
- A type parameter that is left out, and whose name does not start with `_`, is a warning too, though it is not a
  change of name.

The warnings start `WARNING:`, are given in the order of the files and the places in them, and say what the name is in
Rust and why, as:

```text
WARNING: The enum Codec is Codec_ in Rust, since Codec is a name that the generated code adds (n.types, line 8, col 6)
WARNING: The constructor Holder is HolderHolder in Rust, since the enum Holder has its name (n.types, line 26, col 18)
WARNING: The parameter T of Holder is left out in Rust, since no field holds it (n.types, line 26, col 13)
```

The other forms are `The field F of C is R in Rust`, `The record type N is R in Rust`, `The parameter P of E is R in
Rust`, `The variant of the constructor C is R in Rust` and `The project P is the module M in Rust, since M is taken`.

## The codec

The file has one codec, for the run, and the program reads and writes through it. The types are plain data, and have no
`parse` or `to_text` of their own, since the names in the text depend on a project and a type does not have one. See
[decision 51](../decisions/0051-a-codec-for-each-project-holding-its-names-consts-and-settings.md).

```rust
let codec = shapes::codec();
let drawing: shapes::Drawing = codec.read(text)?;
let text = codec.write(&drawing)?;
```

- **`codec()`** makes a new `Codec`, with the default settings. A codec is cheap, and each call makes a new one, so a
  program that changes a setting changes it only for itself.
- **`read::<T>(&self, text)`** reads the one value of the text as a `T`. A text with more or fewer values is an error
  at line 1, column 1: `Expected one value but found 2`.
- **`read_all::<T>(&self, text)`** reads every value of the text, each as a `T`, into a `Vec<T>`.
- **`write(&self, &value)`** writes a value as text, ended by `;` and a new line.
- **`write_all(&self, &[value])`** writes each of the values so, one after the other. An error has the index of its
  value at the front of its path, as `[2].layer`.

Each returns a `Result` whose error is `constructo_runtime::codec::Error`. The text that is written is laid out as
[`convert --from`](convert-command.md#reading-back-with---from) lays it out.

**The codec reads and writes with three traits** that the file declares, `ReadValue<T>`, `WriteValue<T>` and
`DefaultValue<T>`, which the codec implements for each type of the run, and for `u64`, `i64`, `f64`, `String`, `bool`,
`Vec<T>`, `Option<T>` and `Box<T>`. A program calls `read` and `write`, and not the traits, which need `Codec:
ReadValue<T>` and so on. The traits are the file's own, so a program can implement them for a type of any crate. The
impls for the types of a project are in its module.

### Reading is constrained by the type

**The program names the type, and the data can only be what the type permits,** as
[decision 52](../decisions/0052-reading-is-constrained-by-the-target-type.md) says. The constructors that reading can
build are a closed set, fixed when the code is generated: those of the run, generated from the declarations or adopted
in the file of adoptions. There is no registry that can be added to while the program runs, and no lookup of a type by
a name in the data. So a constructor of another type is an error, `Expected Shape but found Point of type Point`, and a
name that the run does not have is one too, `Unknown constructor Hexagon`.

### Names in the text

**A codec reads every form of a name that a values file of the root project reads, and no other,** as
[decision 53](../decisions/0053-names-are-written-as-the-project-writes-them.md) says: the full name, as
`com.example.shapes::Circle`, the name through each group whose scope has it, as `ids::Uuid` and `local::Circle`, and
the bare name if `local` has it, each the new name where an import renames it. The forms are worked out by the resolver
that `check` uses, when the code is generated (see [resolving identifiers](resolving-identifiers.md)). So in the
scenario, where `com.example.ids` is imported into the group `ids`, `Uuid("...")` is `Unknown constructor Uuid`, as it
is an unknown name for `check`.

**A codec writes each name as the root project writes it,** by default: bare if the name resolves to that constructor in
the root's scope, else through the first group in the project-file through which it is visible, else in full. It is
the form that `convert` writes. The setting `names` changes it.

### Consts

**A reference to a const is read as the const's value,** which is read as the type that is expected where the
reference is. The values of the consts of the run are in the file as text, with every name in full, and each is parsed
once by each codec, when it is first referred to. So a generic const such as the prelude's `null`, and a `Nat` const
where an `Int` is expected, need nothing extra. A const name that the run does not have is `Unknown const unit_square`.

**An error inside a const's value is at the reference,** with the const named, as `in the value of const unit: Nesting
is too deep`, and the position of the reference. A chain of consts is followed at the same depth as the reference, so
the depth limit stops a chain that is too long.

**A const whose value Rust cannot hold is refused when the code is generated,** with an error at the const and exit
code 1, so that it is not left to fail on the first read: `Rust cannot hold the value of the const big: The number
99999999999999999999999 does not fit Nat`.

**The writer never writes a reference to a const.** It writes the value.

### Defaults

**An argument that is left out of a tuple takes the default of its type,** as
[decision 18](../decisions/0018-an-omitted-named-argument-takes-the-default-of-its-type.md) says, and as `check` and
`convert` take it: `0` for a `u64` and an `i64`, `0.0` for an `f64`, the empty `String` and `Vec`, `false`, `None`, the
`struct` of the defaults of its fields, and the first constructor of an enum with the defaults of its own parts. It is
the codec's `DefaultValue`, and not Rust's `Default`, whose rules differ.

**A default that cannot be given is an error where the argument is left out,** with the path of the parameter and the
position of the constructor, and the messages of `check`:

- a type whose default would expand for ever, as an enum whose first constructor leads back to it:
  `Type List has no default value`;
- a default with more nodes than the setting `default_limit`: `The default value of argument items has more than 10000
  nodes, which is too large; give argument items`;
- a default of an adopted type that its function refuses: `The default value of argument id is refused: "" is not a
  UUID`.

**The writer writes every argument,** by name if the parameter has one, including those that are equal to their
defaults, so that the text does not depend on the defaults.

### The settings of a codec

A setting is a property of the codec, read by a method of its name and changed by `set_` and its name, with `&mut self`.

| Setting         | Type    | Default                  | Meaning                                                     |
|-----------------|---------|--------------------------|-------------------------------------------------------------|
| `names`         | `Names` | `Names::Project`         | Whether names are written as the root writes them, or full. |
| `max_depth`     | `u32`   | 200, the parser's limit  | The deepest nesting read or written, through consts.        |
| `default_limit` | `usize` | `--default-limit`, 10000 | The most nodes in the default of an omitted argument.       |
| `widening`      | `bool`  | `--widening`, `true`     | Whether a number is read where a wider one is expected.     |

The defaults of `default_limit` and `widening` are the values of the options of the run that wrote the file, and are
shown with the defaults of the options.

- **`max_depth` is at most 200,** the parser's limit, and `set_max_depth` with more sets 200, since the parser could not
  read back what is deeper. Each value is a level, a number included, as the parser counts them. A value that is too
  deep, read or written, is `Nesting is too deep`. The writer applies it too, so that a value that a program has built
  too deep is an error, and not an overflow of the stack.
- **Without a project, `Names::Project` and `Names::Full` are the same,** since a name has nothing to be relative to.
- **`Names`** is `constructo_runtime::codec::Names`.

### Errors

An error of reading has the position in the text, and both kinds have the path in the value, from the outside in, by
field name, by the position of a parameter that has no name, and by index:

```text
Drawing(ids::Uuid("67e55044-10b1-426f-9247-bb680e5fe0c8"), "T", [Styled { item: Circle { radius: 1.0e400 }, ... }]);
```

is refused, when it is read as a `Drawing`, with:

```text
shapes[0].item.radius: The number 1.0e400 does not fit Float (line 1, col 98)
```

The error is shown as `PATH: in the value of const C: MESSAGE (line L, col C)`, with each part only where there is one.
An error of writing has no position. `Error` has the methods `message()`, `pos()`, `path()` and `consts()`, for a
program that wants the parts, and implements `std::error::Error`.

**The messages are `check`'s,** where the error is one that `check` finds: `Expected Nat but found String`, `Missing
field dy in record`, `Unknown field depth in Box`, `Duplicate field width`, `Unknown argument colour in Image`,
`Argument hOffset is given more than once in Image`, and `Constructor Image takes 5 value(s) but has 1`. The errors of
the syntax are the parser's. The errors that only a codec finds are the numbers that do not fit, the `Float` that cannot
be written, `Nesting is too deep`, `Unknown const C`, and `Constructor C takes no fields`, `a record`, `a tuple` or `an
array` for a constructor written with the wrong body.

**The generated code never panics** on a text, or on a value that the program is allowed to hold, and it never writes
something in place of what it cannot write.

## Adoption

**A program can have its own types hold the values of some constructors and enums,** in place of the generated ones, by
naming them in a file that `--adopt=FILE_PATH` names, with a function from the arguments to the type and one back. The
closed set of constructors is kept, since the generated reader still decides each value. The file, what is generated
for it, and its errors are in [the file of adoptions](adopt-file-format.md). See
[decision 55](../decisions/0055-how-a-type-is-adopted-and-why-not-in-the-project-file.md).

## What is not supported

- **serde.** The generated types do not derive `Serialize` or `Deserialize`. A program that wants them implements them,
  and a generation setting may derive them later. JSON and the other notations are what `convert` is for.
- **Rust items for the consts.** A program has its own constants, and a function for each would bring back the
  questions of width, `Box` and generics for little use. One that returns a `Result` can be added later.
- **A tree with the names resolved,** as a `read_tree`. The codec reads from text, and the runtime's tree is not a
  public promise yet. It is left to the first program that asks.
- **A project shared between two runs.** Each run has the types of all its projects, so two files that both import
  `com.example.ids` each have a module of it, and their types are not the same Rust types.
- **The `Drop` of a deep value.** Rust drops a value recursively, so a value that a program builds very deep, such as a
  long chain of a list, can overflow the stack when it is dropped. The codec reads nothing deeper than `max_depth`, so
  this is only for what the program builds, and is the program's to manage.
- **Default arguments when a program builds a value.** Rust has none, and the generated types have no `impl Default`.
  A language that has them may, as
  [decision 56](../decisions/0056-a-generated-class-may-give-a-named-parameter-its-default.md) says.

## Exit codes

- **Exit code 0:** the code was written, with warnings or without, and `--help`.
- **Exit code 1:** an error of `check` in the inputs, a type that Rust cannot hold, and a const whose value Rust cannot
  hold.
- **Exit code 2:** an option or an argument that is not understood, a language that is not known (`ERROR: Unknown
  language python: the languages are rust`), `--out-dir` for Rust, a file of adoptions that cannot be read or has a
  mistake, an input that cannot be read, an output that cannot be written, and the problems with a project-file or with
  how projects are found, as for `check`.

See [the exit codes](exit-codes.md).

## Related

- [The file of adoptions](adopt-file-format.md), for `--adopt`.
- [The check command](check-command.md), for the inputs and the checks.
- [The convert command](convert-command.md), whose typed formats read the same inputs and write the same names.
- [Type checking](type-checking.md), for the defaults and what a number is, and
  [resolving identifiers](resolving-identifiers.md), for the forms of a name.
- [The exit codes](exit-codes.md) and [the configuration file](config-file.md).
- Decisions 18 and 46 to 57, in `docs/decisions/`.
