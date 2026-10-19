# Change Log for Constructo

Following the style in https://keepachangelog.com/en/1.0.0/

## Unreleased

### Added

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
- `tests/functest.sh`, driven by the examples in `syntax/examples/types/`.

### Fixed

### Changed

- `docs/vision.md` diagnostics now include the file name.
