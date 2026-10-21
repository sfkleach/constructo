# Syntax examples

- `types/valid`, `values/valid`: must parse.
- `types/invalid`, `values/invalid`: must be rejected by the parser.
- A file is a types file if its first token is `enum` or `const` (or it has no tokens), and a values file otherwise.
  Values are separated by semi-colons.
- `types/parses-but-invalid`: grammatical, but must be rejected by the
  post-parse validity check (duplicates, unknown names, wrong arity). The examples
  are checked with the standard prelude loaded, which declares `Bool` and `Option`;
  `redeclares-bool.types` is only valid under `--no-prelude`.

- `types/valid/record-consts.types` and the `record-*` examples show the type of an untagged record.
- `types/valid/generic-consts.types` also shows consts that refer to `null`, `true` and `false` of the prelude.
- `types/valid/inferred-consts.types` has consts without a type. `types/refs/inferred-*.types` have inferred consts that
  refer to one another.
- `types/refs`: consts that refer to other consts. `chain.types` is valid with `--allow-refs` and invalid without it.
  `cycle.types` is invalid with or without it.
- `values/valid`, `values/invalid` are only checked for being well-formed (`--no-type-check`).
- `values/typed`: values that are checked against `values/typed/schema.types` (and the prelude). `valid/` must pass
  and `invalid/` must fail the type-check, though each invalid file is well-formed.
  Each invalid file has a `// expect: <message>` line, and the functional test requires that exact error.
