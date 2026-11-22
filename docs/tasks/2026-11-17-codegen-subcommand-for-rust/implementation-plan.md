# Plan: implementing `codegen` for Rust

This plans the work of [codegen-subcommand-for-rust.md](codegen-subcommand-for-rust.md), which says what is to be
built. This file says in what order, what each step delivers and how it is tested, and where each question that the
draft leaves open is settled. The draft is the authority on what the generated code does; where the two disagree, the
draft is right and this plan is corrected.

The three thought experiments, for [Rust](scenario-shapes.md), [Python](scenario-shapes-python.md) and
[TypeScript](scenario-shapes-typescript.md), raised fourteen questions between them, and all have been decided. The
third raised only questions of its own language, which is the sign that the common design is stable enough to build.

## What this task delivers, and what it does not

- **It delivers** `constructo codegen --lang=rust`, which writes one Rust file for a project: its types, a codec with
  its tables of names and consts, and the reading, writing and defaults of every type, with adoption of constructors
  and of whole enums. It delivers the crate `constructo-runtime`, which the generated file depends on, and a
  conformance suite for runtimes.
- **It does not deliver** a second target. Python and TypeScript are tasks of their own, which start from the model of
  step 3 and the conformance suite of step 2. So the parts of the draft that only another target needs are not built
  here: the marker file and the folder of `--out-dir`, the generation settings and how the command is given them (Rust
  has none in the first version), the faithful box for `Option`, and the defaults of a class.
- **It does not change** `check`, `convert` or `query`, apart from moving six modules into the runtime crate in
  step 2, which none of them sees.

## How the work is done

- **Each step gets a plan of its own before it starts**, `plan-stepN.md`, written against the code as it is then, as
  for the `convert` task. This plan is the outline that they fill in, and it is not detailed enough to code from.
- **The spikes are a reference and not a source.** The generated code of `scenario-shapes/rust` predates several
  decisions: its traits are on the value types, not the codec, and it resolves names in a pass of its own. Code is
  written from the draft, and the spikes show what it should look like in use.
- **Every step leaves the repository green:** `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`, and
  `tests/functest.sh`. A step that adds a kind of test adds it to one of these, so that it runs every time.
- **The decisions are written by the step that implements them**, so that each is checked against working code when
  it is written. The table at the end gives each entry of the draft's "Decisions to write" a step. The next number is
  0045.
- **Three pull requests**, so that the large mechanical change is merged before the new code depends on it:
  - the design and steps 1 and 2, which change no behaviour and touch every import of the six modules;
  - steps 3, 4 and 5, which give a working codec without adoption;
  - steps 6 and 7, after which the task moves to `docs/tasks/done/`.

## Step 1: what a number is, in the specifications

Documentation only.

- **What:** the values of `Nat`, `Int` and `Float`, as step 1 of the draft says. `Nat` and `Int` are whole numbers of
  any size. A `Float` is a real number, extended by +∞, −∞ and nullity under transreal arithmetic, and the notation
  writes only the rationals among them. Nullity equals itself, and there is no negative zero.
- **Where:** a section "What a number is" in `docs/specs/type-checking.md`, before "Numbers and widening", which already
  says that widening to `Float` "may lose precision"; that sentence is reworded to say it is the nearest value. A spec
  of its own is not needed while the model is a paragraph.
- **Decision:** 0045, the model of `Float`, with the reasons that the user gave: it is the reals, the notation writes
  rationals, IEEE 754 is not the model, and a target's binary type is an approximation that is accepted.
- **Done when** the spec says it, the decision is written, and the review-specs check passes.

## Step 2: the runtime crate and the conformance suite

- **The workspace.** The root `Cargo.toml` gains `[workspace]` with the member `constructo-runtime/`, and `constructo`
  stays where it is and depends on it by path. `.cargo/config.toml` already puts the target in `_build/target`, which
  is shared by the workspace.
- **The move.** `lexer`, `parser`, `ast`, `values_text`, `limits` and `diagnostics` go into the runtime crate. Their
  `use crate::` lines are only among the six, as the draft found, and that has been checked again for this plan.
  `src/lib.rs` re-exports them under their present paths (`pub use constructo_runtime::parser;` and so on), so the
  other files of `constructo` do not change.
- **The limits are split.** `limits.rs` also has the limits of JSON, XML, `json-typed`, inference, defaults and queries,
  which the runtime does not need. The runtime keeps `MAX_DEPTH` and whatever else the parser uses, and
  `constructo::limits` keeps the rest and re-exports the runtime's, so that they are still seen together, as the
  module's comment and decision 43 require.
- **The types parser stays in the runtime** for now, which settles that open question: cutting it is a refactoring of
  the parser for no gain to this task. It is noted as a later possibility.
- **The runtime is not published** in this task. The generated file names the version that it needs, and a program
  depends on the crate by path or by git until a release decides otherwise. This settles the other open question of
  the section for the first version.
- **The conformance suite** is a folder, `syntax/conformance/`, of inputs, each with the expected result beside it: the
  canonical text that `values_text` writes, or `// expect:` with the message and the position of the error, as the
  invalid examples have now. Its first cases are the values under `syntax/examples`, and cases are added for the
  limits: deep nesting and large inputs. A runner in `tests/` checks the Rust runtime against it, so the runtime that
  the others are held to is the first to pass it.
- **Done when** everything that passed still passes, with no change to any test, and the suite passes.

## Step 3: the model

- **What:** a description of the declarations of a run that every target writes from. It is built after the sources
  have been checked as `convert_typed` checks them, with the prelude, from the `Env` and the resolution, and a run that
  has errors writes nothing.
- **What it holds:**
  - the projects of the run, each with its full name, its imports and their groups, and the module name that a target
    derives from it;
  - for each enum, its full name, its parameters, and its constructors in order, each with its body (none, tuple,
    record or array) and the types of its fields;
  - types with **full names** (`com.example.shapes::Shape`), never the checker's internal `Shape@com.example.shapes`,
    as the Rust scenario found;
  - the record types without a name that are used, with the project that uses each;
  - the default of every type, as the checker works it out;
  - the consts of the run, each with its full name, its type and its value with full names;
  - for each constructor, the form in which the root project writes its name: bare, through a group, or in full.
- **What it works out**, so that no target does it again:
  - the fields that are on a cycle of types held by value, which Rust boxes;
  - for each type, whether a value of it can be the absent value, which Python needs and Rust does not. It is in the
    model because it is a fact of the declarations, and the step that adds Python will test it.
- **Where:** `src/codegen/model.rs`, in the library, as `convert` is, so that it can be used without the command line.
- **Tests:** unit tests on the projects of `scenario-shapes/projects`, on the types examples, and on cases for each
  fact above: a cycle through two enums, a cycle broken by an array, a generic enum, a record without a name used in
  two projects, a const that refers to another.
- **Done when** the model of every valid types example and project example can be built.

## Step 4: Rust, the types

- **The command:** `constructo codegen --lang=rust` with `--project`, the inputs of `check`, and `--out-file`, else the
  standard output, written to a new file that then takes the place of the old one. `--out-dir` with `--lang=rust` is a
  usage error that names `--out-file`. `--adopt` exists from step 6.
- **What it writes:** everything in the draft's "What is written for Rust" and "Modules and names in Rust" that is about
  types: the module of each project, enums and their variants, an enum with one constructor as one `struct`, the
  constructor that is called like another enum, generics, `Vec`, `Option`, `bool`, boxing, records without a name, the
  names that yield and the renamings reported as warnings, keywords, and the std items written in full.
- **Settles:** a parameter that no field uses, which Rust refuses. The likely answer is a field of
  `::std::marker::PhantomData<T>` in each variant that needs one, and the step's plan decides.
- **Tests:**
  - a functest that generates the file for every valid types example and project example and compiles each with
    `rustc --crate-type=lib --edition=2021`, which needs no crate yet, since there is no codec;
  - a functest for the names of the draft: keywords, the field `_`, a type called like a std item, an enum called like
    a generated name, the clashes;
  - the output for the shapes scenario, compared with a file that is checked in, so that a change to it is seen in
    review.
- **Done when** every example compiles, and the warnings for renamings are as the draft says.

## Step 5: Rust, the codec

- **What it writes:** the traits `ReadValue`, `WriteValue` and `DefaultValue`, generated with the codec; the `Codec`,
  with its tables of names and of consts, and `codec()`; and the impls for every type and for `u64`, `i64`, `f64`,
  `String`, `bool`, `Vec<T>` and `Option<T>`. Names and consts are resolved as the value is read, errors carry the path
  of the value, writing returns a `Result`, and a value that does not fit is refused, as the draft's sections say.
  `Float` follows the exception: the nearest value, `1.0e400` refused, the shortest text that reads back, `-0.0`
  written as `0.0`, and a non-finite value refused when written until the task for non-finite values gives it a text.
- **Settles:**
  - **the limits**: which a codec has, their defaults and their names. The parser's depth and size come from the
    runtime; the writer needs a depth limit of its own; a const's chain is held by the depth limit. They are
    properties of the codec, as the settings decision says;
  - **`read_tree`**: whether a codec reads a value from a tree that it is given as well as from text. It is a small
    addition if the reader already works from the tree, and the step's plan decides;
  - **serde**: not supported in the first version, which the documentation says. A program that wants it implements
    it itself, and the generated types can derive it later behind a generation setting.
- **Before this step,** the task [convert-writes-names-through-a-group][group] is done, so that `convert` and the
  codec write the same names and the oracle below holds byte for byte. The writing of a name in its project's form is
  in `resolve`, where `convert` can share it with the model of step 3.
- **Tests:** a harness crate, made in `_build/codegen-harness/` by the functest, depends on `constructo-runtime` by
  path and on a generated file, and is built with `cargo`. For each value of `syntax/examples/values/typed`:
  - the codec reads every value that `check` accepts, and refuses every one that `check` refuses;
  - what it writes is accepted by `check` with the same types;
  - **`convert --to=json-typed` gives the same output for the value and for what the codec wrote of it**, which
    compares the two with consts and defaults filled in, and needs no new oracle;
  - with the names set to `full`, the same, with every name written in full.

  The harness reads each value into a type, and a values file does not say its type, so the corpus gains a comment
  `// type: TYPE` for each value whose type cannot be told from its constructor, which the step's plan checks is
  few. Beside the corpus: two projects with a type of the same name, read by two codecs in one program; the shapes
  scenario read, written and read back, as its spike does; and each refusal in the table of the Rust scenario.
- **Done when** the corpus passes the oracle, and the scenario runs as the spike did.

## Step 6: adoption

- **First, a spike that compiles:** a codec whose traits are implemented for a type of another crate (`uuid::Uuid`,
  in `_build/`), inside a `Vec` and as the argument of a generic type. The Rust scenario adopts types of its own crate,
  so this case of the orphan rule has not yet been compiled.
- **Settles the format of the file:** TOML, as the draft's provisional format, unless the spike or the step's plan
  finds a reason not to. The tables are `[rust.constructor."FULL_NAME"]` and `[rust.enum."FULL_NAME"]`.
- **What it does:** `--adopt=FILE_PATH`, the two functions of an adoption, both of which may fail; whole-enum
  adoption, with the generated constructors as the step in between; the refusal of a unit constructor adopted alone;
  an adopted type breaking a cycle of boxing; and an entry that names a constructor or enum that the run does not
  have, which is an error.
- **Tests:** the spike as a functest; the adoptions of the shapes scenario (`Uuid` and `Colour`); and an error for each
  mistake in the file, with its message and exit code 2.
- **Done when** the shapes scenario runs with its adoptions, generated and not written by hand.

## Step 7: documentation

- `docs/specs/codegen-command.md`: the command, its options, its output, its messages and exit codes, and the mapping
  for Rust, including the range of each number type and the `Float` exception. `exit-codes.md` and the "Related"
  sections of the other specs link to it.
- The decisions left by the table below, and the wording of decision 39, from "driven by a type" to "constrained by
  the target type".
- `CHANGELOG.md`, the README, and the `--help` of the command.
- The task moves to `docs/tasks/done/`, as the last commit before the merge.

## Where each decision is written

| Entry of "Decisions to write" in the draft                                         | Step |
|------------------------------------------------------------------------------------|------|
| The model of `Float` (new)                                                         | 1    |
| The syntax layer is a crate, `constructo-runtime`, and what it holds               | 2    |
| Every other target has a runtime, held to the syntax by a conformance suite        | 2    |
| A codec for each project, holding names, consts and limits                         | 3    |
| How a record type without a name is named, and consts and defaults                 | 3    |
| How the modules and names in Rust are chosen                                       | 4    |
| How generated names avoid Constructo names is the target's choice                  | 4    |
| Every field on a cycle held by value is boxed, at every site                       | 4    |
| A target writes one file or a folder, with `--out-file` and `--out-dir`            | 4    |
| Reading is constrained by the target type, and the constructors are a closed set   | 5    |
| Readers work through the codec; in Rust, traits generated with each codec          | 5    |
| Names and consts are resolved as the value is read and written                     | 5    |
| Names are written relative to the project by default, in full when set             | 5    |
| Generation settings go to the command; codec settings are properties of the codec  | 5    |
| A mapping never changes a value, except `Float`; faithful first, idiomatic if safe | 5    |
| How a type is adopted, and why not in the project-file                             | 6    |
| A generated class may give a named parameter its default                           | 7    |
| The name `codegen`                                                                 | 7    |

The decision on defaults of a class is for Python, and is written in step 7 so that it is not lost, though nothing in
this task implements it.

## Risks

- **The trait solver.** The draft notes that a bound such as `Codec: ReadValue<Nested<Vec<T>>>` inside the impl of
  `ReadValue<Nested<T>>` may be more than Rust can follow. Step 4's tests compile the types only, so it is first met
  in step 5. A case for it goes in step 5's first tests, so that a change of approach is found early.
- **The size of the workspace change.** Step 2 touches the build of everything. It is kept to the move and the
  re-exports, with no other change, so that the diff can be reviewed as a move.
- **The corpus and its types.** If many values of the corpus need a `// type:` comment, step 5 may instead have the
  harness ask `check` for the type of each value, which `check` does not yet report.
- **Compile time of the tests.** The functests of steps 4 and 5 run `rustc` and `cargo`. The harness is one crate for
  the whole corpus, and it shares `_build/target`, so that it is built once a run.

## After this task

- Python, from the model and the suite, with the parts of the draft that only it needs.
- TypeScript in the same way.
- Go, as a fourth thought experiment, if a target with explicit errors and no unions is wanted.
- The text of the non-finite values, after which a codec writes them instead of refusing them.
- Publishing `constructo-runtime`, and cutting its types parser, if either is wanted.

[group]: ../todo/2026-11-21-convert-writes-names-through-a-group/convert-writes-names-through-a-group.md
