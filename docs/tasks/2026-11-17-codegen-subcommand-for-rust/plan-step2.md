# Plan: step 2 of codegen, the runtime crate and the conformance suite

This plans step 2 of the [implementation plan](implementation-plan.md): the syntax layer moves into a crate of its own,
`constructo-runtime`, which the generated Rust code will depend on, and a conformance suite is written that every
runtime is held to. Nothing is generated yet, and no behaviour of `constructo` changes.

## What the code has today

- One crate, `constructo`, with a library (`src/lib.rs`) and a binary (`src/main.rs`). There is no workspace.
  `.cargo/config.toml` puts the target in `_build/target`.
- The six modules of the syntax layer, with their lines and tests:

  | Module        | Lines | Tests | Uses, within the crate                           |
  |---------------|-------|-------|--------------------------------------------------|
  | `diagnostics` | 164   | 5     | nothing                                          |
  | `limits`      | 55    | 0     | nothing                                          |
  | `lexer`       | 416   | 9     | `diagnostics`                                    |
  | `ast`         | 174   | 0     | `diagnostics`                                    |
  | `parser`      | 1383  | 49    | `ast`, `diagnostics`, `lexer`, `limits`          |
  | `values_text` | 404   | 8     | `ast`; in its tests, `diagnostics` and `parser`  |

  None of them uses an external crate, or a module outside the six.
- `limits.rs` holds every limit, as decision 43 requires, but the parser uses only `MAX_DEPTH`. The rest are for
  JSON, XML, `json-typed`, inference, defaults and queries. `MAX_JSON_DEPTH` and `MAX_XML_DEPTH` are derived from
  `MAX_DEPTH`.
- The parser has no limit on the size of its input, only on depth. The draft's "nesting and size" is not what the code
  has.
- One test reads files by a path from the crate: `values_text::what_is_written_is_read_back` joins
  `env!("CARGO_MANIFEST_DIR")` with `syntax/examples/values/valid`.
- The `Justfile` runs `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`, which in a
  workspace whose root is a package see only that package.
- The `Justfile`'s release recipes end in `cargo publish` of `constructo`, by a workflow that is not in the repository
  yet.

## The publishing of the runtime

**Nothing is published in this step;** step 8 of the outline publishes it. But the step does change what the next
release of `constructo` has to do: `cargo publish` refuses a crate that depends on another by path alone, since every
dependency has to be on crates.io. So once `constructo` depends on `constructo-runtime`, `constructo` cannot be
published unless the runtime is published first.

So this step only makes it publishable, and leaves the rest to step 8:

- **The runtime has the same version as `constructo`,** and the dependency is written
  `constructo-runtime = { version = "=0.1.0", path = "constructo-runtime" }`. A build from the repository uses the
  path, and the version is there for the day when it is published.
- **The runtime has `description`, `license` and `repository`,** as `constructo` has, since crates.io requires them.
- **The `Justfile` gets a note** beside the release recipes, that a release now has to publish `constructo-runtime`
  before `constructo`, until step 8 changes the recipes to do it.

The other open question of the draft's section, whether the types parser stays in the runtime, is settled as the
outline says: it stays, since cutting it is a change to the parser for no gain here.

## The workspace and the move

- **The root `Cargo.toml`** gains `[workspace]` with `members = ["constructo-runtime"]` and `resolver = "2"`, and the
  dependency above. The root stays the package `constructo`, so `cargo install --path .` and the binary's path do not
  change.
- **The new crate**, `constructo-runtime/`, has a `Cargo.toml` with no dependencies, and `src/lib.rs` with a doc
  comment that says what the crate is for and that its API is that of `constructo`'s syntax layer, and the six
  modules, each `pub mod`.
- **The modules are moved with `git mv`,** so that their history follows them, and the move is a commit of its own,
  with no other change, so that it can be reviewed as a move.
- **`constructo`'s `src/lib.rs`** replaces `pub mod ast;` and the others with `pub use constructo_runtime::ast;` and so
  on. `crate::parser::parse_file` and the other paths in the remaining 26 files then resolve as before, and those files
  do not change. That is checked by the build, and by `git diff --stat` showing no change to them.
- **The test that reads the examples** joins `../syntax/examples/values/valid` to the runtime's manifest folder. The
  conformance suite below replaces it in time, and it is kept until then.

## The limits

- The runtime's `limits.rs` has `MAX_DEPTH` alone, with its comment, and a module comment that says why the runtime
  has a limit at all: it reads data that is not trusted.
- `constructo`'s `src/limits.rs` keeps the rest, re-exports `MAX_DEPTH` from the runtime with `pub use`, and its module
  comment says that the depth limit of the parser is defined in the runtime and shown here, so that the limits can
  still be seen together. `crate::limits::MAX_DEPTH` keeps working, and `MAX_JSON_DEPTH` and `MAX_XML_DEPTH` are still
  derived from it.
- The draft's "nesting and size" is corrected to nesting. A limit on the size of an input is not added here: decision
  43 allows a limit only against a stack that grows or work that is excessive for the input, and a parser that is
  linear in its input does neither. A codec's limit on the size of what it reads, if any, is for step 5.

## The conformance suite

- **Where:** `syntax/conformance/`, beside `syntax/examples`, since it is about the syntax and not about `constructo`.
  It has a `README.md` that says what a runtime has to do to conform.
- **A case** is a values file, `NAME.values`, and its expected result, `NAME.expected`, which is one of:
  - the values that the file holds, each written by `values_text::render` with `compact` false, one after the other.
    That is the tree written back, so it keeps the text of each number and name as it was written: the suite tests the
    syntax and not the model of numbers;
  - or the one line of the error that the parser reports, as `Diagnostic` writes it, with the file named as
    `NAME.values`: `ERROR: Nesting is too deep (deep-201.values, line 1, col 201)`. A runtime conforms if it gives
    the same message at the same line and column.
- **The cases at first:**
  - every file of `syntax/examples/values/valid` and `syntax/examples/values/invalid`, copied, so that the suite is
    whole in itself, and another runtime, which may be in another repository, can take the folder as it is;
  - the experimental values of `syntax/examples/values/experimental/valid`, in a folder `experimental/`, which a runtime
    reads with the experimental features on, or skips if it has none;
  - the depth limit: a value nested 200 levels deep, which is accepted, and one 201 levels deep, which is refused,
    for each way that a value can hold another: array, record, tuple, tagged record, tagged array and postfix `?`;
  - the cases of the lexer that a hand-written runtime most often gets wrong: escapes, `\uXXXX` with surrogate pairs
    and lone surrogates, signs and exponents, `1e5` and `1.` refused, comments, and a value that ends at the end of
    the file without `;`.
- **The runner** is an integration test of the runtime, `constructo-runtime/tests/conformance.rs`. It reads every case
  under `../syntax/conformance`, parses it with `parse_values_with`, and compares the rendered values or the error with
  the expected file. With `CONSTRUCTO_BLESS=1` it writes the expected files instead, which is how they are made, and how
  they are changed when the syntax is; a change to them shows in review.
- **The suite is checked against the examples** by the runner too: a file of `syntax/examples/values` that has no case
  of the same name in the suite is a failure, so the two cannot drift apart.

## The commands

- The `Justfile` runs `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all --check`, and `fmt` becomes `cargo fmt --all`.
- `tests/functest.sh` does not change, since the binary does not.
- `CLAUDE.md` and the README are checked for commands that would now miss the runtime.

## Decisions

- **0046: the syntax layer is a crate, `constructo-runtime`**, which generated code depends on: what it holds, why it
  keeps the types parser, that it is published with `constructo` at the same version, and that the depth limit is
  defined in it. It
  amends decision 43, whose rule that every limit is in `src/limits.rs` becomes that every limit is shown there.
- **0047: every runtime is held to the syntax by a conformance suite**, whose expected results are Constructo text
  and the messages of errors, made from the Rust runtime, and that every runtime makes the tree of a value public.

## The commits

1. The workspace and the move, with the re-exports and the path of the one test. Everything passes, unchanged.
2. The split of the limits.
3. The `Justfile`: the workspace in its commands, and the note on releases.
4. The conformance suite, its runner and its cases.
5. The decisions, and the draft and the outline brought up to date: a release has to publish the runtime first, and
   the parser has a limit on nesting and not on size.

## Done when

- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` and
  `bash tests/functest.sh _build/target/debug/constructo` pass, with no test changed but the path of the one.
- The 26 files that use the six modules are unchanged.
- `cargo package -p constructo-runtime --list` shows a crate that could be published, with no file outside it,
  although it is not published.
- The suite passes, and has a case for every example.
