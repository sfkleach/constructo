# Plan: step 6 of codegen, adoption

This plans step 6 of the [implementation plan](implementation-plan.md): `--adopt=FILE_PATH`, with which a program has
its own Rust type hold the values of a Constructo constructor or enum, in place of the one that would be generated.
It follows the draft's sections "Adopting types that are not generated" and "The format of the file".

## The spike

The outline asks first for a spike that compiles: the codec's traits implemented for a type of another crate, inside a
`Vec` and as the argument of a generic type. It was made in `_build/adoption-spike/`, from the generated scenario,
edited by hand into what the generator would write. A crate of its own, `foreign`, stands in for `uuid`, since the
orphan rule treats any other crate alike and a functest cannot fetch from crates.io. It adopts three things:

- the constructor `com.example.ids::Uuid`, the one constructor of its enum, as `::foreign::Uuid`;
- the constructor `com.example.shapes::Point(x: Float, y: Float)`, of one constructor too, as `::foreign::Point`,
  whose functions take and give the two arguments as a tuple;
- the enum `com.example.shapes::Colour`, whole, as the program's `crate::paint::Colour`, which has a `Custom` colour
  that Constructo cannot write.

**It compiles, and all eight of its cases pass:** the scenario's drawing read and written back; a `Vec` and an
`Option` of `foreign::Uuid`; a `Styled<foreign::Uuid>` and a `Styled<Vec<foreign::Point>>`; a string that is not a
UUID refused at the value; the id left out, whose default the function refuses; and a custom colour, which is not
written. So the traits generated with each codec are what adoption needs, as decision 51 expected. Two things that it
found are below: the message of a default that the function refuses, and the path through `Some`.

## What the code has today

- **The generator,** `codegen::rust::generate(&Model, &Defaults)`: the types, with a `struct` for each constructor
  with a body and one `struct` for an enum of one constructor, and the codec, with three impls for each enum and
  record, in the module of its project.
- **The model** works out `boxed` and `has_default` from the declarations alone, in `recursion.rs`, and the Rust target
  refuses a type that holds itself with other arguments, in `regular.rs`. Neither knows of adoptions.
- **The command** has `--lang`, `--out-file`, `--out-dir` and the typed inputs. The crate already reads TOML, with
  `toml`, for the settings file of `--config`.

## The file

TOML, as the draft proposes; the spike found no reason to change it.

```toml
[rust.constructor."com.example.ids::Uuid"]
type = "foreign::Uuid"
from = "crate::adopt::uuid_from_string"
to = "crate::adopt::uuid_to_string"

[rust.enum."com.example.shapes::Colour"]
type = "crate::paint::Colour"
from = "crate::adopt::colour_from_generated"
to = "crate::adopt::colour_to_generated"
```

- **The top-level table is the language,** and `rust` is the only one: any other is an error that lists the
  languages, as `--lang` does, and is added with its target.
- **Under it, `constructor` and `enum`,** each with a table for each adopted name, which is a full name, as
  `com.example.ids::Uuid`, or without a project the bare name. Each has the three keys `type`, `from` and `to`, all
  required, and no other.
- **The paths are Rust paths, written into the generated code,** as below.
- **`type` is a path without generic arguments.** The generator adds the parameters that the Rust type keeps, in their
  order, as the draft says, so a constructor of `Tagged<T>` adopted as `crate::my::Tagged` is `crate::my::Tagged<T>`.

## What is generated

- **The arguments** of an adopted constructor are given to `from`, and taken from `to`: the value itself if the
  constructor has one parameter or field, and a tuple in the order of the declaration if it has several. The spike's
  `point_from((x, y): (f64, f64))` is the case of two. A constructor with no parameters has `()`.
- **`from` returns `Result<T, String>`, and so does `to`,** with the arguments for `T`. A refusal of `from` is an error
  of reading at the value, and one of `to` an error of writing with the path of the value.
- **A constructor of an enum of several constructors** is adopted inside the enum's impl: its variant holds the adopted
  type, and the arm of its constructor reads the arguments and calls `from`. No impl is made for the adopted type, so
  one Rust type can be adopted for several constructors, as the draft says.
- **The one constructor of an enum,** as `Uuid`, makes the enum's Rust type the adopted type: no `struct` is
  generated, every use of the enum is the adopted type, and the three impls are for it, as in the spike.
- **A whole enum** keeps its generated enum, as the step in between. Every use of the enum, its own constructors
  included, is the adopted type, and the three impls for the adopted type read and write the generated enum and call
  `from` and `to`, as in the spike.
- **Two adoptions that would give one Rust type two impls,** two enums or enums of one constructor with the same `type`,
  are an error, since Rust would refuse the file. The generator compares the paths as they are written, so two paths to
  one type are left to Rust's error.
- **A default** of an adopted type is built from the defaults of the arguments and given to `from`. **Decided,** from
  the spike: a refusal is a `NoDefault::Refused` with the function's reason, and the message is
  `The default value of argument id is refused: "" is not a UUID`. The spike's message, through `NoDefault::None`, was
  `Type "" is not a UUID has no default value`.
- **The path of an error through `Some`** is `colour.0`, since the value is written `Some(...)` and the argument has no
  name. **Decided:** it is kept, as faithful to the text, and not hidden for `Option`.

## Recursion

An adopted type is opaque: the generator cannot see into it, and its author boxes what it needs. So the graph of what
holds what by value loses the edges out of an adopted constructor, and the edges into an adopted enum and into an enum
whose one constructor is adopted. **`boxed` is worked out again with adoptions,** by giving `recursion.rs` the set of
adopted names, and so is the check of `regular.rs`: a cycle through an adopted type is no cycle in Rust. `has_default`
does not change, since the default of an adopted type is still built from its arguments.

## Errors in the file

Each is exit code 2, before anything is generated, with the file and what is wrong, as the errors of `--config` are:

- a file that cannot be read, or is not valid TOML;
- a language other than `rust`, a table other than `constructor` and `enum`, a key other than `type`, `from` and `to`,
  or one of them missing or not a string;
- a name that the run does not have, as a constructor or as an enum;
- a constructor with no body, adopted on its own, which says to adopt its enum, as the draft says;
- an enum and one of its constructors, both adopted;
- the prelude's `Bool` or `Option`, or one of their constructors, which are Rust's own;
- two adoptions that would give one Rust type two impls, as above;
- a path that is not valid, as below.

`--adopt` with a language that has no adoptions is not a case yet, since Rust is the only one.

## The paths

The generated code is in the module of a project, which may be a submodule, so a path in the file is only right if it
does not depend on where it is written. Rust resolves `uuid::Uuid` first against the items of the module, so a
generated record whose fields are named `uuid` would hide the crate.

**Decided: a path that starts with `crate::` or `::` is written as it is, and any other is written with `::` in front,**
as the path of another crate, so `uuid::Uuid` is `::uuid::Uuid`. A path that starts with `self::` or `super::` is an
error, since it would mean something else in each module, and so is one that is not a Rust path, such as one with
generic arguments or an empty segment. The file can use the paths that a program writes, and the generated code cannot
be misled. Requiring `crate::` or `::` would refuse the common `uuid::Uuid`, and writing paths as they are would be
wrong where a module has an item with the name of the crate.

## The tests

- **Unit tests** of the file: each error above, with its message; a valid file read into the adoptions; and the
  writing of a path.
- **Unit tests** of the generator: the text of an adopted constructor of an enum of several constructors, of the one
  constructor of an enum, of a whole enum, of a generic constructor with a parameter left out, and `boxed` worked out
  again where an adopted type breaks a cycle.
- **The harness** gains the spike: the scenario is generated with `syntax/examples/codegen/shapes/adopt.toml`, and the
  harness has the program's side, `adopt.rs` and `paint.rs`, and the stand-in crate `foreign`, in
  `tests/codegen-harness/`. Its scenario tests then run with the adoptions, generated and not written by hand, with the
  two refusals that step 5 left for this step, and the cases of the spike. `shapes.rs`, which is generated without
  `--adopt`, does not change.
- **The functests of the command:** a folder `syntax/examples/codegen/adopt-invalid/`, with a file for each error, whose
  first line is `# expect: <message>`, each run with exit code 2 and nothing written.

## Decision

- **0055: how a type is adopted, and why not in the project-file:** the closed set kept, the two functions, the
  arguments, the whole enum with the generated one as the step in between, the file and its errors, and the paths.

## The commits

1. `NoDefault::Refused` in the runtime, and the file: reading, checking against the model, and its errors, with
   `--adopt` and the functests of its errors.
2. `boxed` and the check of regularity worked out again with adoptions.
3. The generated code for adoptions, with its unit tests.
4. The harness with the adoptions of the scenario.
5. The decision, and the draft and the outline brought up to date.

## As it was built

*Done,* in the five commits above, with decision 55. What the plan did not say:

- **The writer of an adopted generic type bounds each of its parameters by `Clone`,** since `to` gives the arguments
  back owned. A scratch crate with a generic constructor adopted as `Tag<T>` did not compile without it.
- **A `struct` that is not generated is renamed without a warning,** as that of an adopted constructor, or an enum whose
  one constructor is adopted, so that a program is not told of a name that it never sees. The names are still worked
  out as before, so an adoption renames nothing else.
- **An enum whose one constructor is adopted is a comment in its module,** `// The enum ... is adopted as ...`, since a
  doc comment needs an item.
- **Two adoptions that would give one Rust type two impls** are reported at the later of the two in the file.
- **The mistakes of the TOML are reported in the words of the `toml` crate,** as for the settings file: a language
  that is not known is `unknown field `python`, expected `rust``.
- **The spike's cases are in the harness,** with its stand-in crate `foreign` and the program's side, `adopt.rs` and
  `paint.rs`, and the scenario's file of adoptions, `syntax/examples/codegen/shapes/adopt.toml`, which adopts `Point`
  as well as `Uuid` and `Colour`, so that a constructor of two arguments is tested. The spike itself stays in `_build/`.
- **An adopted constructor of an enum of several,** and a generic one with a parameter left out, were compiled and run
  in a scratch crate, and their generated text is held by unit tests; the harness has neither.

## Done when

- The shapes scenario runs in the harness with its adoptions, `Uuid` and `Colour`, generated and not written by hand.
- Each mistake in the file is an error with its message and exit code 2.
- `just test` passes.
