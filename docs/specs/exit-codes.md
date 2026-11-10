# Exit codes

`constructo` ends with one of three exit codes. They let a script tell a bad schema or a bad value from a bad command
or a bad setup, which is why the second and third are kept apart. This document says what each one means, and which
problems give which. The messages themselves are described with the commands that give them.

| Code | Name   | Meaning                                                                                        |
|------|--------|------------------------------------------------------------------------------------------------|
| 0    | Ok     | The command did what was asked, and the inputs have no errors.                                 |
| 1    | Errors | The inputs were read and judged, and they have errors.                                         |
| 2    | Usage  | The command could not be run as asked, or something it needs was wrong, so nothing was judged. |

There are no other codes. A diagnostic is a line on the standard error that starts `ERROR:` or `WARNING:`. An error
is what makes a run exit with 1 or 2, and such a run has printed at least one. A run with exit code 1 may print many. A
warning says that the inputs were used but something was left out, and does not change the exit code, so a run that
only warns exits with 0. Only `constructo convert` gives warnings so far, and `constructo check` gives none, so a run of
it that reports anything at all does not have exit code 0.

The test that separates 1 from 2 is whether the thing that is wrong is **in the data that is being checked**. A syntax
error, a duplicate name and a value of the wrong type are in the data, and are 1. A file that cannot be read, an
option that is not understood and a project-file that is malformed are not about the data but about how it is found,
and are 2. The two kinds are told apart because a person fixes them in different places.

## `constructo check`

### Exit code 0

- The inputs have no errors. With `--verbose` the `OK:` lines are written to the standard output as well.
- `--print-config`, which prints the settings and exits without reading any input.
- `--help`.

### Exit code 1

The inputs have an error of one of these kinds. See [the check command](check-command.md) for the order in which they
are found.

- A syntax error in a types file or a values file. The run stops at the first one.
- A types file after a values file.
- A file that a project lists as a types file but that holds data-values.
- A name that cannot be resolved, a name that two projects of one group declare, and a name in `hide` or `rename` that
  the imported project does not declare. See [resolving identifiers](resolving-identifiers.md) and
  [the project-file](project-file-format.md).
- A validity error, such as a duplicate or an unknown type. See [validity checks](validity-checks.md).
- A type error in a const or in a value. See [type checking](type-checking.md).
- A project that does not match [the lock file](lock-file-format.md): a file whose digest differs, a project or file
  that is not in the lock or is only in the lock, or a project that comes from somewhere else now.
- A project from a repository for which the lock has no entry, or no commit that matches, or whose commit is not in the
  cache, or when there is no lock at all. These are reported at the import and say to run `constructo project update`.
  The last three are about what the lock and the cache hold and not about how they are used, and so they count with the
  lock mismatches above.

### Exit code 2

- An option or an argument that is not understood, which includes a value that is not allowed (an unknown experimental
  feature, a limit that is not a whole number), a missing `--project`, file or `--stdin`, and no subcommand at all.
  These are reported by the argument parser, which says how to use the command.
- `--config` given more than once, and a configuration file that cannot be read or that has an error in it. See
  [the configuration file](config-file.md).
- A `--cache` or `cache` setting that is empty, and a cache folder that is not valid. See
  [the cache](cache-management.md).
- An input that cannot be read, including the standard input. Nothing after it is read, so a syntax error that comes
  later is not found.
- A problem with the project-file or with how projects are found: a key that is not known or a value that is not valid,
  a name that does not match, a folder that is not there, a cycle of imports, two projects with one name, a link in a
  repository that leads out of it, a bad name in `hide` or `rename`, and the problems with git or the URL. See
  [the project-file](project-file-format.md) and [how git is run](git-management.md).
- A lock file that cannot be read or does not parse, and a project file that cannot be read while its digest is being
  worked out.

## `constructo convert`

[The convert command](convert-command.md) judges the inputs, so what gives 1 is the inputs, and 2 is the command and the
setup. For the typed formats (`json-typed` and `jsonl-typed`) it judges the types and the values as `check` does, and
the same errors have the same codes.

- **Exit code 0:** the values were converted, or with `--from` read back. The warning that more values follow, and
  `--help`, are 0. A lines format with no values to convert is 0 as well, and writes nothing.
- **Exit code 1:** a syntax error in an input, no value to convert for a format that writes one document (without
  `--to-array`), and a name that is given twice in a record or in the named arguments of a tuple. For the exact formats,
  an input that is a types file. For the typed formats, the errors of `check` in the types and in the values that are
  converted (the type errors, the validity errors, names that cannot be resolved, a project that does not match its
  lock), and a value that cannot be written as JSON: `Some(None)`, or one nested too deeply or too large. The run stops at the first
  error in a value. A format that writes one document writes nothing to the standard output when there is an error. A
  lines format writes the lines before an error in a value, and nothing if the error is found before the first value is
  converted, as an error in the types or in the syntax is.
- **With `--from`, exit code 1** is JSON that is not valid, or is valid and is not what the format says, including an
  empty document, and a number or a name that is not what its key says. For the typed formats it is also an error in
  the types or in `--type`, JSON that does not fit the type, and a JSON file among the types files before a `--`. The
  values before the problem have been written, as for a lines format, except that a JSON document is read whole first,
  so an error in its syntax writes nothing, and that an error in the types or in `--type` writes nothing.
- **Exit code 2:** an option or an argument that is not understood, which includes neither `--to` nor `--from` or both
  of them, a `--to` or `--from` that is not a format, `--to-array` with a lines format or with `--from`, `-x` with
  `--from`, an option for types (`--project`, `--no-prelude` and the others) with a
  format that needs no types, `--type` missing with a typed `--from` or given with another format, and no inputs at all
  or no JSON among them; `--config` given more than once or one that cannot be read, a
  `--cache` that is empty, an input that cannot be read, including the standard input, and an output that cannot be
  written, such as a pipe whose reader has gone. For the typed formats, the problems with a project-file or with how
  projects are found, as for `check`. As for `check`, an unreadable input ends the run at once, and a syntax error in an
  input that comes before it is found first.

## `constructo project`

- `constructo project update` exits with 0 when it has written the lock or found it up to date, and with 2 for any
  error. It judges nothing about the types, so there is no case for 1. A repository that cannot be fetched, a `rev` that
  does not exist, a missing `git`, a cache folder that was changed and a project that cannot be loaded are all 2. See
  [the lock file](lock-file-format.md).
- `constructo project init` exits with 0 when it has written `constructo.toml` and also when the folder has one already,
  which is left as it is, and with 2 if the file cannot be written or the folder is not there. See
  [the project-file](project-file-format.md).
- `constructo project` with no subcommand is a usage error, 2.

## `constructo` itself

`constructo --version` and `constructo version` print the version, and give 0. The version is only at the top: it is not
an option of a subcommand, and `constructo check --version` is an argument that is not understood, so it gives 2.
`--help` is an option of the command and of every subcommand, and gives 0. Running `constructo` with no subcommand, or
with one that does not exist, is 2.

## Choosing between 1 and 2

When a problem is added, it is given its code by asking what a person has to change to put it right:

1. If it is **the inputs**, a types file, a values file or the lock that is checked against them, it is 1. A hide of a
   name that the imported project does not declare is in `constructo.toml`, but it can only be found once the types
   files are parsed and their declarations are known, so it is reported with the errors of the types files, and is 1.
2. If it is **the command, a setting, the file system or the setup of the project**, such as an option, a path, a
   cache, a lock that cannot be read or a project-file that does not parse, it is 2.

The setup is dealt with before the files are checked. So a bad option or a bad project-file gives 2 and the files are
not checked at all. Within the files, an input that cannot be read ends the run at once with 2, whereas a syntax error
in an earlier input ends it with 1, and an unreadable input after it is never reached:

```text
constructo check bad-syntax.types missing.types   # exit code 1
constructo check missing.types bad-syntax.types   # exit code 2
```

## Related

- [The check command](check-command.md), for the stages of a run and where the errors of code 1 come from.
- [The convert command](convert-command.md), for the warning and the errors of `convert`.
- [The configuration file](config-file.md), [the project-file](project-file-format.md),
  [the lock file](lock-file-format.md), [the cache](cache-management.md) and
  [how git is run](git-management.md), for the errors of code 2.
