# Task: Configuration file

## Overview

It should be possible to specify all command-line options via a configuration
file using new `--config=PATH` option. The file is in TOML format, and its
settings are overridden by any explicitly supplied command-line argument.

This is also a prerequisite for the case-convention task, whose `--case=KIND:STYLE`
option is bulky enough to want a home in a file.

## Design points

- **Precedence.** Built-in defaults are overridden by the config file, which is
  overridden by explicit command-line arguments. The config file is only used
  when `--config=PATH` names it: there is no search for a file, and so no
  hidden state. A missing or unreadable file is an error.
- **Exactly one `--config`.** Repeating it is a usage error (exit code 2).
- **Options only.** The inputs (the files and `--stdin`) are never taken from
  the config file, and neither are `--config`, `--help` and `--version`. A list
  of inputs belongs in the project-file of the projects task.
- **TOML via the `toml` crate.** This is the first dependency apart from `clap`.
  The loading is in the library, as a new `config` module, so that it can be
  tested without the binary and so that a program embedding the library can read
  the same file. The binary does the layering of defaults, file and arguments.
- **Every problem is an error.** An unknown key or section, a value of the wrong
  type or out of range, a TOML syntax error, and an unknown experimental feature
  are all reported in the usual form `ERROR: <message> (<file>, line L, col C)`
  with exit code 2. An unknown key lists the keys that are known, as an unknown
  `-x` feature does. A typo can then never leave a setting at its default.
  The price is that renaming an option later needs a deprecation step.
- **Key names.** A key is the long form of the option without the leading `--`,
  with hyphens as on the command line: `allow-refs`, `infer-limit`. Where the
  option is `--no-FEATURE` the key is `FEATURE` with the opposite sense, so
  `--no-type-check` is `type-check = false`.

### Paired flags

The command line must be able to undo the file, so every boolean option has a
positive and a negative form, and the last one given wins:

| Key           | Switch on         | Switch off          | Default |
|---------------|-------------------|---------------------|---------|
| `verbose`     | `--verbose`, `-v` | `--no-verbose`      | false   |
| `prelude`     | `--prelude`       | `--no-prelude`      | true    |
| `type-check`  | `--type-check`    | `--no-type-check`   | true    |
| `widening`    | `--widening`      | `--no-widening`     | true    |
| `continue`    | `--continue`      | `--no-continue`     | false   |
| `allow-refs`  | `--allow-refs`    | `--no-allow-refs`   | false   |

`--infer-limit=N` and `--default-limit=N` take a number, and the file gives
them as a TOML integer. A negative number or a float is an error.

### Experimental features

The `[experimental]` table has one boolean for each feature, named as it is on
the command line. `-x` or `--experimental` switches features on and
`--no-experimental` switches them off. Each takes an optional value that must
be given with `=`, naming a feature, and with no value it means every feature.
For each feature the last of the arguments that mentions it wins over the file.
So with `some-query = true` in the file, `--no-experimental=some-query` turns it
off for one run, and `--no-experimental` turns off all of them.

### Sections

One file serves every subcommand. A subcommand reads the shared sections and its
own, and ignores the table of another subcommand, but a table that no subcommand
has is an error. So `constructo check` accepts a file with a `[lint]` section.
The list of sections is kept in one place.

- `[global]`: settings that every subcommand has, which today is `verbose`.
- `[experimental]`: the experimental features.
- `[check]`: the settings of `constructo check`.

Later tasks add their own tables. The case-convention task, for example, will
add a `[case]` table with one key for each kind of identifier, and an explicit
`--case=type:PascalCase` replaces the setting for just that key.

### The file

A typical configuration file might look like the below.

```toml
[global]
verbose = false

[check]
prelude = true
type-check = true
widening = true
continue = false
allow-refs = false

# Maximum number of nodes in the inferred type for a const.
infer-limit = 10000

# The most nodes that the default value of an omitted argument may have.
default-limit = 10000

[experimental]
some-query = false
```

Every key is optional, and a key that is left out keeps its default.

## Step 1: Paired flags

Give every boolean option its negative form, and add `--no-experimental`, with
the last argument winning. No config file is involved yet. For the clap
declaration, `overrides_with` makes each pair mutually exclusive in effect
without a usage error, and the existing `--no-FEATURE` options keep their
spelling. `--help` shows the defaults.

- Unit tests are not needed for clap itself, but the functional tests cover each
  pair in both orders (`--continue --no-continue` and the reverse).
- `-x` and `--no-experimental` interact feature by feature, as above.

## Step 2: Implement the `--config=PATH` option

- The `config` module in the library. `Config::parse(text, name)` returns the
  settings, every one optional, or the diagnostics in the standard form. Unknown
  keys are rejected with serde's `deny_unknown_fields`, with the error mapped to
  a line and column from the span the `toml` crate reports.
- `CheckOptions` can be built from a `Config`, and the binary layers the file
  over the defaults and the explicit arguments over the file. In clap that means
  telling apart a value that was given from one that was not, with
  `ArgMatches::value_source`.
- A relative path is relative to the current folder. Nothing else in the file is
  a path.
- Examples: valid config files, and invalid ones for each kind of problem, with
  a functional test that runs each and checks the message and the exit code 2.
  The functional tests also cover the precedence: that the file overrides a
  default and that an explicit argument overrides the file, for a flag, a number
  and an experimental feature.
- Write a decision record.

## Step 3: Implement `--print-config`

`constructo check --print-config` prints the settings that the run would use,
after the defaults, the file and the arguments have been combined, as a TOML
document that `--config` can load again. It exits 0 without checking anything,
and so it does not need any files. It prints every key of the sections that the
subcommand uses, and it is the starting point for writing a config file.

This also makes the precedence testable end to end: the functional tests compare
the printed document for each combination of default, file and argument.

## Step 4: Update the CHANGELOG.md

Ensure the CHANGELOG.md is up to date with this feature, including the paired
flags and the new dependency.
