# The configuration file

A configuration file gives the settings that the command-line options would. It is read when it is named with
`--config=PATH`. There is no search for a file, so a run depends only on its command line and the file it names.

## Writing a file

`constructo check --print-config` prints the settings that the run would use, with a comment on each key, and exits
without checking anything. It needs no input files. So a template that has every setting is made with:

```sh
constructo check --print-config > constructo-settings.toml
```

The output is itself a valid configuration file. Options given on the same command line are part of what is printed,
so `constructo check --print-config --no-widening` writes a file that has `widening = false`, and `--config=PATH` as
well starts from that file. A key that is the default may be deleted from the file with no change in meaning.

## Layering

Each setting comes from the first of these that gives it:

1. An option on the command line.
2. The configuration file.
3. The built-in default.

A boolean option has a positive and a negative form, such as `--continue` and `--no-continue`, and when both are
given the last one wins. So any setting in a file can be undone for one run. Likewise `-x` (`--experimental`) and
`--no-experimental` are applied in order, feature by feature, and either can name one feature with `=FEATURE`.

## Format

The file is in [TOML](https://toml.io). Every key is optional, and a key that is left out keeps its default.
The keys are spelt as the long form of the option, without the leading `--` and with hyphens. Where the option is
`--no-FEATURE` the key is `FEATURE`, with the opposite sense: `--no-type-check` is `type-check = false`.

A file serves every subcommand. A subcommand reads the shared tables and its own, and ignores the table of another
subcommand. Today there is one subcommand, `check`.

### `[global]`

| Key       | Type    | Default | Option                      |
|-----------|---------|---------|-----------------------------|
| `verbose` | boolean | `false` | `--verbose`, `--no-verbose` |

### `[experimental]`

| Key          | Type    | Default | Option                                          |
|--------------|---------|---------|-------------------------------------------------|
| `some-query` | boolean | `false` | `-x=some-query`, `--no-experimental=some-query` |

`some-query` allows a postfix `?` on a value, as in `0?`, which is shorthand for `Some(0)`.

### `[check]`

| Key             | Type    | Default | Option                                  |
|-----------------|---------|---------|-----------------------------------------|
| `prelude`       | boolean | `true`  | `--prelude`, `--no-prelude`             |
| `type-check`    | boolean | `true`  | `--type-check`, `--no-type-check`       |
| `widening`      | boolean | `true`  | `--widening`, `--no-widening`           |
| `continue`      | boolean | `false` | `--continue`, `--no-continue`           |
| `allow-refs`    | boolean | `false` | `--allow-refs`, `--no-allow-refs`       |
| `infer-limit`   | integer | `10000` | `--infer-limit=N`                       |
| `default-limit` | integer | `10000` | `--default-limit=N`                     |

A limit is a whole number that is not negative.

## What is not in the file

This file is not the project-file. A project is described by `constructo.toml` in the project folder, which `--project`
reads (see decision 24), and it holds no settings. The settings file is only ever read when `--config` names it, and
`--print-config` writes one, conventionally named `constructo-settings.toml`.

The inputs (the files to check and `--stdin`) are never taken from the file, and nor are `--config`, `--print-config`,
`--help` and `--version`. `--config` can be given once in all, whether it is before or after the subcommand.

## Errors

Every problem with the file is an error, so a misspelt key can never leave a setting at its default. It is reported in
the usual form, `ERROR: <message> (<file>, line L, col C)`, and the exit code is 2. That covers:

- a key or a table that is not known, where the message lists the ones that are;
- a value of the wrong type, or out of range, such as a negative limit;
- an error in the TOML itself, such as a repeated key.

A file that cannot be read is reported as `ERROR: Cannot read PATH: <reason>`, also with exit code 2.

## Examples

`syntax/examples/config/` has valid files, invalid files with the expected message on the first line of each, and
files that the functional tests use to check the layering. The decision is recorded in
[decision 21](../decisions/0021-settings-can-be-read-from-a-toml-configuration-file.md).
