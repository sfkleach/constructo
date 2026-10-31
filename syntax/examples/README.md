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
- `project/valid-imports` and `project/invalid-imports`: projects with `[[import]]`. In each case the project to check
  is the `app` folder or the folder named in the functional test, and the others are its imports.
  `valid-imports/diamond` imports one project through two others, `two-groups` has one project in several groups, and
  `chain` shows that a project is loaded when it is imported indirectly. Each `invalid-imports/CASE/app/constructo.toml`
  has a first line `# expect: <message>`, and the functional test requires that the error starts with it, with exit
  code 2. A cycle, a project name that does not match, two projects with one name, a duplicate import and every kind of
  bad key are there.
- `project/valid-names` and `project/invalid-names`: projects whose names refer to other projects. `valid-names`
  shows a group, a full name, an import into `local`, two projects with the same names, a group of two projects, and a
  project that is not imported by the one that passes on its const. Each `invalid-names/CASE/app/app.types` has a first
  line `// expect: <message>`, and the functional test requires that the error starts with it, with exit code 1. The
  cases are an ambiguous name, a name in a group, a project or group that is not known, a project that is not imported,
  the prelude, and a message that shows a name in full.
- `project/valid-prelude`: projects that override the prelude, which is the built-in project `constructo.prelude` that
  every project imports into `local` unless it imports it itself. `override` imports it into the group `std`,
  `explicit-local` into `local`, `no-group` with no group, and `full-name` uses the default import.
  `invalid-names/prelude-clash` and `override-needs-option` show a project that declares a name of the prelude and one
  that overrides it and then uses the question mark, and `invalid-imports/builtin-*` and `unknown-builtin*` show the
  errors for a built-in project.
- `values/typed/valid/qualified-names.values` and the `unknown-*` examples beside it use `local::`, which is the only
  group that files without a project have.
- The `qualif*` examples in `types/invalid` and `values/invalid` show a qualifier without a name, a dotted name with no
  `::`, and a qualified declaration, which are syntax errors.
- `config`: configuration files for `--config`, which are TOML and not Constructo. `valid/` must load, and `invalid/`
  must be rejected with exit code 2. Each invalid file has a `# expect: <message>` line, and the functional test
  requires that the error starts with exactly that. `precedence/` has the files that the functional tests combine with
  arguments to check that an explicit argument overrides the file.
