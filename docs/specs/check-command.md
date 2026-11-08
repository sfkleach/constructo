# The check command

`constructo check` reads data-types and data-values and reports what is wrong with them. This document says what it
reads, in what order it does things, what it prints and what its exit code means. The rules that decide whether a
declaration is valid are in [validity checks](validity-checks.md), and the rules that decide whether a value fits its
type are in [type checking](type-checking.md).

```text
constructo check [OPTIONS] [FILES]...
```

## Inputs

The inputs are, in this order:

1. The types files of the project named by `--project=FOLDER`, if there is one. See
   [the project-file](project-file-format.md).
2. The `FILES`, in the order that they are written.
3. The standard input, if `--stdin` is given. It is processed as one more file, named `<stdin>`, after all of the
   others.

A run needs at least one of `FILES`, `--stdin` and `--project`, unless `--print-config` is given. Without any, the
command line is refused as a usage error (exit code 2). The prelude is not an input of the user's. Unless
`--no-prelude` is given it is loaded ahead of everything else, as a project of its own, and it is never reported. See
[resolving identifiers](resolving-identifiers.md).

The settings of a run come from the command line, then from the file named by `--config`, and then from the defaults,
as [the configuration file](config-file.md) says. `--print-config` prints them and exits, without reading any input.

### Types files and values files

An input is a types file or a values file, and its first token decides which:

- A file whose first token is `enum` or `const` is a types file. A file with no tokens, which is empty or only has
  comments, is a types file too.
- Any other file is a values file.

The decision does not depend on the name of the file, and so it works for the standard input. It is made even when the
file does not parse, so that a values file that starts with a mistake gets the errors of the values grammar. A file
that a project lists as a types file but that holds data-values is an error at line 1, column 1, with the message `The
project includes this file as a types file but it holds data-values`.

Every types file must come before every values file. A types file that follows a values file is an error:
`A types file must come before the values files, but this follows FILE`, at line 1, column 1 of the types file, naming
the first values file. A types file that declares nothing may be anywhere. Because of this a values file always sees
all of the declarations of the run. The order of the types files among themselves does not matter, and neither does
the order of the declarations within a file. The syntax of both kinds of file is in
[the grammar](../../syntax/grammar.md).

## What a run does

The run is in stages. A stage that finds an error stops the later ones, as set out here.

1. **Reading and parsing.** The inputs are read and parsed one at a time, in order.
   - An input that cannot be read ends the run at once, with exit code 2 and the message
     `ERROR: Cannot read FILE: REASON`. Nothing after it is read.
   - The first syntax error ends the run, with exit code 1. It is the only diagnostic, and the inputs after it are
     neither read nor checked. So an unreadable input after a syntax error does not hide the error, whereas one before
     it does. The standard input is only read if no syntax error has been found.
   - There is no recovery from a syntax error, so a run shows one at a time.
2. **The order of the files.** A types file after a values file is reported, as above.
3. **Names.** The names in the types files are resolved to declarations. Those that cannot be are all reported, and
   so are the names that an import hides or renames but the project does not declare, and then the stages below do not
   run. See [resolving identifiers](resolving-identifiers.md).
4. **Validity.** The types files are checked together, as one namespace. Every error is reported. See
   [validity checks](validity-checks.md).
5. **Type checking.** If there has been no error so far, and `--no-type-check` is not given, the consts are checked
   and then the values. See [type checking](type-checking.md). Without this stage the values are only checked for being
   well-formed, which parsing has done. With it, the first type error ends the run, unless `--continue` is given, and
   then there is one error for each top-level value, and for each const.

Since an error in an earlier stage stops the later ones, a file can have problems that are not reported until the
first ones are corrected. Each stage reports all of its errors, apart from stage 1 and the type errors.

## Diagnostics

A diagnostic is one line on the standard error:

```text
ERROR: Duplicate constructor Dev (harbour.types, line 1, col 32); first declared at line 1, col 12
```

It is `ERROR: `, the message, and the file name and position in brackets. The line and column start at 1, and the column
counts characters. The name of the standard input is `<stdin>`, and any other file is shown by the path that it was
given by, or found by in a project. A name that comes from the prelude is shown as `constructo:/prelude/prelude.types`.

Some diagnostics end with a note after a semi-colon:

- `; first declared at line L, col C` for a duplicate in the same file, and `; first declared at FILE, line L, col C`
  when the first is in another file.
- `; in enum NAME` or `; in const NAME` for a syntax error, where the declaration being parsed is known. It is not
  known when the error is in the keyword or in the name itself.

The diagnostics of one run come in the order of the stages. The validity errors are in order of file, line and column.
The type errors come in the order in which the consts and then the values were checked.

There are no warnings. Everything that is reported is an error, and a number that is the wrong sort for its type, such
as `+8080` where a `Nat` is expected, is one of them.

## Verbose output

Nothing is printed on success unless `--verbose` (`-v`) is given. Then, after the diagnostics, these lines are written
to the standard output, one for each input that the user gave, in order:

| Line                                  | Meaning                                                           |
|---------------------------------------|-------------------------------------------------------------------|
| `OK: NAME (data-types)`               | A types file with no errors.                                      |
| `OK: NAME (data-values)`              | A values file that parses, and was not type-checked.              |
| `OK: NAME (data-values, type-checked)`| A values file that parses and fits the types.                     |
| `FAILED: NAME (KIND)`                 | A file that has an error, with its kind as above.                 |

A project line is `FAILED` if one of its files is, and also if its project-file has an error that is found when the
names are resolved, such as a name in `hide` that the imported project does not declare, even if all of its files pass.
A file that was not reached because the run stopped is not listed. The files of the prelude and of the other built-in
projects are never listed.

Under a types file that has consts, each const is listed with its type, if the values were type-checked, and a type
that was inferred is marked: `  const count: Nat (inferred)`. A generic const shows its parameters, as in
`const e<T>: [T] (inferred)`.

When the run has a project, a line for each project comes before the line of its first file, and the last line also
counts the projects:

```text
OK: syntax/examples/project/valid/shapes (project com.example.shapes)
OK: syntax/examples/project/valid/shapes/circles/shape.types (data-types)
OK: syntax/examples/project/valid/shapes/root.types (data-types)
OK: checked 2 file(s) in 1 project(s)
```

A project other than the root also says `verified` or `unverified`, as [the lock file](lock-file-format.md) says.
Without a project the last line is `OK: checked N file(s)`. It is only written if there were no errors. The standard
input counts as a file.

The output is the same with and without `--verbose`, apart from these lines.

## Exit codes

| Code | Meaning                                                                                       |
|------|-----------------------------------------------------------------------------------------------|
| 0    | There are no errors.                                                                          |
| 1    | The inputs have errors: a syntax error, a validity error, a type error, or a file out of order. |
| 2    | The invocation or the setup is wrong, so the inputs were not judged.                          |

Exit code 2 is for:

- an option or an argument that is not understood, which includes a value that is not allowed, such as an unknown
  experimental feature, and a limit that is not a whole number;
- `--config` given more than once, or a configuration file that cannot be read or has an error in it;
- an input that cannot be read, or a standard input that cannot be read;
- a problem with the project-file, the imports, the lock file or the cache, other than a project that does not match
  the lock. A project that does not match the lock is exit code 1, as it is an error in what is checked. See
  [the lock file](lock-file-format.md). So is a name in `hide` or `rename` that the imported project does not
  declare, which is found when the names are resolved.

This lets a script tell a bad schema or a bad value from a bad command. [The exit codes](exit-codes.md) lists every
problem and its code, for this command and for the others.

## Options

The options that are not settings are `--config`, `--cache`, `--stdin`, `--print-config`, `--project` and `--help`.
`--version` belongs to `constructo`, and not to `check`, which has no version of its own. The others are settings,
which `--print-config` writes and which [the configuration file](config-file.md) lists, with their defaults. In short:

| Option                               | Effect                                                                      |
|--------------------------------------|-----------------------------------------------------------------------------|
| `--prelude`, `--no-prelude`          | Load the prelude, or leave it out, and `Bool` and `Option` with it. |
| `--type-check`, `--no-type-check`    | Check values against types, or only for being well-formed.                  |
| `--widening`, `--no-widening`        | Let a number of a narrower type fit a wider one.                            |
| `--continue`, `--no-continue`        | After a type error, carry on with the next value, or stop.                  |
| `--allow-refs`, `--no-allow-refs`    | Let the definition of a const refer to a const.                             |
| `--infer-limit=N`, `--default-limit=N` | The limits on the size of an inferred type and of a default value.        |
| `-x`, `--experimental[=FEATURE]`, `--no-experimental[=FEATURE]` | Switch experimental features on or off. |
| `-v`, `--verbose`, `--no-verbose`    | Print the lines above.                                                      |
| `-d`, `--dev`, `--no-dev`            | Skip the check of folder projects against the lock.                         |

## Related

- [The configuration file](config-file.md), for the settings and how they layer.
- [Validity checks](validity-checks.md) and [type checking](type-checking.md), for the errors of stages 4 and 5.
- [Resolving identifiers](resolving-identifiers.md), for stage 3.
- [The exit codes](exit-codes.md), for the codes of every command.
- [The project-file](project-file-format.md) and [the lock file](lock-file-format.md), for `--project` and `--dev`.
- Decisions 1, 2, 6, 8, 9, 11, 12, 20 and 23, in `docs/decisions/`.
