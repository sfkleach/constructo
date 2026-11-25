# 0051 - A codec for each project, holding its names, consts and settings, 2026-11-25

## Issue

`constructo codegen --lang=rust` writes the types of a run, as
[decision 49](0049-how-the-modules-and-names-in-rust-are-chosen.md) says, and a program has to read and write their
values as Constructo text. The names in that text depend on a project: the same constructor is `Circle` in one,
`shapes::Circle` in another, and `com.example.shapes::Circle` in any. The consts that a text may name are those of the
run. And a program that talks to two services has two runs, each with a `Shape`. The question was where the reading and
writing live, what they hold, how a program changes how they behave, and how they are built in Rust.

This is a **Leap**, from the draft of the codegen task, written after the code, which the codegen harness holds to
`check` and `convert`. Building it changed three things, which are marked below.

## Factors

- The type of a value knows nothing of any project, so the names cannot be its own: `Circle` is written differently by
  two projects that both use it.
- Rust's orphan rule lets a crate implement a trait for a type only if the trait or the type is its own. A trait of the
  runtime crate could not be implemented by a program for a type of a third crate, such as `uuid::Uuid`, which
  adoption (step 6 of the task) needs.
- A program may read from a peer that it does not trust, and want tighter limits there than for its own files, from
  one generated file.
- Some settings change the code that is generated, such as the type of a field, which the program's compiler checks.
  Others change only what the code does when it runs.
- Rust picks the impl of a trait bound by the where-clauses of the impl it is in, before it looks at the types around
  it. In `impl<T> ReadValue<List<T>> for Codec where Codec: ReadValue<T>`, a call `self.field(...)` whose part is a
  `Box<List<T>>` is taken to be a `T`, and does not compile.

## Decision

**One codec for each run, named after its root project.** The run is the root project and the projects that it
imports, as `check` loads them, and the codec reads and writes the values of their types, with their names and consts
and no others. The generated file has `pub struct Codec` and `pub fn codec()`, which makes a new one each time. A
program calls `codec.read::<T>(text)`, `read_all`, `write(&value)` and `write_all`.

**Readers and writers work through the codec.** Every reader and writer is given the codec, and asks it for the full
name of a constructor, for the value of a const, and for its settings. The types are plain data, and have no `parse`
or `to_text` of their own.

**In Rust, the traits are generated with each codec:** `ReadValue<T>`, `WriteValue<T>` and `DefaultValue<T>`, each
implemented by the codec for every type of the run, for `u64`, `i64`, `f64`, `String`, `bool`, `Vec<T>`, `Option<T>`
and `Box<T>`. They are the program's own, so a program can implement them for a type of any crate, and two codecs can
treat one type differently. Each has one parameter more than the draft gave it, which a program does not see: the
depth, for the limit below, and for a default, a budget of nodes.

**The impls for the types of a project are in its module,** where the types are, so that the type of each part is
written as the type itself is. *Changed while it was built,* from all of them at the root. The codec, its traits and
its private helpers are at the root, and a submodule reaches them through `super`, as Rust lets a child module see its
parent's private items.

**Each helper of a generic impl is given its type,** as `self.field::<Box<self::List<T>>>(...)`, since Rust would
otherwise take it from the impl's bound. *Added while it was built.*

**The settings of a codec are its properties,** changed with `&mut self`, so that a change is the program's own and
does not reach another user of the codec:

- `names`: `Project`, to write each name as the root project writes it, or `Full` (decision 53);
- `max_depth`: the deepest nesting that is read or written, counted as the parser counts it and through consts. Its
  default is the parser's limit, which is also its ceiling, since the parser could not read back what is deeper;
- `default_limit`: the most nodes in the default of an argument that is left out;
- `widening`: whether a `Nat` is read where an `Int` or a `Float` is wanted, and an `Int` where a `Float` is.

The defaults of `default_limit` and `widening` are the values that the command was given for `--default-limit` and
`--widening`, so that a codec reads what `check` reads with the same options. They are still the codec's settings,
since they change only what it does.

**A generation setting is given to the command,** since it changes the generated code. Rust has none in the first
version; TypeScript's `Nat` as `number` or `bigint` is the example of one.

**A default that cannot be given is a `NoDefault`,** which says whether the type has none or the default is too large,
so that the error at the argument that was left out can name the parameter and the codec's limit. *Changed while it
was built,* from a `String`.

**What every codec calls and needs no trait is in the runtime,** in `constructo_runtime::codec`: the error, with its
position, its path in the value and the consts that it is inside; the setting of names; the parts of a record and a
tuple in the order of the declaration; the numbers; and the builders of a tree. The part of the codec that is the same
in every file is generated from a template, `src/codegen/rust/codec.rs.in`.

## Consequences

- A program with two services has two codecs, which do not clash, and a value of one cannot be given to the other.
  A project that both import is generated in each, so its types are two types in Rust, which is accepted for the first
  version.
- A program can implement the traits for its own types, which adoption (step 6) builds on.
- The generated file is long, since every path is written in full and every helper of a generic impl has its type,
  but it is not meant to be read often.
- The module `codec` of the runtime is public, and becomes a promise when the runtime is published.

## Pros and Cons of Options

### Where the traits are

- **Generated with each codec.** A program can implement them for any type, and two codecs do not collide. Each file
  has a few dozen lines that are the same.
- **In the runtime crate.** One copy, but the orphan rule stops a program from implementing them for a type of another
  crate, and two codecs would share one impl for a type.

### Where the impls are

- **In the module of each project.** The type of each part is written as in the type itself.
- **At the root.** One place, but a part's type would have to be written again with a path from the root, and a record
  without a name, which has a `struct` in each module that uses it, would need the path of the right one.
