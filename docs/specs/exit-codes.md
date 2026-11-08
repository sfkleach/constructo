# Exit codes

`constructo` ends with one of three exit codes. They let a script tell a bad schema or a bad value from a bad command
or a bad setup, which is why the second and third are kept apart. This document says what each one means, and which
problems give which. The messages themselves are described with the commands that give them.

| Code | Name   | Meaning                                                                                        |
|------|--------|------------------------------------------------------------------------------------------------|
| 0    | Ok     | The command did what was asked, and the inputs have no errors.                                 |
| 1    | Errors | The inputs were read and judged, and they have errors.                                         |
| 2    | Usage  | The command could not be run as asked, or something it needs was wrong, so nothing was judged. |

There are no other codes. There are no warnings, so a run that reports anything at all does not have exit code 0. A
diagnostic is a line that starts `ERROR:` on the standard error, and a run that exits with 1 or 2 has printed at least
one. A run with exit code 1 may print many.

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

`--help` and `--version` give 0, and so do the same options on a subcommand. Running `constructo` with no subcommand, or
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
- [The configuration file](config-file.md), [the project-file](project-file-format.md),
  [the lock file](lock-file-format.md), [the cache](cache-management.md) and
  [how git is run](git-management.md), for the errors of code 2.
