# 0021 - Settings can be read from a TOML configuration file, 2026-10-26

## Issue

**Fork.** The options of `constructo check` are many, and some of them, such as the planned `--case=KIND:STYLE`, are
bulky enough that nobody wants to type them on every run. A file should be able to give the same settings, which
raises the questions of its format, where it is found, how it combines with the command line, how strict it is, and
where the code that reads it lives.

## Decision

- The file is TOML and it is read with the `toml` crate (with `serde`), the first dependencies apart from `clap`.
- It is used only when `--config=PATH` names it. There is no search for a file, so that a run is determined by its
  command line and its files. `--config` can be given once, either before or after the subcommand. A file that cannot
  be read is an error.
- Built-in defaults are overridden by the file, which is overridden by any argument given on the command line.
  Every boolean option has a positive and a negative form, with the last one given winning, so that the command
  line can undo the file. The experimental features are switched on by `-x` and off by `--no-experimental`, one at a
  time, in the order the arguments are written.
- The file holds options only: not the inputs, and not `--config`, `--stdin`, `--help` or `--version`.
- The keys are the long options without the leading `--` and with their hyphens, as in `allow-refs`. Where the option
  is `--no-FEATURE` the key is `FEATURE`, so `--no-type-check` is `type-check = false`. Every key is optional.
- The tables are `[global]` (today only `verbose`), `[experimental]` (one boolean for each feature, named as on the
  command line) and `[check]`. One file serves every subcommand, so each subcommand reads the shared tables and its
  own. A table that no subcommand has is an error.
- Every problem is an error, reported in the usual form `ERROR: <message> (<file>, line L, col C)` with exit code 2:
  an unknown key or table, a value of the wrong type or a number that is not a non-negative integer, and a syntax error
  in the TOML. The message for an unknown key lists the keys that are known. A typo can then not leave a setting at
  its default.
- The reading is in the library, as `Config` in the `config` module, with `Config::parse(text, name)` and
  `Config::check_options()`, so that it is tested without the binary and an embedding program can read the same file.
  The binary layers the command line over it. Only the first problem in a file is reported, since the TOML parser
  stops at the first.

## Consequences

- Renaming or removing an option breaks a config file that uses it, so it needs a deprecation step.
- The library crate now depends on `toml` and `serde`.
- The tables of later subcommands, and a `[case]` table for the case-convention task, are added as fields of `Config`.
- The effective settings can be seen with `--print-config` (the next step of the task).

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A hand-written parser for a subset of TOML: no dependency, but a second parser to maintain and a file that is only
  TOML-like.
- A Constructo values file checked against a built-in schema: no dependency, but an unfamiliar format for a config.
- A file that is found by searching the current folder and the user's configuration folder: convenient, but a stray
  file changes behaviour silently, and it overlaps with the project-file of the projects task.
- Command-line options that take `=false`, instead of negative flags: it would change how the existing `--no-`
  options read.
- Warning on unknown keys, or ignoring them: so that a file works with several versions, but typos are invisible.
