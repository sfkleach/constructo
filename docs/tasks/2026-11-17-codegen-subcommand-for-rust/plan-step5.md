# Plan: step 5 of codegen, the Rust codec

This plans step 5 of the [implementation plan](implementation-plan.md): the codec, which step 4's file gains, so that
a program reads and writes values of the generated types. It follows the draft's sections "What is written for Rust",
"The codec of a project", "Settings", "The names in the text", "Numbers", "Consts", "Defaults" and "A faithful mapping
first". Adoption is step 6, so here every type of the run is generated, `ids::Uuid` of the scenario included.

## What the code has today

- **The file of step 4:** the types, with every path in full, a module for each project but the prelude, the names
  `Codec`, `codec`, `ReadValue`, `WriteValue` and `DefaultValue` reserved, and parameters that no field holds left
  out. It compiles with no crate. The functest compiles every example with `rustc`, and `shapes.rs` is checked in.
- **The model:** for each constructor its full name and `written`, the form in which the root project writes it:
  bare, through a group, or in full. There is nothing for the forms that the root project **reads**, which are more:
  the full name, `local::Circle`, and a renamed name. Each const has its full name, its type and its value, with every
  name in full, and a reference to another const kept as a reference. Each enum has `has_default`.
- **The runtime,** `constructo-runtime`: the parser, whose `parse_values` gives a `Vec<ast::Value>`; the tree, which
  keeps the text of each number, each name as it was written and the positions; `values_text::render`; `MAX_DEPTH`,
  200, the parser's one limit; and `Diagnostic`. It has nothing for a codec yet.
- **`convert`:** `--to=json-typed` writes a value with its consts replaced, its defaults filled in and its numbers
  widened, and `--from=json-typed --type=T` writes the text of a JSON value, with the names in the form of the project
  and every argument by name, as the codec is to write it. It keeps the text of a number as it was written, so
  `1.50` stays `1.50`. It refuses `Some(None)`, which JSON cannot hold.
- **`check`'s options** that change what a value may be are `--widening`, on by default, and `--default-limit`,
  10,000.

## The generated code

The file of step 4, with the codec after the types. The header says the version of `constructo-runtime` that it
needs, and every path to it is in full, `::constructo_runtime::codec::Error`, as for std.

**The traits** are generated with the codec, as the draft says, so that step 6 can implement them for a type of any
crate:

```rust
pub trait ReadValue<T> { fn read_value(&self, value: &Value, depth: u32) -> Result<T, Error>; }
pub trait WriteValue<T> { fn write_value(&self, value: &T, depth: u32) -> Result<Value, Error>; }
pub trait DefaultValue<T> { fn default_value(&self, budget: &mut usize) -> Result<T, String>; }
```

They have one parameter more than the draft's, which the program does not see, since it calls `read` and `write`:

- **`depth`**, the nesting of the value so far. The parser's limit bounds a text, but a const's value is read where it
  is referred to, so a chain of consts is deeper than the text, and a program can build a value that is deeper than
  any text, such as a long chain of `Cons`. One count, against one limit, covers both, and the writer's own recursion.
- **`budget`**, the nodes that a default may still have, as `--default-limit` counts them for `check`.

**The `Codec`** has the settings and nothing else. Its tables are functions with a `match`, which are shared by every
codec, as in the spike:

- `fn constructor(name: &str) -> Option<&'static str>`: from each form that the root project reads, to the full name;
- `fn written(full: &str) -> &'static str`: the form that the root project writes;
- `fn constant(name: &str) -> Option<(usize, &'static str)>`: from each form, to the index of the const and the text
  of its value, with full names.

**The methods** that a program calls are `codec()`, `read::<T>(&self, text)`, `read_all::<T>(&self, text)`,
`write::<T>(&self, &T)`, `write_all`, and a setter for each setting. `read` parses the text, gives each value to
`read_value` with depth 0, and refuses a text with more or fewer than one value.

**The impls,** for every generated type and for `u64`, `i64`, `f64`, `String`, `bool`, `Vec<T>`, `Option<T>` and
`Box<T>`, each generic one with the bounds that it needs on its kept parameters: `ReadValue` and `DefaultValue` for
reading, `WriteValue` for writing, `DefaultValue` for the default.

- **Reading an enum** resolves the name of a tagged value with `constructor`, which is an error at the name,
  `Unknown constructor Triangle`, if the run does not have it, and then matches the full name against the
  constructors of the enum, `Expected Shape but found Point` for any other. Then the body is read in its form: none, a
  record with every field required and no other, a tuple with the positional arguments first and the named ones by
  name or by position, or an array. A named argument that is left out takes `default_value`, and its failure is an
  error at the constructor. A const is read where it is referred to, below.
- **A record without a name** is read from an untagged record, with its fields in any order.
- **Writing** builds the tree, with each name in the form of the `names` setting, every argument of a tuple by name if
  it has one, and the fields of a record without a name in their sorted order, and then renders it. So it writes what
  `convert --from=json-typed` writes for the same value.
- **Defaults** are the draft's: `0`, `0.0`, `""`, `[]`, `false`, `None`, a record of the defaults of its fields, and
  an enum's first constructor with the defaults of its arguments. An enum whose `has_default` is false returns
  `Type Chain has no default value`, as `check` says.

**The messages are `check`'s,** where the error is the one that `check` finds: `Expected Nat but found String`,
`Missing field height in Box`, `Unknown const nothing`. `check` works out the types as it goes, and the codec is
given them, so the two do not always find the same error first, and only the refusal is tested (below).

## The runtime

A module `codec` of `constructo-runtime`, for what the generated code calls and needs no trait. It is public, and so
a promise once it is published in step 8, so it is kept small:

- **`Error`:** the message; the position in the text, for an error of reading; the path in the value, by field and
  index, which is added to as the error passes out of each field and element, and is built only when there is an
  error; and the const, if the error is inside a const's value. It is shown as
  `shapes[0].colour: Expected Nat but found String (line 3, col 12)`, and with a const as
  `..., in the value of const unit_circle: ...`.
- **`Names`:** `Project` or `Full`.
- **The helpers** of the spike, `fields` and `arguments`, which take the parts of a record and a tuple in the order of
  the declaration, and the builders of a tree.
- **Numbers:** a number is read from its text, and `+5` is an `Int` and never a `Nat`, as `check` says. A `Nat` or an
  `Int` that does not fit is `The number 18446744073709551616 does not fit Nat`. A `Float` is Rust's nearest `f64`, a
  text that is too large for one, `1.0e400`, is refused, and one too small is zero. A `Float` is written as the
  shortest text that reads back, which is what Rust's `{:?}` gives, with `.0` put in where it has none, as `1.0e300`;
  `-0.0` is written as `0.0`; and a value that is not finite is refused. An `Int` that is not negative is written with
  `+`, as `convert` writes it.

## The names

The codec reads every form of a name that the root project reads, and no other. **The model gains them:** for each
constructor and const of the run, the names that a values file of the root project resolves to it. That is the full
name, the name through each group whose scope has it, `local::Circle` among them, and the bare name if `local` has
it, each the new name where an import renames it, and none where an import hides it. Without a project, the forms
are the name and `local::` with it.

They are worked out in `resolve`, beside `written`, from the same scopes, so that a name that `check` resolves is one
that the codec reads, by construction. A unit test of `resolve` holds the two together: for each project of
`syntax/examples/project/valid-names`, each form that it gives resolves in `check` to that declaration.

## Consts

- **A reference to a const** is resolved with `constant`, as a constructor is, and its value is parsed and read as the
  type that is expected there, with the depth at the reference. An error in it is at the reference, with the const
  named.
- **A const is parsed once for each codec,** which holds a `OnceLock<Value>` for each, so the codec stays `Send` and
  `Sync`. The data can refer to a const many times, but the size of a const is fixed by the declarations, so it can
  make the work larger only by a factor that is known when the code is generated.
- **A const whose value does not fit is refused when the code is generated,** as the draft says: the generator reads
  each const's value as its type, with the codec's own rules for numbers, and an error is at the const, exit code 1.
- **Writing never writes a const.**

## The settings and limits

The settings of a codec are its properties, set with `&mut self`:

- **`names`**, `Project` by default, or `Full`. Without a project the two are the same.
- **`max_depth`**, the deepest nesting that the codec reads or writes, counted as the parser counts it, through
  consts as well. Its default is the parser's `MAX_DEPTH`, which is also its ceiling: the parser cannot read deeper,
  so a deeper value that was written could not be read back. A program may lower it.
- **`default_limit`**, the most nodes in a default.
- **`widening`**, whether a `Nat` is read as an `Int` or a `Float`, and an `Int` as a `Float`.

**Decided:** `default_limit` and `widening` have, as their defaults, the values that the command was given for
`--default-limit` and `--widening`, so that the codec accepts what `check` accepts with the same options, which is the
oracle. They change only what a codec does, so they are still the codec's settings, as the draft says. There is no
limit on the size of a text, which the parser does not need.

## Decided, which the outline left to this plan

- **`read_tree` is not added.** It would be a public function of the runtime, and a promise, for a use that no program
  has yet. The draft leaves it to the first program that asks, and nothing here makes it harder to add.
- **serde is not supported** in the first version, as the outline says, and step 7 documents it.
- **No Rust items for consts,** as the draft says.

## The tests

- **Unit tests** in the runtime, of `Error`, `fields`, `arguments` and the numbers: the edges of `u64` and `i64`, `+0`
  and `-0`, `1.0e400`, a denormal, the shortest text of `0.1`, `1e300` and `1e-7`, `-0.0`, NaN and the infinities.
  In `constructo`, of the forms of the names in `resolve`, and of the generated text for a small case of each kind.
- **The compiling functest** builds the runtime once, with `cargo build -p constructo-runtime`, and compiles every
  example with `rustc --extern constructo_runtime=...`, so that it stays fast. `shapes.rs` is generated again and
  checked in.
- **The harness** is a crate in `tests/codegen-harness/`, with a `[workspace]` of its own so that it is not taken for
  a member. The functest copies it to `_build/codegen-harness/`, puts the generated file for the schema of
  `syntax/examples/values/typed` beside it, and builds it with `cargo`, which shares `_build/target`. For each values
  file of the corpus, the harness reads every value as its type, writes it, and prints what it wrote, or the error.
  The functest then checks, for each value:
  - a valid value is read, and what is written is accepted by `check`;
  - `convert --to=json-typed` gives the same output for the value and for what was written, which compares them with
    their consts and defaults filled in, and where JSON cannot hold the value, as for `Some(null)`, what is written is
    read and written again unchanged;
  - with `names` set to `Full`, the same;
  - an invalid value is refused, except where `check` refuses it only because it cannot work out a type that the
    harness gives, as `Cell("x")` read as a `Cell<Nat>`, which is accepted, and the harness's table says so.

  The corpus's numbers are in the shortest form, so the comparison is exact. A number that is not, such as `1.50`,
  is a case of its own, with the text that the codec writes for it.
- **Beside the corpus:** two projects that each have a type named `Shape`, read by two codecs in one program; the
  shapes scenario read, written and read back, as its spike does, with the refusals of its table that do not need
  adoption, which step 6 adds; the names of a project with a group, a rename and a hide, read in every form and
  written in the project's; a chain of consts deeper than `max_depth`; a value built deeper than it; a default over
  the limit; and a `Float` that is not finite, written.

## Where the type of each value of the corpus is given

The harness reads a value into a Rust type, which has to be written in its source, and a values file does not say the
type of a value. Of about fifty valid values, twenty have a generic or untagged type, and every invalid value needs
one too.

**Decided: a table in the harness,** checked in, which gives, for each file of the corpus, the Rust type of each of
its values, `List<f64>` and so on. The compiler checks the types, the corpus does not change, and the harness fails if
the table and a file have different numbers of values, so that a value added to the corpus is not missed. Two values
that have no Rust type, `{ anything: ["goes"] }` and `[{ a: 1 }, { b: 2 }]`, are marked to be skipped. A comment
`// type: List<Float>` before each value in the corpus, as the outline proposed, would keep the type next to its
value, but a script would then have to turn it into a Rust type, knowing the generated names, for a convention of the
corpus that only the harness reads.

## Decisions

The outline gives step 5 seven entries of the draft's "Decisions to write". They are written as four:

- **0051: the codec of a project:** one for each project, holding its names, consts and settings; readers and writers
  that work through it; in Rust, traits generated with each codec; and generation settings given to the command while
  codec settings are properties of the codec.
- **0052: reading is constrained by the target type:** the constructors are a closed set, and names and consts are
  resolved as the value is read.
- **0053: names are written as the project writes them,** and in full when the codec is set to.
- **0054: a mapping never changes a value, except a `Float`;** faithful first, and idiomatic where it is safe.

## The commits

1. The forms of a name that a project reads, in `resolve` and the model, with their tests.
2. The module `codec` of the runtime, with its tests.
3. The codec in the generated file, with its unit tests, the compiling functest with the runtime, and `shapes.rs`.
4. The harness and the functests beside it.
5. The decisions, and the draft and the outline brought up to date.

## Done when

- Every value of the corpus passes the oracle, or is refused, as the harness's table says.
- The shapes scenario is read, written and read back, as its spike was, without adoption.
- `just test` passes.
