# Plan: step 7 of codegen, documentation

This plans step 7 of the [implementation plan](implementation-plan.md): the documentation of `constructo codegen` and
what it writes, the decisions that are left, and the documents that a user reads first. It changes no behaviour, apart
from the text of `--help`.

## What the documentation has today

- **The specs,** `docs/specs/`, one for each command, `check`, `convert` and `query`, and one for each file format,
  the project-file, the lock file and the settings file, with `exit-codes.md` for every command. There is nothing on
  `codegen`. Each spec has the form of `convert-command.md`: what the command does, a synopsis, a section for each part,
  the settings, the exit codes in a table, and "Related".
- **`exit-codes.md`** has a section for each command, and none for `codegen`.
- **`config-file.md`** says which tables each command reads, and does not name `codegen`, which reads `[global]`,
  `[experimental]` and the keys of `[check]` that `convert` reads for the typed formats.
- **`CHANGELOG.md`** has the group in the names that `convert` writes, which the group task added, and nothing else of
  this task: not the runtime crate and the conformance suite of step 2, nor `codegen`.
- **The README** describes `check`, `convert`, `query` and projects, and not `codegen`.
- **`--help` of `codegen`** says that the types are checked and that renamings are warnings, and nothing of the codec or
  of `--adopt`.
- **The decisions** that the draft's "Decisions to write" has not had written are two: that a generated class may give
  a named parameter its default, and the name `codegen`. Decision 39 says "driven by a type", which decision 52 calls
  the weaker statement.

## The specs

**`docs/specs/codegen-command.md`**, in the form of the other commands:

- **The command:** the synopsis, `--lang`, `--out-file` and `--out-dir`, the inputs as for the typed formats of
  `convert`, `--adopt`, that the file is replaced as a whole, and that nothing is written if there is an error.
- **What is written for Rust:** the file and its modules, the header and the version of the runtime that it needs, the
  mapping of each type, with the range of `u64`, `i64` and `f64` and the exception of `Float`, the boxing of a cycle,
  the records without a name, the parameters that are left out, the names that change and their warnings, and the types
  that are refused, from decisions 48 and 49.
- **The codec:** `codec()`, `read`, `read_all`, `write` and `write_all`; the four settings, with their defaults and the
  ceiling of `max_depth`; how names and consts are read and written; the defaults of arguments that are left out; the
  form of an error, with its path, its consts and its position; and that the generated code depends on
  `constructo-runtime`, which a program adds to its own `Cargo.toml`. From decisions 51 to 54.
- **What is not supported, and why:** serde, Rust items for the consts, a tree with the names resolved
  (`read_tree`), a project that two runs share, which is generated in each, and the `Drop` of a deep value, which is the
  program's to manage.
- **The messages,** each quoted as the code gives it, so that the review of the specs can find it, **the settings,**
  and **the exit codes**, in a table, as `convert` has.

**`docs/specs/adopt-file-format.md`,** in the form of the other file formats: the tables, the keys, the paths and how
they are written, the three ways of adopting, the functions `from` and `to` with their arguments and results, the
default of an adopted type, that an adopted type is opaque to boxing and to the check of regularity, and the errors
with their messages and exit code 2. `codegen-command.md` says what `--adopt` does and links to it. **Decided,** as
below.

**Others:**

- `exit-codes.md` gains a section for `codegen`.
- `config-file.md` names `codegen` among the commands that read `[check]`.
- The "Related" sections of `check-command.md`, `convert-command.md`, `type-checking.md` (the section "What a number
  is", for the exception of `Float`) and `resolving-identifiers.md` (for the forms of a name that the codec reads) link
  to the new spec.

## The decisions

- **0056: a generated class may give a named parameter its default,** for a language that has default arguments, from
  the codec's own function, so that the two cannot differ. It is Python's, and is written now so that it is not lost,
  as the outline says. Rust has no default arguments, and has none.
- **0057: the name `codegen`,** with the other names that were thought of.
- **Decision 39 gets a note,** and is not reworded: a decision is a record of what was decided and why, and its title
  is a link from other documents. The note says that "driven by a type" describes how the reader is written, and that
  what the data may do is decision 52's "constrained by the target type". **Decided,** in place of the outline's
  rewording.

## The documents that a user reads first

- **`CHANGELOG.md`,** under "Unreleased": `constructo codegen --lang=rust`, with `--adopt`; the crate
  `constructo-runtime` and the conformance suite; and the refusals of the Rust target, so that a user who meets one
  knows that it is meant.
- **The README:** a paragraph on `codegen`, with the example of the scenario, and a sentence on the runtime crate.
- **`--help` of `codegen`:** that the file has the types and a codec, that it depends on `constructo-runtime`, and
  `--adopt`, with an example.

## Where the file of adoptions is specified

**Decided: option 1.** Every other file that `constructo` reads has a spec of its own, the
project-file, the lock file and the settings file, and the commands' specs link to it. Two ways:

1. **A spec of its own, `docs/specs/adopt-file-format.md`:** the tables, the keys, the paths, the three ways of
   adopting, the functions and their arguments, and the errors with their messages. `codegen-command.md` says what
   `--adopt` does and links to it. It is the form of the other files, and keeps the spec of the command to a length
   that can be read. **Chosen.**
2. **A section of `codegen-command.md`.** One document for everything about the command, but a long one, and the only
   file format without a spec of its own.

## How it is checked

- The review-specs check, `python3 -I .claude/skills/review-specs/check_specs.py`, on the new and changed specs, which
  finds broken links, paths that do not exist and lines that are too long.
- Every message that the specs quote is found in `src/` or the runtime with a search, and every example of a file or an
  output is produced by the binary.
- `just test`, since `--help` changes.

## The commits

1. The spec of the command, and of the file of adoptions, with the links from the other specs, `exit-codes.md` and
   `config-file.md`.
2. Decisions 56 and 57, and the note on decision 39.
3. The CHANGELOG, the README and `--help`.
4. The draft and the outline brought up to date.

## As it was built

*Done,* in the four commits above, with decisions 56 and 57. What the plan did not say:

- **The specs were checked against the code and the binary,** and two of the draft's claims were corrected: a type with
  no default is named without its arguments, as `Type List has no default value`, and the column of the example of an
  error is the one that the codec gives.
- **The codec's section on names is "Names in the text",** so that it has an anchor of its own beside the section on the
  names that Rust needs changed, which `resolving-identifiers.md` links to.
- **`exit-codes.md` no longer says that only `convert` gives warnings,** since `codegen` gives them too, and it says why
  a mistake in the file of adoptions that is only found against the run is exit code 2.
- **`codegen-command.md` links to decisions 56 and 57,** in its "Related" section and where it says that Rust has no
  default arguments.
- **The `--help` links to the spec,** as that of `check` links to the spec of the settings file.
- **`docs/specs/query-command.md:125` is longer than 120 characters,** from an earlier commit, and is left for the user
  to decide.

## Done when

- Every behaviour of `codegen` and of the code that it writes, which a user or a program can rely on, is in a spec.
- The review-specs check passes on the specs, and every quoted message is found in the code.
- `just test` passes.
