# Task: Implement the `codegen` subcommand for Rust

This is a draft, written from a conversation and not from the code. Anything
marked **Open** is not decided, and every claim about what the tool does now is
to be checked against the code before it is relied on.

## Overview

The `codegen` subcommand writes source code in another language from the
data-types, so that a program in that language can **serialise its data into
Constructo values and deserialise Constructo values back into its data**,
reliably. It is part of giving Constructo the capability that JSON has in
programs today, which is that a program can write its data out and read it in
again, without Constructo borrowing anything from JSON.

Rust is the first target, because its `enum` is nearly the same thing as a
Constructo `enum`. The command is meant to have more targets later, chosen with
an option.

```
constructo codegen --lang=rust shapes.types > shapes.rs
constructo codegen --lang=rust --project=. > src/model.rs
```

The name is `codegen` and not `generate`, since `generate` does not say what is
generated, and in a tool that also converts data it could be read as making
sample values. If the command ever writes something that is not code, such as a
schema or documentation, the name is to be looked at again.

## Why

A program that wants to write its data as Constructo, or to read Constructo
data, has to have types that match the declarations, and has to keep them in
step with the declarations by hand. Generating them removes the copy that can
drift, and lets the code that reads the data be the code that the declarations
describe and nothing more.

The data is Constructo's own syntax, the values that `check` reads. JSON is not
involved: `convert` is the tool for other notations, and `codegen` is not built
on it and does not depend on what it writes.

The test that needs no new oracle is `check`. What the generated code writes for
a value has to be accepted by `check` with the same types, and a value that
`check` accepts has to be read by the generated code as that value.

## The rule that matters most

**Reading is constrained by the target type.** Unlike JSON, which has no types,
a value of Constructo is read into a type that the caller asks for, and the data
can only be something that the type permits. Nothing in the data can name a type
to build, so there is no way to point it at a type that was not meant. This is
the answer to the problem of reading data into arbitrary types, and the
generator has to keep it.

So the constructors that can be invoked by reading data are **a closed set,
fixed when the code is generated, and nothing else**:

- Every type and constructor that the reader can build is one that the generator
  wrote out from the declarations, or one that was adopted explicitly (below).
- There is no registry that can be added to while the program runs, no lookup of
  a type by a name that is in the data, and no open type such as a trait object,
  a tagged registry, or an "any" type. The generator refuses to write one.
- The data holds only data. It has no code, no callbacks and no references to
  anything outside the document.

The word is "constrained" and not "driven". Decision 39 says "driven by a type",
which describes how the reader is written and not what the data may do. The
decision and the docs may be reworded to match when this task is done.

### Adopting types that are not generated

Every constructor has a Rust type, and a Constructo `enum` becomes a Rust `enum`
with one variant for each of its constructors, each holding the type of that
constructor. The type of a constructor is **either generated or adopted**: a
type that the program already has, such as a `Uuid` or a date, is adopted in
place of the one that would have been written. The adoption has to be explicit.

```rust
struct Circle { r: f64 }                   // generated
enum Shape { Circle(Circle), Dot }         // always generated
```

A constructor with no body, such as `Dot`, is a variant with no data, since it
has no arguments to hold or to convert (below). It cannot be adopted on its own.

The enum is always generated, so the set of constructors that the data can
select is fixed by the declarations, and adopting a type can only change which
Rust type holds the arguments of one constructor. It cannot add a constructor.

An adopted constructor is still declared in the types, so that `check` and the
other commands see it, and a value of it is written in the syntax of the
declaration. For `enum Uuid = Uuid(String);` the value is `Uuid("...")`. What the
adoption adds is the Rust type, and two functions that the person who adopts it
writes. The **arguments** are the parameters of that one constructor: the
parameter itself if there is one, a tuple of them if there are several, and the
fields of a record constructor.

- **To deserialise**, a function from the arguments to the adopted type. For
  `Uuid` it takes a `String` and returns a `uuid::Uuid`, or an error.
- **To serialise**, a function from the adopted type to the arguments, or an
  error. For `Uuid` it takes a `uuid::Uuid` and returns a `String`, and it never
  fails. It may fail because an adopted type can have a value that the
  declarations have no form for, and the function must be able to say so rather
  than write something else in its place (see writing, below).

The functions do not know about the other constructors of the enum, and a
single-constructor enum such as `Uuid` is the plain case of the same thing.

The rules:

- Only a constructor that the adoptions list is adopted. It is never found by
  looking for a type that happens to fit, and it never comes from the data.
- The two functions are the one place where code that is not generated runs on
  data that was read, and they are named in the adoptions, where they can be
  reviewed. The generator writes only calls to them. That the function that
  deserialises can fail is a use and not a nuisance: it is where a value that
  has the right shape and is not valid, such as a string that is not a UUID, is
  refused. Its error is reported as a read error at that value.
- If a type needs an adoption and none is given, the generator stops with an
  error. It does not guess.
- A constructor with no body cannot be adopted on its own, since it has no
  arguments for the functions to convert, and an adoption of one is an error
  that says to adopt its enum. Adopting the enum covers it, as for `Colour` in
  the [scenario](scenario-shapes.md). An enum whose only constructor has no
  body, `enum Unit = Unit;`, is the `struct Unit;` of an enum with one
  constructor, and is adopted as an enum.

A constructor of a generic `enum` can be adopted too. Its Rust type has the
type parameters that the constructor uses, as in the section on generics below.

**A whole `enum` can be adopted too.** Adopting a constructor changes only what
a variant holds, so a field whose type is the enum, as in `color: Color`, still
has the generated `Color`. Adopting the enum makes every use of it a use of the
program's own type, such as an existing Rust `Color` or `std::cmp::Ordering`.

- **The generated enum stays, as a step in between.** The data is read into the
  generated enum, by the `match` over its fixed list of constructors, so the
  closed set holds as before. Two functions of the person who adopts it then
  convert: one from the generated enum to the program's type, and one from the
  program's type back to the generated enum. Either may fail: the program's
  `paint::Colour` has a `Custom` colour that the enum has no constructor for.
- **The functions never see the text and never choose a constructor.** The
  generated code has already decided which constructor a value is, and the
  functions only map one Rust type to another.
- **A generic enum** is adopted by a Rust type with the same parameters, in the
  same order, and the functions are generic over them, as for a constructor.
- **An enum and one of its constructors cannot both be adopted.** That is an
  error in the first version, since what it would mean is not clear and nobody
  needs it yet.

**Every adopted type is handled by the codec.** The generated code has to
convert an adopted type wherever it occurs, in a `Vec`, in an `Option` or as the
argument of a generic type, and in Rust that needs a trait that the type
implements. The adopted type is often of another crate, as `uuid::Uuid` is, and
Rust's orphan rule forbids an impl of a trait of another crate for a type of
another crate. So the traits are not in the runtime crate: they are generated
with the codec, in the program's own crate, where an impl for any type is
allowed (see the codec below).

**Adoptions are not in the project-file.** The project-file belongs to the owner
of the project, and a person who uses a project is very likely not that owner.
An adoption is a choice by whoever consumes the types: it says which types of
their own program may be built from data. The author of a project that is
imported cannot make that choice for the importer, and a fetched project that
could adopt types would be able to decide what the importer's program
constructs, which is what the closed set is there to prevent. For the same
reason the types files, which are also the project's, hold no adoption.

So adoptions are in **a file of the person who runs the command**, which is not
the project-file and is named by the option `--adopt=FILE_PATH`. It is not the
settings file of `--config`: the keys of that file are the options of the
command line, and `check --print-config` writes a template that has every one of
them, whereas an adoption is data and not a setting.

### The format of the file

**Open:** the format. For the moment it is TOML, as the settings file is, and
this section is a sketch of what that would look like and not a design.

```toml
# Adoptions for the Rust target. The name of a table is the full name of a
# Constructo constructor or enum, and its keys say which Rust type is adopted.
[rust.constructor."com.example.ids::Uuid"]
type = "uuid::Uuid"
from = "crate::adopt::uuid_from_string"
to = "crate::adopt::uuid_to_string"

[rust.enum."com.example.colours::Colour"]
type = "crate::paint::Colour"
from = "crate::adopt::colour_from_generated"
to = "crate::adopt::colour_to_generated"
```

with, in the program, functions of these forms:

```rust
pub fn uuid_from_string(text: String) -> Result<uuid::Uuid, String> { ... }
pub fn uuid_to_string(id: &uuid::Uuid) -> Result<String, String> { ... }
```

- A table is named for a target language, so one file can serve more than one.
  Under it, `constructor` and `enum` each have a table for each adopted name.
  They are kept apart because the names of types and of constructors are
  separate namespaces (decision 4): `enum Uuid = Uuid(String);` has an enum
  `Uuid` and a constructor `Uuid`, and one key could not say which is meant. A constructor
  has one Rust type, and one Rust type can be adopted for several constructors.
- The name of the constructor or enum is its full name, which is absolute and means one
  thing wherever it is written (decision 42). It is not a bare name, which would
  depend on which project writes it.
- A constructor or enum that is not one of the project that was loaded is an error, so a
  misspelt name is never taken for an adoption. A table or a key that is not
  known is an error too, as in the settings file.
- The generator cannot see inside the functions, and Rust checks that they have
  the types that the generated code calls them with. What it cannot check is
  that they agree with each other, so that what is serialised is deserialised as
  the same value. That is for the author to get right, and the docs say so.
- The file is read only when `--adopt` names it, and a file that is not found
  or not valid is an error with exit code 2.

## What is written for Rust

The generated code is in three layers. The first is in the runtime crate, the
second is generated for each project, and the third is generated for each type.

1. **The syntax layer**, from text to a generic tree of values and back. It is
   the same for every type, and it builds data only. A constructor name in the
   text is a string at this point, and nothing is instantiated.
2. **The codec**, one for each project that the code is generated for, which
   reads the tree into the Rust types and writes them back. It holds the names
   and the consts of its project and the limits, and it resolves each name as
   it reaches it (see below).
3. **The types**, which are plain data and know nothing of any project. The
   conversions are the codec's.

The codec reads and writes with **traits that it implements, one for each
type**, and so `self` is the codec wherever a name or a const is looked up:

```rust
pub trait ReadValue<T> { fn read_value(&self, value: &Value) -> Result<T, Error>; }
pub trait WriteValue<T> { fn write_value(&self, value: &T) -> Result<Value, Error>; }
pub trait DefaultValue<T> { fn default_value(&self) -> Result<T, String>; }

impl ReadValue<Shape> for Codec { ... }
impl<T> ReadValue<Styled<T>> for Codec where Codec: ReadValue<T> { ... }
```

`read_value` for an enum is a `match` over the fixed list of its constructors,
by their full names, so a name that is not in the list is an error. That is the
closed set above, and it holds by construction: nothing can be invoked that the
`match` does not name. It is also where the two functions of an adopted
constructor are called, and where an omitted argument is given its default. The
program calls `codec.read::<Drawing>(text)`, which needs `Codec:
ReadValue<Drawing>`. In Python, which has no traits, they are the codec's
methods ([scenario for Python](scenario-shapes-python.md)).

**Names and consts are resolved as the value is read,** and not in a pass of
their own. At a constructor, the reader asks the codec for its full name and
matches that. At a const, it asks for the const's value and reads it with the
same reader, against the type that it already expects there. Writing is the same
the other way: each name is written in the form that the project writes as the
tree is built. So an error is reported where the reader is, with the names of
the parameters in its path, which a pass cannot know, and no copy of the tree is
made.

**The key between the codec and the impls is the full name** of a constructor,
such as `com.example.shapes::Circle`. It is unique in a run, it is the language's
own absolute name (decision 42), and it is what a codec writes when its names
are set to `full`, so the writer needs no step for that setting. The checker has a form of its own for the
names that it resolves, which is internal, and the generated code does not use
it. Any key that is unique in the run would do, since the generator writes both
sides; the full name is chosen because it is already specified. A generated
`enum` of the constructors, in place of strings, would make a misspelt key an
error of the compiler, but the codec would then need a tree of its own in place
of the parser's, and it is left as a later change.

The program calls the codec: `service_a::codec().read::<Shape>(text)` and
`codec.write(&shape)`. There are no `parse` and `to_text` on a type, since the
names in the text depend on a project and the type does not have one.

**Writing can fail, and returns a `Result`,** as reading does. A value that the
program holds can have no Constructo form: a `Float` that is not finite has no
text, and an adopted type can have a value that the declarations have no
constructor for, which its function refuses. The generated code never panics on
a value that the program is allowed to hold, and it never writes something in
place of what it cannot write.

An error of writing has no position in a text, so it has **a path in the value**
instead, by field name and by index, followed by the message, as in
`shapes[0].colour: The colour #336699 has no Constructo form`. The path is built
only when there is an error, as the error passes out of each field and element.

### The codec of a project

An application can talk to more than one service, each with its own Constructo
project, and so it needs more than one codec. One run of `constructo codegen
--project=FOLDER` writes the code for one project, and the codec is the part
that is specific to it.

- **What it holds:** the table of names (every constructor of its run, with the
  form that the project writes for it), the table of consts (below), and the
  limits of the parser and of the reader.
- **It is bounded by its run.** A values file can name any project of its run
  (decision 42), and the run is the projects that this codec was generated for.
  Text that is read by the codec of service A cannot name a constructor that
  only service B's project has, because A's table does not have it, and it is
  an error at the name.
- **Reading** is text, then the tree, then the codec's readers, which resolve
  each name and const as they reach it. **Writing** is the codec's writers,
  which write each name as the project writes it, then text.
- **One Rust module for each Constructo project** of the run, so that two
  services that each have a `Shape` do not clash. The root project is the
  generated file itself, and the codec is in it. The rest are in submodules
  (see the next section).
- **A project that both services use** is generated once for each run, so two
  codecs that share `ids::Uuid` get two Rust types for it, and a value of one
  cannot be passed to the other. This is accepted for the first version, and the
  documentation says so. An option such as `--extern=PROJECT=RUST_PATH` could
  later point a run at the module that another run made, and it is not part of
  this task.
- **Adoption is for each run,** by the file that `--adopt` names. Two runs that
  adopt the same constructor differently get two types, which is for the user to
  keep in step.
- **A record type without a name** (below) is in the module of the project that
  uses it, so the same record type in two projects is two `struct`s.
- **In Rust, the traits are the codec's own.** `ReadValue`, `WriteValue` and
  `DefaultValue` are generated with the codec, in the program's crate, and the
  codec implements them for its types and for `u64`, `i64`, `f64`, `String`,
  `bool`, `Vec<T>` and `Option<T>`. A trait of the program's crate can be
  implemented for a type of any crate, so `impl ReadValue<uuid::Uuid> for Codec`
  is no trouble, which it would be if the traits were in the runtime crate
  (Rust's orphan rule). Each codec has its own traits, so two codecs can adopt
  the same type differently and their impls do not collide. It costs a few dozen
  lines that are the same in each generated file.
- **The settings of a codec are its own, and can be changed** (below).
- **The runtime crate is one.** Every generated file depends on the same version
  of `constructo-runtime`, since they share its syntax layer, and the generated
  code says the version that it needs. The runtime crate has no traits.

### Settings: of the generated code, and of a codec

A setting either changes the code that is generated, or only how a codec behaves,
and that decides where it is given.

- **A generation setting changes the generated code,** and so it is given to the
  command, for one target. TypeScript's `Nat` as `number` or as `bigint` is one:
  it is the type of a field, which the program's compiler checks before the
  program runs, and a codec that made a `bigint` for a field declared `number`
  would break the program while it type-checks. The extension in TypeScript's
  imports is another, since it is text in the generated code.
- **A codec setting changes only what a codec does,** and is a property of the
  codec, which a program can change: the limits, such as the depth of nesting and
  of consts, and `names`, which is `project`, to write each name as the project
  writes it, or `full`. A service that reads from a peer that it does not trust
  can tighten the limits, and a program can write its own files with the
  project's names and the text for another party in full, from one generated
  package.
- **A codec is cheap, and is never shared by default.** `codec()` makes a new
  one each time, and the generated code keeps none of its own for callers to
  share, so a program that changes a setting changes it only for itself. The
  tables of names and consts are made when the code is generated and are shared
  by every codec; a codec holds only its settings and the consts that it has
  parsed. In Rust a setting is a field that is changed through `&mut self`, so
  the borrow checker stops a change behind another user's back.
- **Without a project, `names` can only be `full`,** since there is nothing for a
  name to be relative to.

### Modules and names in Rust

The project with `--project` is the **root**, and the generated file is its
module. The user mounts it with a name of their own, `mod service_a;`, so the
Constructo name of the root never shows, and two services that each have a
`Shape` are `service_a::Shape` and `service_b::Shape`.

- **Every other project of the run is a submodule,** except the prelude, whose
  `Bool` and `Option` are Rust's own. It is named by its parts joined with `_`,
  so `com.example.base` is `com_example_base`. The parts keep their case, with
  `#[allow(non_snake_case)]`, since the names of projects are case sensitive and
  lower-casing could join two of them. The generated code reaches one project
  from another with paths that start with `super::`, so the file can be mounted
  anywhere.
- **The names are flat and not nested** (`com::example::base`). In Rust a module
  and a type are in one namespace, so a nested module `b` would clash with a
  record `struct b` of the same module, and a project `a.b` next to `a.b.c`
  would make `b` both a project and a path. Flat paths are also shorter.
- **A clash is settled by number.** Two projects can have the same joined name,
  as `a_b.c` and `a.b_c` do, and a submodule can have the name of a record
  type of the root (above). The first, in the order of the files and of the
  declarations, keeps the name and the others are `_2`, `_3` and so on. It is
  rare, and the Constructo names in the text do not change.
- **Files without a project** (`constructo codegen --lang=rust shapes.types`)
  have no name to show, and all of their types are in the file's module.
- **Keywords.** A joined name has a `_` between its parts, so it is never a
  keyword. A field, a parameter or a type that is one is written with a raw
  identifier, `r#type`. A few cannot be raw, `self`, `super`, `crate` and
  `Self`, and nor can `_`, which `check` accepts as a field name, and these get a
  `_` at the end, repeated if that is the name of another field of the same
  `struct`. The name in the text is the Constructo name, held in the reader and
  the writer as a string, so it does not change.
- **An enum with one constructor is one `struct`,** named after the enum, as
  `enum Point = Point(x: Float, y: Float);` is `struct Point { x: f64, y: f64 }`.
  A Rust enum and a `struct` of the same name would clash, and this is the usual
  way to write a type with one constructor. It is so whatever the constructor is
  called: `enum Temperature = Celsius(Float);` is `struct Temperature(f64)`, and
  the codec still reads and writes `Celsius(21.0)`. The enum and its
  constructor are then one Rust type, so adopting either is the same adoption.
  Adding a constructor later makes the `struct` an `enum`, which breaks the
  program's code as adding a constructor to an `enum` already does.
- **A constructor that is called like another enum** gives the enum's name to the
  enum, since that is the name that fields refer to. The `struct` of the
  constructor takes the name of its own enum in front: with `enum Circle = ...;`
  the constructor `Circle` of `Shape` is `ShapeCircle`. It is done only where
  there is a clash, and it does not depend on order. A number is the last
  resort, where `ShapeCircle` is taken too.
- **The names that the generator adds are fixed:** `Codec`, `codec`,
  `ReadValue`, `WriteValue` and `DefaultValue`. They are what the program calls, so
  they do not move, and a Constructo name that is one of them yields, with a `_`
  at the end: an enum `Codec` is `Codec_`. The generated code has no other names
  of its own, such as aliases, and writes everything else in full.
- **Every renaming is reported** by the generator as a warning, which says the
  Constructo name and the Rust name, so that the author of the program is not
  surprised by `ShapeCircle` or `Codec_`.
- **The std items are written in full,** as `::std::vec::Vec`,
  `::std::boxed::Box` and `::std::result::Result`, in the generated code. A type
  of the user that is called `Vec` or `Result` would otherwise hide the item that
  the code needs.

The test for this step compiles the output for names that are keywords, for the
field `_`, for a type that is called like a std item, for an enum called like a
generated name, and for the clashes above.
I ran `check` on `enum A = A { _: Nat };` and on a record of the fields `_x`,
`type` and `self`, and it accepts them all.

### The runtime crate

For Rust, the syntax layer is a **small runtime crate**, named `constructo-runtime` (in
code, `constructo_runtime`), that the generated code depends on, and that this
repository provides. The name is that of its job, which is to be what generated
code calls, and it stays right if the types files part of the parser is cut
later. The convention for a crate of a project is `<project>-<role>`, in
kebab-case. It is the code that `check` already uses,
so the generated Rust code and `check` agree on the syntax by construction, and
there is not a copy of the parser in every generated file. Other targets have a
runtime of their own (below). The generated
files stay small.

The modules that it would hold are `lexer`, `parser`, `ast`, `values_text` (the
printer), `limits` and `diagnostics`, about 2,600 lines with their tests. They
use nothing outside themselves and no external crate: the only `use crate::`
lines of the six files are among the six. The rest of `constructo` uses them
from 26 files (counted with a search for their paths, so it is approximate), so
the main crate re-exports them under their present paths and those files do not
change.

- It would be a member of a workspace of this repository, and the main crate
  would depend on it and stay where it is.
- It has to carry the limits of the parser (nesting and size), since it reads
  data that is not trusted.
- It also parses types files, because the parser is one module. It is simplest to
  leave that in, and it could be cut later.

*Decided since:* it is published with `constructo`, at the same version, which
`cargo publish` requires once `constructo` depends on it; and the types files
part of the parser stays in it for now. See the
[implementation plan](implementation-plan.md), steps 2 and 8.

### The runtime of another language

The runtime crate is the parser of `check` only for Rust. Another target needs a
runtime in its own language, and the [scenario for Python](scenario-shapes-python.md)
found that the draft had taken one parser for every target for granted.

- **Each target has a runtime of its own,** written in its language: the syntax
  layer of values only (no types files, projects or type checking), which is a
  lexer, a parser, a printer and the limits, and the helpers that the generated
  code calls. It is small, and it installs wherever the language does, with no
  native build.
- **A conformance suite holds them to one syntax.** For each input, from
  `syntax/examples` and from cases written for it, the suite says what a runtime
  must give: the canonical text that the Rust printer writes for it, or the
  message and the position of the error. A runtime conforms when it gives them
  exactly. The expected results are Constructo text, so the suite needs no other
  notation. In place of one parser to audit there is one suite, which makes the
  claim that every target reads the same syntax one that can be checked.
- **The limits are part of it.** A runtime reads data that is not trusted, so the
  suite has deep nesting and large inputs, and every runtime refuses them with
  the same message.
- **A change to the syntax is a change to every runtime,** and the suite is what
  finds one that has not been changed.
- **A native runtime can be added later,** as Python's `json` has a module in C
  beside the one in Python: a binding to the Rust runtime, for speed. The
  generated code calls only the runtime's functions, so it would be a change of
  packaging and not of the generated code. It is not part of this task.
- **Where the reading of a type lives is the target's choice.** What is common
  is that every reader and writer works through the codec: it asks the codec for
  the full name of a constructor, for the value of a const and for its limits,
  and it is given the codec to do so. Where the code is put is left to the
  target. Rust generates its traits with each codec, because of the orphan rule
  (above). Python makes them the codec's methods. TypeScript, which has neither
  traits nor that rule, keeps `Type<T>` and the basic types in its runtime,
  shared by every codec, and gives the codec to them as a parameter ([scenario
  for TypeScript](scenario-shapes-typescript.md)).
- **How the names that the generator adds avoid Constructo names is the target's
  choice.** What is common is that they never clash, that those the program calls
  are fixed and do not move, and that every renaming of a Constructo name is
  reported. Rust and Python have no character that a Constructo name lacks, so a
  Constructo name yields, with a `_` at the end. A Constructo name cannot have a
  `$`, so TypeScript puts one in every name that it adds, such as `$kind` and
  `Colour$Constructors`, and nothing yields.

### The tree of a value

A program should be able to read Constructo into a neutral tree, and write a tree
as Constructo, without any generated code and without types, as a program can
read JSON into dictionaries and lists. The syntax layer has the tree already, so
this makes it public, in every runtime.

- **`parse(text)` gives the values of a text as trees, and `render(tree)` gives
  the text of a tree.** The tree is the syntax and nothing more: a number keeps
  the text it was written with, so it is exact, a name is as it was written,
  bare, through a group or in full, a const is a reference to it, a tuple keeps
  which arguments were given by name, and a record keeps the order of its fields.
  Each value has its position in the text, for messages.
- **Nothing is built from the data,** since the tree is data only, so it is safe
  for data that is not trusted, as the codec is.
- **Rendering can fail.** A tree that a program builds can be one that has no
  text, such as a name that is not an identifier, or a const with a body, and
  `render` refuses it. Whatever `render` writes, `parse` reads back as the same
  tree.
- **The suite covers it,** since the canonical text that it expects is what
  `render` writes for what `parse` reads.
- **The tree is an interface,** in Rust as in every other target, so its shape is
  a promise once it is published. For Rust it is the `ast` module of the parser,
  which was written for `check`, and it is to be looked at for public use
  before the runtime crate is published.

**Open:** a tree with the names resolved and the consts replaced, but no types.
The codec could give it, with `codec.read_tree(text)`, since it has the names and
the consts of its project: every name in full, and a name that is not of the run
refused. It is between the neutral tree and a value of a generated type, and
whether it is wanted is for the first program that asks for it.

### The names in the text

A constructor can be written bare, through a group, or with the full name of its
project. A full name is valid in any project (decision 42), and it is long: the
prelude's `Some(1)` is `constructo.prelude::Some(1)`, and the prefix is on every
constructor, so a large document is several times its size. The writer therefore
writes each name in the shortest form that the project can resolve.

- **Project-relative by default.** When the code is generated for a project
  (`--project`), the generator knows which names that project can write bare,
  and it builds into the codec, for each constructor, the form that the project
  would use: **bare, else through a group, else in full.** A name in scope is
  written bare, as in `Some(1)`. A name that the project can reach through a
  group is written through it, as in `ids::Uuid`; the projects of one group share
  no names (decision 32), so it is never ambiguous. Any other is written with the
  full name of its project. What is written is accepted by `check` in that
  project, so the test oracle holds. It is only guaranteed for
  that project, and another that imports other things may not resolve it.
- **Renames and hides count.** A name that an import renames is written with
  its new name, bare or through the group, and a name that an import hides is
  not visible through that import's group, so that group is not a candidate.
  The generator resolves names as `check` does, so it gets these right by
  construction, and each has a test.
- **The choice is fixed.** Where a project is in more than one group, the
  generator takes the first import in the project-file through which the name
  is visible, so that the same project always gives the same text.
- **`convert` writes names in the same way.** It writes bare or in full today,
  and the task [convert-writes-names-through-a-group][group-task] adds the
  group, so that the two keep writing the same text for the same value, as the
  [scenario](scenario-shapes.md) found that they do.

- **Full names on request.** A codec whose `names` setting is `full` writes the
  full name of every constructor, which is valid in any project. It is for text
  that goes to a consumer whose imports are not known. It is a setting of the
  codec and not an option of the command, so that one program can write both
  (see the settings of a codec, below).
- **Without a project the names are full,** since bare names have nothing to be
  relative to.
- **The codec accepts every form on reading:** the full name, and the name that
  the project resolves. It turns each into the full name before the type layer
  sees it. The type that is asked for already decides which constructors
  can be invoked, so the qualification tells it nothing. The closed set and the
  names resolved at generation time are what keep it in agreement with
  `check`.

Compressing the text is another way to make it smaller. It is not part of this
and is left to the program (see below).

[group-task]: ../todo/2026-11-21-convert-writes-names-through-a-group/convert-writes-names-through-a-group.md

### Generic enums

An `enum` may have type parameters, as in
`enum List<T> = Nil, Cons { head: T, tail: List<T> };`. A parameter has to be
used by some constructor, unless its name starts with an underscore (decision
3), but a constructor does not have to use every parameter of its enum: `Nil`
uses none.

**A constructor's type has exactly the parameters that occur in its own
arguments, in the order of the enum.** With
`enum Either<L, R> = Left(L), Right(R);`, if `Left` had both, it would have a
parameter that it never uses, which Rust refuses (E0392) unless it holds a
`PhantomData`, and anyone who adopted `Left` would have to carry an `R` that
they do not need. `Nil` has no body, so it is a variant with no data and has no
type to give parameters to.

```rust
struct Left<L>(L);
struct Right<R>(R);
enum Either<L, R> { Left(Left<L>), Right(Right<R>) }

struct Cons<T> { head: T, tail: Box<List<T>> }
enum List<T> { Nil, Cons(Cons<T>) }
```

- **Reading.** The generated impls have bounds,
  `impl<T> ReadValue<List<T>> for Codec where Codec: ReadValue<T>`: the codec
  can read a `List<T>` if it can read a `T`. The caller names
  the instance, as in `codec.read::<List<u64>>(text)`, so reading is still
  constrained by the target type, and no type is chosen by the data.
- **Adoption.** The Rust type that is adopted for a constructor has the type
  parameters that the constructor uses, in the same order, and the two functions
  are generic over them. The generator cannot see the generics of the Rust type,
  so a mismatch is an error from Rust in the program that uses it.
- **Generic consts** are with the other consts, below.

**Open:**

- **A parameter that no constructor uses.** `enum Phantom<_T> = Phantom;` has no
  constructor that uses `_T`, so the Rust type, which is `struct Phantom<_T>` as
  an enum with one constructor, would have a parameter that is not used, and it
  needs a hidden `PhantomData`. An enum of several constructors with such a
  parameter needs one too. It is the one case where the generated type has
  something that the declaration does not.

Recursion, which needs the `Box` of `Cons`, is below.

The mapping of the types is as in the table, provisional because the language
may grow and the grammar has not been checked against each row.

| Constructo                                  | Rust                                                              |
|---------------------------------------------|-------------------------------------------------------------------|
| `enum`                                      | an `enum` with a variant for each constructor, holding its type |
| an `enum` with one constructor              | one `struct`, named after the enum (below)                        |
| a constructor with no body                  | a variant with no data, as `Shape::Dot`                           |
| a constructor with a record                 | a `struct` with named fields                                      |
| a constructor with a tuple and no names     | a tuple `struct`                                                  |
| a constructor with a tuple and names        | a `struct` with named fields (below)                              |
| a constructor with an array                 | a tuple `struct` that holds a `Vec`                               |
| a generic `enum`                            | an `enum` with the same parameters (above)                        |
| `[T]`                                       | `Vec<T>`                                                          |
| `String`                                    | `String`                                                          |
| `Nat`                                       | `u64`                                                             |
| `Int`                                       | `i64`                                                             |
| `Float`                                     | `f64`                                                             |
| `Bool` and `Option<T>` of the prelude       | `bool` and `Option<T>`                                            |
| a named parameter with a default (decision 18) | a field that the reader fills with the default if it is omitted |

**The parameters of a tuple.** Unnamed parameters come first and named ones
after them (decision 17), so a tuple has one of three shapes:

- **No names,** `C(Nat, String)`: a tuple `struct`, `struct C(u64, String)`. One
  parameter is a newtype, so `Uuid(String)` is `struct Uuid(String)`.
- **All named,** `Point(x: Float, y: Float)`: a `struct` with those names,
  `struct Point { x: f64, y: f64 }`.
- **Some of each,** `B(Nat, y: Nat)`: a `struct` with named fields, where an
  unnamed parameter is named by its position, `_0`, `_1` and so on, as Rust
  names the fields of a tuple `.0` and `.1`: `struct B { _0: u64, y: u64 }`. An
  unnamed parameter is always before the named ones, so its position is its
  index. A parameter that is called `_0` itself clashes only in the same
  constructor, and the rule for a field that is a keyword gives it a `_` at the
  end.

A name is what a program wants to read, `p.x` and not `p.0`, and it is what the
writer writes, since an argument that has a name is written by name, as
`convert` writes it, whether or not it was given by name.

**Open:**

- **Field order.** Fields are written in the order of the declaration, and the
  generated structs are to keep it.
- **Limits.** The parser of `check` has a nesting limit (decision 43) and the
  encoder of `convert` has a budget of nodes. The codec has to have
  limits as well, since it is meant for data that is not
  trusted, and the docs say what they are.
- **Other crates.** Whether the generated types also implement the traits of
  another crate, such as `serde`, so that other notations can use them. It is not
  needed for the goal, and it is not in this task unless it is wanted.

### Recursion

A Rust type that holds itself by value has no size, so
`struct Cons<T> { head: T, tail: List<T> }` does not compile: some field on every
such cycle has to hold its value through a pointer, `Box<List<T>>`. Python has
nothing like it, since its values are all references, so the model only says
which fields hold their type by value, and each target decides what that means.

**Which fields are boxed.** The generator makes a graph of the Rust types that it
generates, the enums, the `struct`s of constructors and those of records, with an
edge from A to B where A holds B by value:

- directly, or in an `Option`, or in a field of a tuple;
- through a type argument, where the declaration holds that parameter by value:
  `Styled<X>` holds an `X`, since its `item: T` is by value, and a type that
  holds its `T` only in a `Vec` does not;
- and not in a `Vec`, which is a pointer already, nor in an adopted type, which
  the generator cannot see into, and whose author boxes what it needs.

A cycle in the graph is a type of infinite size. **Every field whose edge is on a
cycle is boxed, and nothing else is.** `next: Node?` on a cycle is
`Option<Box<Node>>`.

- **Fields only, and never a variant.** `Shape::Circle(Circle)` is never boxed.
  A `struct`'s only edges are its fields, so every cycle has a field on it, and
  boxing the fields is enough. A `match` on the variants stays as it would be
  written by hand.
- **Every site of a mutual recursion.** One box is enough to make a cycle
  finite, but choosing one needs a rule such as "the first declared", and then
  moving a declaration would move the box, and change the types that the
  program's code is written against, for a change that means nothing in
  Constructo. Boxing every field on the cycle makes the type of a field depend
  only on whether it is on a cycle, and it costs an allocation more where there
  could have been one fewer. It also needs no search for the fewest boxes, which
  is a hard problem in general.
- **The text does not change.** The codec reads and writes through
  `impl<T> ReadValue<Box<T>> for Codec where Codec: ReadValue<T>`, so a `Box`
  is not seen in the text.

Two examples. `syntax/examples/types/valid/mutual.types` needs no box:

```
enum Tree<T> = Leaf(T), Node(Forest<T>);
enum Forest<T> = Trees([Tree<T>]);
```

`Tree`'s `Node` holds a `Forest<T>` by value, but `Forest<T>`, as one
`struct Forest<T>(Vec<Tree<T>>)`, holds the `Tree<T>` only in a `Vec`, so there is
no cycle. This pair needs a box at both sites:

```
enum Expr = Num(Nat), Neg(Term);
enum Term = Lit(Nat), Paren(Expr);
```

```rust
pub enum Expr { Num(Num), Neg(Neg) }
pub struct Neg(pub Box<Term>);      // a Term can hold an Expr by value

pub enum Term { Lit(Lit), Paren(Paren) }
pub struct Paren(pub Box<Expr>);    // an Expr can hold a Term by value
```

**To test, and not to take for granted:**

- **Recursion that changes its argument.** `enum Nested<T> = Flat(T),
  Deep(Nested<[T]>);` is accepted by `check`. The `Box` makes the type finite,
  but the bound `Codec: ReadValue<Nested<Vec<T>>>`, inside the impl of
  `ReadValue<Nested<T>>`, may be more than Rust's trait solver can follow. If it
  is, such a type needs a reader that is written out and not bound. The test of
  step 4, which compiles the output, has one.
- **Deep values.** The parser's limit on nesting (decision 43) bounds how deep
  the reader goes. A program can build a value that is deeper, such as a long
  chain of `Cons`, and the writer, which is recursive, applies the same limit and
  returns an error, and does not run out of stack. The `Drop` of Rust is
  recursive too, which is the program's to manage, and the documentation says
  so.

### Numbers

Constructo puts no limit on the size of a `Nat` or an `Int`: the digits are never
parsed, so a number of any length is valid. Rust has to choose a width, and the
choice is made for Rust, as it is not the job of Constructo to fix the limits of
a language. Another language gets its own choice. Python, for one, has integers
of any size and would use them.

- **The mapping.** `Nat` is `u64`, `Int` is `i64` and `Float` is `f64`. There is
  no option to change this in the first version.
- **Writing is exact.** Every `u64` and `i64` fits, so what a program writes is
  read back unchanged.
- **Reading fails when the number does not fit.** The error is at the number, in
  the form that `convert --type` already uses: `The number 18446744073709551616
  does not fit Nat`. A number is never wrapped, clamped or rounded.
- **A narrower type is for the program.** A field that should be a `u8` or a
  `u32` is a Rust type that wraps the number, or an adopted type. The generator
  does not guess from the data.
- **Big integers are a later option,** such as `--nat=TYPE`. This choice does not
  prevent it.

**NaN and the infinities.** Constructo has no text for them yet, so writing one
is an error, as writing can be (below), and a `Float` that is too large for an
`f64`, such as `1.0e400`, is refused when it is read, as a whole number that
does not fit is. The task
[non-finite-values](../todo/2026-11-21-non-finite-values/non-finite-values.md)
gives them a text (`0n1.0`, `-0n1.0` and `0n0.0`), and when it is done the
`f64` of Rust maps to it in both directions.

### Record types without a name

`{ x: Nat, y: Nat }`, written inside a type, is a record type of its own. It is
closed and structural (`docs/specs/type-checking.md`): it is the set of its
fields and their types, in any order, so two of them anywhere are the one type.
A tuple would lose the names of the fields and a map the types, so Rust gets a
generated `struct`, and it needs a name.

- **One `struct` for each distinct type,** as the language has it. A value of
  one place can be passed to another that has the same type.
- **The name is the field names, sorted and joined with `_`.** A record
  of `mem` and `cpu`, both `Nat`, is `cpu_mem`. It does not depend on the order of the fields or on where
  the type is used, so moving a declaration does not rename it.
- **The name is lower-case, as the fields are.** A constructor or an enum starts
  with a capital letter, so a name made of fields can never be the same as a
  generated one. The generated code carries `#[allow(non_camel_case_types)]`
  for these.
- **The same names with different types** are different types. `{ x: Nat }` and
  `{ x: String }` are both `x`, so the first to be found, in the order of the
  files and of the declarations, keeps `x` and the others are `x_2`, `x_3` and
  so on. The join can also be the same for different sets of names: a record with
  the one field `cpu_mem` and one with `cpu` and `mem` are both `cpu_mem`, and
  the same rule separates them.
  These are the only names that depend on order, and only when there is a clash.
- **No fields:** `{}` is named `empty`.
- **The fields of the `struct` are in the sorted order,** which is also the order
  that the writer writes them in. The reader accepts any order, as `check` does.
- **Parameters follow the rule above:** the `struct` has exactly the type
  parameters that occur in its fields, in the enum's order.

**Open:** a record with many fields has a long name, and where it is too long
the name needs a limit, such as a short hash of the rest. Whether the names
should be written in the camel case of Rust, at the cost of the guarantee that
they cannot clash with a declared name, is also open.

### Consts

A values file can name any const of the types of the run, and the prelude's
`null`, `true` and `false` are consts (decision 14), so a document such as
`{ active: true, parent: null }` is accepted by `check`. A reader that did not
resolve consts would refuse it. A const is a name for plain data, which is
checked against its type, and a generic one such as `null` is good at any type.

- **The codec resolves references from a closed table.** The generator embeds
  the full name of each const of the run and its value, written with full names.
  When the reader meets a const name, it reads the const's value into the type
  that it expects there, as for any other value. The codec can parse each value
  once and keep it. The value is untyped until it is read, so a generic const and a
  widening (a `Nat` const where an `Int` is wanted) need nothing extra. The table
  is fixed when the code is generated, so the data can name only the consts that
  the generator knew for that project, as it can invoke only the constructors
  that it knew.
- **A const that refers to another** is resolved in the same way, and the
  runtime's depth limit stops a chain that is too long. A circle is refused by
  `check`, so it does not arise.
- **An error inside a const's value is reported at the reference,** with a note
  that names the const: `5:12, in the value of const unit_circle: ...`. It says
  both where the const was used and what is wrong in it. The values are checked
  by `check`, so it is rare, and is mostly a value that does not fit the
  target, such as a `Nat` too large for a `u64`.
- **Names are resolved like those of constructors:** the full name, or the name
  that the project resolves.
- **A value that does not fit Rust is refused when the code is generated.**
  `const big: Nat = 99999999999999999999999;` does not fit a `u64`, and it is not
  left to fail on the first read.
- **The writer never writes a reference to a const.** It writes the value. What
  it writes for a `Bool` and for `None` is what `convert` writes, which the spec
  says is always bare.
- **No Rust items are generated for consts** in the first version. A program has
  its own constants, and a generated function for each would bring back the
  difficulties of width, `Box` and generics for little use. The documentation
  says so. A function for each const that returns a `Result` can be added
  later without changing the rest.

### Defaults

A named parameter of a tuple constructor may be left out, and the argument then
takes the default of the type of the parameter (decision 18). The default
depends on the type alone: `0` for a `Nat` and an `Int`, `0.0` for a `Float`, `""`
for a `String`, `[]` for an array, the record of the defaults of its fields for a
record, and for an enum its first constructor with the defaults of its own
parameters. A record field is never optional, and nor is the parameter of a
constructor that has no names. `check` accepts a document that leaves an
argument out, so the reader has to fill it in.

- **A trait of the codec gives the default,** `DefaultValue<T>`, beside
  `ReadValue<T>` and `WriteValue<T>`. It is not Rust's `Default`, whose rules differ: it follows the
  rules above and ignores a derived or hand-written `Default` of a type. The
  reader calls it for each argument that is left out.
- **The generated impls follow the rules.** A number is `0` or `0.0`, a `String`
  is empty, a `Vec` is empty, a record `struct` takes the default of each field,
  and an `enum` takes its first constructor with the default of its type. The
  generated `enum` keeps the order of the constructors, because that order is
  what decides the default. `Bool` gives `false` and `Option<T>` gives `None`,
  as the prelude orders them.
- **A type with no usable default is an error where it is left out,** and not
  where it is declared, as in `check`. The generator works out, for each type,
  whether it has a default, and gives it the same limits (`--default-limit`).
  The default of a type is then a `Result`: a type that has none returns an
  error with the message that `check` gives, such as `Type List<T> has no default
  value`. A type that is never left out is not affected.
- **A generic type uses the default of its parameter,** so the impl has the
  bound `Codec: DefaultValue<T>`. The caller names the instance, so the type is known
  where `check` would have to work it out from the other values.
- **An adopted constructor gets its default through its function.** The default
  arguments are built as for any other constructor and are given to the
  deserialising function, which may refuse them. `Uuid(String)` has the
  argument `""`, which that function probably refuses, and the refusal is an
  error where the argument is left out.
- **The writer writes every argument,** including those that are equal to the
  default, so that the text does not depend on what the defaults are. Leaving
  them out would make the text shorter, and may be an option later.

**Defaults when a program builds a value.** A target whose language has default
arguments can give a named parameter its default in the generated class as well,
so that a program can leave it out as the text can. Python does: the binding of
decision 17, unnamed parameters by position and then named ones by position or
by name, is Python's binding for the fields of a class that have defaults and
those that do not. So `Point(x=4.0)` is what `Point(x: 4.0)` reads as.

- **Only a named parameter has a default.** An unnamed parameter and the field of
  a record are required, as they are in the text.
- **The default is the codec's.** The class's default is the same generated
  function that the codec calls when an argument is left out of the text, so
  the two cannot differ: there is only one of them.
- **No `None` stands in for a default.** A field's default is `None` only when its
  type is an `Option`, which is when decision 18 says that the default is
  `None`. A `Float` never has `None` for its default, and a type checker refuses
  a factory that gives one, which the spike's run of mypy shows.
- **A default that fails, fails where it is left out,** as in the text: the
  adopted `Uuid` has no default, so `Drawing(title="T")` raises the adopted
  function's refusal, as `Drawing(title: "T")` is refused when it is read.
- **A parameter of the type `T` has no default,** since the type is not known at
  run time. One whose default does not depend on `T`, such as `[T]` or `T?`,
  has one.
- **A test holds them together:** for each type, a value built with arguments
  left out equals the value that the codec reads from the text with the same
  arguments left out.

Rust has no default arguments. `impl Default`, with `..Default::default()`, is
the nearest, but it cannot fail and it covers every field, so the first version
has none. An `impl Default` for a type whose defaults all succeed is a possible
addition later.

## The options

**Open.** The likely shape is below, and is not final.

```
constructo codegen --lang=LANG [--project=FOLDER] [--adopt=FILE_PATH]
                   [--out-file=FILE_PATH | --out-dir=FOLDER] [FILES]...
```

- `--lang=LANG` is required, and the only value at first is `rust`. It is
  matched exactly, as the formats of `convert` are, and an unknown value is an
  error that lists the ones that exist.
- `--adopt=FILE_PATH` names the file of adoptions, if there are any. Its format
  is open (above).
- `--project=FOLDER` is the project that the codec is for. An application with
  two services runs the command twice, with each project and its own output
  file, and calls the codec of each.
- A target's generation settings (below) are given to the command. How they are
  given, an option for each or a table for the target in a file, is open.
- The types come as they do for the typed formats of `convert`: from `--project`
  and from the files given, with the options of `check` for projects, the
  prelude and consts where they apply.
- `--out-file=FILE_PATH` and `--out-dir=FOLDER` say where the code is written,
  and a target takes one kind of output and not the other (below).
- Nothing is written if there is an error, as for a `convert` format that writes
  one document: every file is made in memory first.
- Exit codes are those of the other subcommands: 0 for success, 1 when the
  inputs have errors, 2 for a usage error, a setup problem or a file that cannot
  be read.

### A faithful mapping first, and an idiomatic one where it is safe

A value that a target's mapping is given ends in one of three ways: it is **held**
exactly, and comes back as the same value; it is **refused**, and the codec says at
the value that the target cannot hold it; or it is **changed**, and becomes another
value without a word. The rules:

- **A mapping never changes a value,** with one exception, `Float`, below.
- **A mapping may refuse a value that is outside the target's range for its
  type,** when it is read and when it is written, with an error at the value.
  A `Nat` that does not fit a Rust `u64` or a TypeScript `number`, and a `Float`
  that is not finite, are refused. The range is the target's, and is documented,
  and a generation setting may widen it, as `bigint` would for TypeScript.
- **A mapping may not refuse a form that the declarations contain.** A declared
  `Nat??` asks for `Some(None)` as a value, and refusing it would take away part
  of the type, and not limit its range. Where an idiomatic form cannot hold a
  form that the declarations have, the faithful form is used. This is why
  `Option<T>` as `T | None` everywhere would be a bug, while a `Nat` as a `u64`
  is not: the `u64` limits how large a `Nat` is, and takes no constructor away.

So a target's first mapping of a Constructo type is the one that is always exact.
Where the language has a more idiomatic form that loses nothing in a case that
can be told from the declarations, the generator uses it there, and only there.
An idiomatic form that is wrong in a rare case is a bug, however common the case
in which it is right.

Two consequences, which the targets so far have to meet:

- **A string that has no Constructo text is refused.** A JavaScript string can
  hold a lone surrogate, which has no form in UTF-8. A writer that put U+FFFD in
  its place would change it, so it refuses it. The TypeScript spike does not
  check this yet.
- **The functions of an adoption have to be inverses,** and the codec cannot see
  that they are. A `from` that makes a UUID lower case changes one that was
  written in upper case. It is the adopter's to get right, as the section on
  adoption says, and this is why it matters.

**`Float` is the one exception.** A `Float` is a real number, extended by +∞, −∞
and nullity, with Anderson's transreal arithmetic as their theory (the task
[non-finite-values](../todo/2026-11-21-non-finite-values/non-finite-values.md)
gives them a text). The notation writes only rationals, an integer over a power
of ten, so a value in a text is always one of those. Held exactly, it would need
a decimal or rational type in every target, which no one would want, since the
hardware is IEEE 754, and escaping it gains little and costs much. So:

- **A target maps a `Float` to its binary floating type,** holding the nearest
  value, and the loss of precision is accepted, as it is across the industry.
  This is the only exception to the rule that a mapping never changes a value,
  and no other type is to have one.
- **Only precision is excepted, and not range.** A finite `Float` that is too
  large for the target is refused: `1.0e400` is not made +∞, which would make a
  finite value infinite. A `Float` too small to be held other than as zero is
  made zero, which is a loss of precision like any other.
- **The writer writes the shortest text that reads back as the same binary
  value,** so a value that a program holds goes through text and back exactly,
  and a long literal in a file does not keep the digits that the binary type
  cannot hold.
- **A negative zero is written as `0.0`.** A `Float` has no distinct negative
  zero value, so there is nothing else for it to be, and the two are equal in
  IEEE as well.
- **The non-finite values map to IEEE's,** when they have a text: +∞ and −∞ to
  the infinities, and nullity to NaN, and every NaN is written as nullity.
  Until then, writing one is an error, as the section on numbers says.

The model of a `Float` is in [type checking](../../specs/type-checking.md), the
section "What a number is", and in decision 45, which step 1 wrote.

`Option` in Python is the case that showed it
([scenario for Python](scenario-shapes-python.md)):

- **The faithful mapping** of `Option<X>` is `Some[X] | None`, where `Some` is a
  box of one field in the runtime. `None`, `Some(None)` and `Some(Some(5))` are
  three values, so nothing is lost, but it is not what a Python programmer
  writes.
- **The idiomatic mapping** is `X | None`. It is exact when no value of `X` is
  `None` in Python, since a `None` can then only be the absent case.
- **The generator uses the idiomatic one where it can show that,** which it can
  from the declarations alone: `X` is a `Nat`, an `Int`, a `Float`, a `String`, a
  `Bool`, an array, a record without a name, a generated class or enum, or an
  adopted type. An adopted type qualifies because the codec refuses a `None` from
  an adopted function, as an error at the value, so no value of it is `None`.
- **Everywhere else it uses the faithful one:** where `X` is itself an `Option`,
  so `Nat??` is `Some[int | None] | None`, and where `X` is a type parameter, so
  `T?` is `Some[T] | None`, since the caller may choose a `T` that has `None` in
  it. `Option<Option<T>>` is how a partial update is written, with "leave it" and
  "set it to nothing" as two values, and it works in every target.

Rust needs none of this, since its `Option<Option<T>>` is exact. The model of
step 3 has to give a target what it needs to decide, which here is, for each
type, whether its values in the target can be the absent value.

### Where the code is written

A target writes either one file or a folder, according to its language, and the
option says which is meant, so that a path is never taken for the other kind.

- **Rust writes one file,** since its modules can be inside one file. It goes to
  `--out-file=FILE_PATH`, or to the standard output if neither option is given.
- **Python writes a package,** since a module of Python is a file, and a run
  with projects that are imported has more than one. It goes to
  `--out-dir=FOLDER`, which it needs. The folder is the package, and its name is
  the one that the program imports, as a Rust program names the module of the
  file. `__init__.py` has the types of the root project and the codec, and each
  other project is a module beside it, named as the Rust submodule is
  (`com_example_ids.py`). They import each other with relative imports, so the
  package can be put anywhere.
- **The wrong option is a usage error,** exit code 2, which names the right one:
  `Python writes a package: use --out-dir=FOLDER`, or `Rust writes one file:
  use --out-file=FILE_PATH, or neither for the standard output`. Both options at
  once is an error too.
- **The folder of `--out-dir` belongs to the generator.** A run has to remove
  what an earlier run wrote and no longer writes, such as the module of a project
  that is not imported any more, so it replaces what is in the folder as a whole.
  So that it never removes a file of the user's, it writes only to a folder that
  does not exist, is empty, or has the marker file that `codegen` writes in every
  folder that it makes. Any other folder is refused, with exit code 2, and
  nothing in it is changed.
- **The folder is replaced at once.** The package is written to a new folder
  beside it, which then takes its place, so that a failure part of the way
  through leaves the old package as it was.
- **A file of `--out-file`** is written in the same way, to a new file beside it
  that then takes its place.

## Steps

An outline, to be cut when the open questions are answered.

**Before any of them, the same thought experiment for Python.** The
[scenario for Rust](scenario-shapes.md) found five things that the design had
not said, most of them about how Constructo's names and types meet those of
Rust. Python has no `enum` with data, no static types to check the generated
code, integers of any size and one namespace for a module's names, so it will
find others, and some of them may change what is common to every target, such as
the model of step 3 and the codec. The scenario is to be done for Python, with
the same projects and data, before the implementation starts.

It is done: [the scenario for Python](scenario-shapes-python.md). Its section
"What changes the common design" lists five things to decide before step 2: the
runtime for a language other than Rust, output that is a folder, `Option` inside
`Option`, resolving names as the value is read, and defaults as those of a
class.

[The scenario for TypeScript](scenario-shapes-typescript.md) followed, for a
language whose numbers are all doubles and whose types are gone when the program
runs. Its section "What changes the common design" lists four more: a number
that does not fit is refused and never changed, settings of a target's own,
where the reading of a type lives, and how generated names avoid Constructo
names.

1. **What a number is, in the specifications.** The values of `Nat`, `Int` and
   `Float` are written nowhere, and every mapping of a number depends on them.
   A `Nat` and an `Int` are whole numbers of any size. A `Float` is a real
   number, extended by +∞, −∞ and nullity, with Anderson's transreal arithmetic
   as their theory: nullity is equal to itself, and there is no distinct
   negative zero value.
   The notation writes only the rationals among them, an integer over a power of
   ten, which is the number that the digits write. It goes in
   `docs/specs/type-checking.md`, beside the types, or in a specification of its
   own, with a decision for the model of `Float`, and it refers to the task for
   the text of the non-finite values. The mapping of a `Float` to a binary type,
   and the precision that it gives up, are `codegen`'s, and stay here. *Done:*
   in `docs/specs/type-checking.md` and decision 45.
2. **The runtime crate.** Move the six modules into `constructo-runtime`, a crate
   of its own in a workspace, re-export them from `constructo`, and check that everything that
   passed still passes. Nothing is generated yet. The conformance suite is
   written here too, from the output of this runtime, which is the first to
   pass it and the one that the others are held to.
3. **The model.** A description of the declarations that is the same for every
   target: the types after resolution, with full names, generics and
   defaults. The checker already holds most of it, so this step may be a view of
   what exists and not new code. Each target writes from it, so a second
   language needs only a writer.
4. **Rust, the types.** Enums, variants, generics, `Vec`, `Option` and `Bool`. A
   test that compiles the output.
5. **Rust, the codec, writing and reading.** The codec of a project, with its
   tables of names and consts, and the code that writes a value as Constructo
   syntax that `check` accepts, and reads a value that `check` accepts, for each
   example under `syntax/examples`. A test with two projects that have a type
   of the same name, read by two codecs in one program. Limits are decided
   here.
6. **Adoption.** A small program that compiles first: a codec whose traits are
   implemented for a type of another crate, inside a `Vec` and as the argument
   of a generic type, to show that the orphan rule allows it. Then the file and
   its entries for constructors and enums, the two functions, the check that a type
   that needs an adoption has one, and the refusal of an open type.
7. **Documentation.** A spec of the command in `docs/specs/`, a decision for the
   closed set and one for the adoption, and the wording of decision 39.

## Decisions to write

- That reading is constrained by the target type, and the constructors it can
  invoke are a closed set that is fixed when the code is generated.
- How a type is adopted, a constructor or a whole enum, and why it is not in the
  project-file.
- That every reader and writer works through the codec, and where their code
  lives is the target's choice; and that in Rust the traits are generated with
  each codec and not put in the runtime crate, because of the orphan rule, and
  the codec implements them, one for each type, so that it is `self` where names
  and consts are looked up.
- That how the names that the generator adds avoid Constructo names is the
  target's choice, provided that they never clash, the ones the program calls
  are fixed, and every renaming is reported: a `_` that yields in Rust and
  Python, a `$` in TypeScript.
- That names and consts are resolved as the value is read and written, and not
  in a pass of their own.
- That a generated class may give a named parameter its default, from the
  codec's own function, where the language has default arguments.
- The name `codegen`.
- That names are written relative to the project by default, and in full when a
  codec is set to write them so.
- That a setting which changes the generated code is given to the command, and
  one that changes only behaviour is a property of a codec, which is cheap and
  never shared by default.
- That the syntax layer is a small crate, `constructo-runtime`, which the generated
  code depends on, and what it holds.
- That every field on a cycle of types held by value is boxed, at every site,
  and no other is.
- That a mapping never changes a value, may refuse one outside the target's
  range for its type, and may not refuse a form that the declarations contain;
  and that `Float` is the one exception, mapped to the nearest binary value;
  and so that a target maps a type faithfully, and idiomatically only where the
  declarations show that it is exact, with `Option` in Python as the example.
- That a target writes one file or a folder, with `--out-file` and `--out-dir`,
  and that a folder belongs to the generator, guarded by a marker file.
- That every other target has a runtime of its own, held to the syntax by a
  conformance suite whose expected results are Constructo text, and that every
  runtime makes the tree of a value public.
- That name resolution, the consts and the limits are in a codec for each
  project and not in the types, so that an application can have more than one
  codec, and that the generated code has a Rust module for each project.
- How the modules and the names in Rust are chosen: the root is the file, the rest
  are flat, and a clash is numbered.
- How a record type without a name is named, the handling of consts, and the
  defaults of omitted arguments, once chosen.

## Not in scope

- A target other than Rust. The model of step 3 is meant to make the next one
  small, and it is not written here.
- JSON, XML or any other notation from the generated code. `convert` does that,
  and `codegen` is about Constructo's own syntax.
- Sharing a project between two codecs, which for now is generated once for
  each. An `--extern` option is a later step.
- Compression of the text. A standard compression format can be applied to what
  is written, by the program or on the way to storage, and it is not part of
  the notation.
- Anything that builds, runs or publishes the generated code. The command writes
  text.
