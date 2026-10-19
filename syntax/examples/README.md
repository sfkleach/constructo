# Syntax examples

- `types/valid`, `values/valid`: must parse.
- `types/invalid`, `values/invalid`: must be rejected by the parser.
- `types/parses-but-invalid`: grammatical, but must be rejected by the
  post-parse validity check (duplicates, unknown names, wrong arity). The examples
  are checked with the standard prelude loaded, which declares `Bool` and `Option`;
  `redeclares-bool.types` is only valid under `--no-prelude`.

Type-correctness of values against types (e.g. a bare `Quit` where the schema
declares `Quit()`) is a separate check, not covered by these files.
