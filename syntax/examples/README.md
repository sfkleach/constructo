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
- `types/valid/postfix-optional.types` shows the postfix `?` on types, which is shorthand for `Option`.
- `values/experimental`: values that use the postfix `?` on values, which needs `--experimental=some-query` (or `-x`).
  `valid/` is only checked for being well-formed, and fails without the option. `typed/valid` and `typed/invalid` are
  checked against `values/typed/schema.types` in the same way as `values/typed`.
- `values/typed`: values that are checked against `values/typed/schema.types` (and the prelude). `valid/` must pass
  and `invalid/` must fail the type-check, though each invalid file is well-formed.
  Each invalid file has a `// expect: <message>` line, and the functional test requires that exact error.
- `types/valid/array-variants.types`, the `array-variant-*` examples and `values/valid/tagged-array.values` show the
  array variant `Users[User]` and its values `Users[u1, u2]`. `values/typed/valid/tagged-arrays.values` and the
  `*array-variant*` and `*-for-array-variant*` files in `values/typed/invalid` are checked against the schema.
- `project`: folders for `--project=FOLDER`. Each has a `constructo.toml`. `valid/` projects must check. `invalid/` ones
  must be rejected with exit code 2, and the first line of each `constructo.toml` is `# expect: <message>`, which the
  functional test requires the error to start with. `invalid/no-project-file` has no project-file at all.
  `rejected/` has a project whose patterns include a values file, which is exit code 1. `valid/shapes` has a hidden
  folder, a nested project, a values file and a text file, all of which must be ignored, because the types files in
  the first two clash with `circles/shape.types`. `valid/patterns` uses `include` and `exclude`, and
  `valid/imports-only` has no types files.
- `values/valid/qualified-names.values` and the `qualif*` examples show names that are qualified with a group or the
  full name of a project. They are only checked for being well-formed (`--no-type-check`), and the `invalid/` examples
  show a qualifier without a name, a dotted name with no `::`, and a qualified declaration.
- `config`: configuration files for `--config`, which are TOML and not Constructo. `valid/` must load, and `invalid/`
  must be rejected with exit code 2. Each invalid file has a `# expect: <message>` line, and the functional test
  requires that the error starts with exactly that. `precedence/` has the files that the functional tests combine with
  arguments to check that an explicit argument overrides the file.
