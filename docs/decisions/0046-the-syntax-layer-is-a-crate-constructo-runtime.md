# 0046 - The syntax layer is a crate, constructo-runtime, 2026-11-24
> Amends [decision 43](0043-depth-limits-are-aligned-across-formats.md): every limit is shown in `src/limits.rs`,
> and the depth limit of the parser is defined in `constructo-runtime`.

## Issue

`constructo codegen` writes Rust code that reads and writes values of the declared types. That code needs a lexer, a
parser and a printer of data-values, and the limits that protect them. `constructo` has them already, in six modules
of its library: `lexer`, `parser`, `ast`, `values_text`, `limits` and `diagnostics`. The question was whether the
generated code should depend on `constructo`, on a copy of the syntax layer, or on a crate that holds it alone.

## Factors

- Generated code that reads the syntax with a parser other than `check`'s can disagree with `check` about what is
  valid. One parser for both makes them agree by construction.
- A program that uses the generated code should not have to build the whole of `constructo`, with its projects, its
  fetching of repositories, its formats and its command line, to read a value.
- The six modules use nothing outside themselves and no external crate, so they can be moved as they are.
- About 26 files of `constructo` use them by their paths, such as `crate::parser::parse_file`.
- `cargo publish` refuses a crate that depends on one that is not on crates.io.
- Decision 43 put every limit in `src/limits.rs`, so that they can be seen and changed together. The parser's limit
  has to be where the parser is.

## Decision

- **The syntax layer is a crate of its own, `constructo-runtime`** (`constructo_runtime` in code), a member of a
  workspace whose root is still the package `constructo`. It holds the six modules, and generated Rust code depends
  on it and on nothing else of `constructo`. The name is that of its job, to be what generated code calls, and
  follows the convention `<project>-<role>`.
- **`constructo` re-exports the modules under their old paths,** with `pub use constructo_runtime::parser;` and so on,
  so the files that use them did not change.
- **It keeps the parser of types files,** since the parser is one module, and cutting that part out is a change to the
  parser for no gain to generated code. It could be cut later.
- **It is published with `constructo`, at the same version, and before it.** `constructo` depends on it with
  `version = "=X.Y.Z"` as well as by path: a build from the repository uses the path, and the published crate the
  version. A release that publishes `constructo` alone is refused by `cargo publish`.
- **The depth limit of the parser, `MAX_DEPTH`, is defined in the runtime,** since the runtime reads data that is not
  trusted and has to protect its own stack. It is the only limit there. The others, of JSON, XML, `json-typed`,
  inference, defaults and queries, stay in `src/limits.rs`, which re-exports `MAX_DEPTH` and says where it is defined.
  So decision 43's rule that every limit is in `src/limits.rs` becomes that every limit is shown there.
- **The parser has a limit on nesting, and none on the size of its input.** It is linear in its input, so a limit on
  size would protect neither the stack nor the time, and decision 43 allows a limit for no other reason.

## Consequences

- `check` and generated Rust code read the same syntax, since they use the same code.
- The `Justfile` runs its tests, clippy and fmt over the workspace.
- From now on, a release has to publish `constructo-runtime` before `constructo`. Until the release recipes do that,
  it is done by hand; step 8 of the `codegen` task changes them.
- The runtime's public API, and the `ast` module in particular, become a promise once it is published, although they
  were written for `check`. They are looked at before the first release that publishes it. Two functions,
  `lexer::scan_string` and `values_text::bare`, had to be made public for `constructo` to keep using them.
- A target in another language cannot use this crate, and has a runtime of its own; see
  [decision 47](0047-every-runtime-is-held-to-the-syntax-by-a-conformance-suite.md).

## Options and Outcome

1. Generated code depends on `constructo`.
2. Generated code carries a copy of the parser in every generated file.
3. A crate of its own for the syntax layer, which `constructo` and generated code both use. **Chosen.**

## Pros and Cons of Options

### Option 1: depend on `constructo`

- Pros: no change to the build.
- Cons: a program that reads one value builds the whole tool, with its dependencies, and is tied to the API of a
  library that is not designed for it.

### Option 2: a copy of the parser in each generated file

- Pros: generated code has no dependency at all.
- Cons: every generated file is large, and each copy can drift from `check`'s parser, so a fix to the syntax has to be
  made by regenerating every program.

### Option 3: a crate for the syntax layer

- Pros: one parser for `check` and for generated code, a small dependency, and generated files that stay small.
- Cons: a workspace, and two crates to publish in order at every release.
