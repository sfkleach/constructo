# Plan: Step 3 of the convert subcommand, `--to=json-typed`

This plans Step 3 of [convert-subcommand.md](convert-subcommand.md), which has the mapping to JSON. This document is
about
how to get there from the code as it is. It is the largest step so far, because `json-typed` needs what `check` works
out, the type of every value, and `check` only ever says whether the values fit.

## What the code has today

- `check_with_layout` in `src/lib.rs` does everything in one function: parse, resolve the names (which rewrites them to
  canonical form, such as `Circle@com.example.shapes`), validate, check the consts, and check every value with
  `check_value`. It returns a `Report` of diagnostics and outcomes, and nothing about types.
- The checker in `src/typecheck.rs` infers by unification. A value is checked against a type variable, and the variable
  is bound as the value is met. The type of a node is therefore not known until the whole value is checked: in
  `[1, 2.5]` the `1` is first a `Nat` and ends in a `Float` slot. Omitted arguments are checked at the end for the same
  reason. The checker keeps no record of the type of a node.
- A declared default is only counted (`default_nodes`), with an explicit stack, against
  `--default-limit`. Nothing builds one.
- The prelude's `Bool` and `Option` are `Bool@constructo.prelude` and `Option@constructo.prelude` after resolution,
  whatever the project calls them, so they are told from a user's own `Bool` by the name alone.
- A const reference is checked by `check_const_ref`, from the declared type or the inferred scheme. The value of the
  const is not looked at again.
- Loading a project, reading its lock, verifying it, and reading the inputs are all in the arm of `main.rs` for
  `Command::Check` and in `check()`.
- `json-exact` has `Json`, which can only hold strings, arrays and objects. The lexer accepts `007`
  and `00.5`, which are
  not JSON numbers.

## Decisions of the owner

1. **`--dev` and `--no-dev`.** `convert` takes them, as `check` does, with the same meaning and the `dev` key of
   `[check]`. A project that is being developed has no lock yet, and could not be converted otherwise. This replaces the
   line in the task that said `convert` does not take `--dev`.
2. **Which values are checked.** Only the values that are converted. Converting the first value is a convenience, and a
   file of any number of values is converted by its first one. A type error in a value that is not converted is not
   found, as a duplicate name in a later value is not found by `json-exact`.

## Design

### The pipeline takes a mode

`check_with_layout` is not cut in two. Its steps are the ones that a conversion needs, in the same order, and cutting
them would copy their errors and their way of showing names. Instead the loop that checks the values takes a mode.
`check` checks every value, as it does. A conversion checks and encodes only the values that it converts, and stops
there: it counts the values, and the first one that is converted is checked and encoded, and the position of the next is
kept for the warning. The encoder is called inside the loop, where the `Env` is alive, so there is no need to hand the
types out. The report of the run, its diagnostics with the names shown as they are written, is the report of the
conversion.

The front is the same code, so the options (`--no-prelude`, `--widening`, `--allow-refs`, the limits, `-x`) mean exactly
what they mean for `check`, and need no second implementation. The `check` tests are the proof that it was not changed.

### The checker keeps the type of each node

`Checker` gets an optional record, `notes: HashMap<*const Value, Ty>`, filled by `check` when it is on: the type that
each
value node was checked against. The values live in a vector that is not changed while the check runs, so the address of
a
node is a name for it. When the check of a value has passed, every variable that was bound is resolved with `shallow`
and a new function that goes all the way down, so that the encoder sees the final type. A flexible variable that a wider
number widened is then a `Float`.

A type that is still open at the end only happens where the value has nothing to say, an empty array or `None`, and the
encoding of those (`[]`, `null`) does not need it.

The check that is switched on for conversion is the same `check`, so a value that converts is a value that `check` would
accept, as for any single value. It is a separate entry point, `check_value_typed`, and `check_value` is unchanged.

### The encoder, `src/typed.rs`

`encode_typed(env, checked, value) -> Result<Json, TypeError>` walks the value with the notes:

- **Numbers.** The text is copied, with a leading `+` dropped and the leading zeros of the whole part removed (`007` is
  `7`, `-007` is `-7` and `00.5` is `0.5`), because JSON does not allow them. A number in a `Float`
  slot that has no point
  or exponent gets `.0`. A number in a `Nat` or `Int` slot is as it is.
- **Strings, arrays and records.** An array is an array. A record is an object in the order of the
  type of the node, which
  is the declaration for a field of a declared type, and the order written for a record at the top level, whose type is
  made by the value.
- **Constructors.** The node's type says the enum, and the name is looked up in `Env`. `Bool` and `Option` are told by
  their canonical names, and are written as `true`, `false`, `null` and the unwrapped value. Any other constructor is
  written as in the mapping of the task. The name is the one the enum declares, which is the
  canonical name up to the `@`.
- **`Some(None)`** is the error of the task, at its position.
- **Omitted arguments** are written as their defaults. The nodes of a default are bounded by `--default-limit` and were
  counted by the check, so the builder cannot be asked for more. It is a loop with an explicit stack like
  `default_nodes`,
  because a chain of declarations that each start with the next can be thousands of levels deep.
- **Const references** are written as their value: the value of the const is checked again against
  the type that the node
  has, by a checker of its own with its own notes, so that two references to a generic const with different arguments do
  not share them, and then encoded. A reference where a `Float` is expected to a const `5` is `5.0`.

`Json` gets three more kinds for this: a number (kept as text), `true`/`false`, and `null`. The writer, the layouts and
the
escapes are those of `json-exact`.

### The command

- `Format::JsonTyped`. The names that said that it is not available are removed, and the messages list the three
  formats. It is the only format that takes `--project`, the types files, `--prelude`, `--no-prelude`, `--widening`,
  `--no-widening`, `--allow-refs`, `--no-allow-refs`, `--infer-limit`, `--default-limit`, `--dev` and `--no-dev`.
  Given with `json-exact`, they are a usage error, as `--to-array` is with a lines format.
- `--to-array`, `--compact`, the first-value rule and the warning are those of `json-exact`.
- The inputs are those of `check`: the project's types files, then the FILES, then `--stdin`. A
  file is a types file or a
  values file by its first token. Only the values files give values, and the first value of the first one that has one
  is converted. With no value it is the same error as for `json-exact`.
- The settings file: `convert` reads `[check]` as well as `[global]` and `[experimental]`, for the keys that apply, and
  ignores `continue` and `verbose`.
- The shared loading of the project and the inputs is lifted out of `Command::Check` and `check()` into functions that
  both use, so there is one place for the lock, the cache, the exit codes and the order of the reading.

## Steps

### Step 3a: share the loading

1. Move the code that loads a project, reads and verifies the lock and reads the inputs from `main.rs`'s arm for `check`
   into functions that `convert` can call. No change in behaviour: the functests of `check` and of
   the projects must pass
   with no change, and that is the test.

### Step 3b: keep the types

1. The notes in `Checker`, and `check_value_typed`, `check_value_as` and the `Typed` that they give. The type of a part
   is looked at one level at a time, with `shallow`, as the checker does, and `scheme` resolves it all the way down,
   with a limit, for the check of the value of a const against the type of its use. Unit tests: the type of the parts
   of `[1, 2.5]`, of a value in an enum with a type parameter, of an untagged record, of a reference to a const, and
   of what the value leaves open; a scheme to check another value against; and that the errors are those of
   `check_value`.

### Step 3c: the encoder

1. The three new kinds of `Json`, the number text rules, and the unit tests on them (`+5`, `007`,
   `-007`, `00.5`, `5` for
   a `Float`, `1.50`, `1.5E+2`).
2. `src/typed.rs`, one unit test for every row of the mapping of the task, and for each of: a bare
   constructor of an enum
   that has data too, a field that is `None`, `Some(Some(1))`, `Some(None)`, a record whose fields
   are written in another
   order than declared, an omitted argument whose default is a record, an enum or an array, a const reference, a generic
   const referred to twice with different arguments, an enum that is a user's own `Bool`, and `--no-prelude`.

### Step 3d: the command

1. `Format::JsonTyped`, `convert_typed`, the options and the settings, the usage errors for the
   options that do not apply
   to `json-exact`.
2. Functional tests: a schema and values in `syntax/examples/convert/typed/`, with the expected JSON, pretty and
   `--compact`; the first value and the warning; `--to-array`; a type error and an unknown constructor, with exit code 1
   and no output; a type error in a value that is not converted, which is not found; `--project` with a lock, and
   `--dev` without one; the options that apply to `json-exact` only; a settings file with `[check]`.

### Step 3e: the documents

1. The spec `docs/specs/convert-command.md` gets the `json-typed` section: the inputs, the options, and the mapping with
   its examples. A new decision, 37, says that the typed shape follows the convention of serde and why, and the choices
   that the convention does not make for us (a bare constructor, the unwrapped `Some`, defaults written out, consts
   inlined, `Some(None)` refused, numbers as the text that was written).
2. `docs/specs/exit-codes.md` and `docs/specs/config-file.md`; the CHANGELOG and the README. `/review-specs`. Tick the
   tasks of Step 3.

## To come back to

- **Leading zeros.** The lexer accepts `007` and `00.5` as numbers, which is probably an oversight and not a rule. The
  encoder strips them so that the output is valid JSON, and a round trip of the text is not a goal. Whether the lexer
  should refuse them is a question for the grammar, to be taken up separately.

## Risks

- **Stack depth.** The default builder, and the encoder, must not recurse over a type or a value
  that is deep. The parser
  limits the nesting of a value, and the builder is a loop. The encoder is as deep as the value, which is bounded.
- **The address of a node as its name.** It is only valid while the values are not moved. The notes are made and used
  inside one call, where they are not.
- **A second route to the same answer.** The encoder repeats some of what the checker did, such as binding the arguments
  of a tuple to the parameters. It uses the same `bind_arguments`, and does not do its own.

## Verification

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.
- `bash tests/functest.sh _build/target/debug/constructo`, with the `check` tests unchanged by 3a and 3b.
- `python3 -I .claude/skills/review-specs/check_specs.py`.
- By hand: the typed output of the examples in `syntax/examples/values/typed/`, read for the
  shapes. As for `json-exact`,
  the output is checked with an independent JSON reader, since the lines of a typed file have to be
  JSON that other tools accept.

## Size

The biggest step of the task: a change to the shape of `main.rs` and `lib.rs`, a change to the checker, a new module of
some hundreds of lines with its tests, and a new section of the spec. It could be two pull requests: 3a and 3b, which
change nothing for the user, and then 3c to 3e.
