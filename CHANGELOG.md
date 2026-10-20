# Change Log for Constructo

Following the style in https://keepachangelog.com/en/1.0.0/

## Unreleased

### Added

- `constructo check` now type-checks data-values against the data-types that come before them on the command line
  (and the prelude). Constructors must exist and have the declared shape, records must have exactly the declared
  fields, and type parameters of generic enums are unified across a top-level value. Numbers widen from Nat to Int to
  Float, and `[1, 2.5]` has type `[Float]`. Untagged top-level values are checked for internal consistency. `null` is
  `None` of `Option<T>`. The first type error stops the run. See decisions 9 to 11.
- Options `--no-type-check` (only check that values are well-formed), `--no-widening` (numbers must have exactly the
  expected type) and `--continue` (carry on with the next value after a type error).
- In verbose mode a type-checked values file is reported as `OK: prod.values (data-values, type-checked)`.
- Library: the `typecheck` module, `CheckOptions` and `check_sources_with`; `Report` gains `type_checked`.
- `constructo check` now classifies each input by its first token as data-types (`enum`, or no tokens) or data-values,
  and checks that a data-values file is well-formed, reporting the file name and location of any syntax error.
- In verbose mode each input is reported with its kind, e.g. `OK: prod.values (data-values)`.
- Library: `parser::parse_values`, `parser::parse_file` and `SourceKind`; `Report` gains `kinds`.
- The parser rejects input nested more than 200 levels deep instead of overflowing the stack.
- `constructo check` now checks data-type declarations. It parses types files (stopping at the first syntax error)
  and then checks for duplicate types, constructors, fields and type parameters, names that clash with the basic
  types (`Nat`, `Int`, `Float`, `String`), unknown type names, generic arity, and unused type parameters.
- Type parameters may start with an underscore, e.g. `<_T>`, to mark them as intentionally unused.
- The standard prelude (`enum Bool = True, False;` and `enum Option<T> = None, Some(T);`) is loaded by default by
  `constructo check`; `--no-prelude` disables it. The library exposes it as `PRELUDE` and `prelude_source()`.
- `constructo check` is silent on success. `--verbose` (`-v`) reports each input as `OK: <name>` or
  `FAILED: <name>` (`<stdin>` for standard input, never the prelude) and ends with `OK: checked N file(s)` when
  everything passed.
- A syntax error now stops the run, and names the enclosing enum when it is known, e.g.
  `ERROR: Expected comma or semi-colon (f.types, line 3, col 1); in enum Shape`.
- Diagnostics have the form `ERROR: <message> (<file>, line L, col C)`. The exit code is 0 for success, 1 if the
  inputs contain errors and 2 for usage or I/O problems.
- Library modules for the lexer, parser, syntax tree and validation, with `check_sources` as the entry point.
- `tests/functest.sh`, driven by the examples in `syntax/examples/`.

### Fixed

- A values file followed by further arguments was mistaken for a syntax error, which stopped later files from being read.

### Changed

- Data values are separated by semi-colons, with an optional trailing one. This resolves the ambiguity of a
  constructor followed by a record or tuple. The values examples and grammar are updated.
- `docs/vision.md` diagnostics now include the file name.
