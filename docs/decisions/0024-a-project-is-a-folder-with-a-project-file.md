# 0024 - A project is a folder with a project-file, 2026-10-31

## Issue

**Fork.** Every run of `constructo check` was given its inputs on the command line, and all of them shared one
namespace. That does not scale to data-types that are shared between teams. A project is a folder that holds types
files and a project-file that describes it. This is the first step: a single project, with no imports yet. We have to
fix
the format of the project-file, how its files are found, and how it relates to the settings file.

## Decision

- The project-file is `constructo.toml` in the root of the project folder. It is not the settings file of `--config`
  (decision 21), which is a separate file that is read only when it is named. So the project-file holds no settings, and
  `--print-config` now suggests the name `constructo-settings.toml`.
- The file has a `[project]` table with `name`, which is required, and `include` and `exclude`, which are lists of glob
  patterns and default to `["**/*.types"]` and `[]`. A name is one or more dot-separated parts, each starting with a
  letter and holding letters, digits, `_` and `-`.
- The file is strict. An unknown key or table, a missing name, a bad name, a value of the wrong type and a pattern that
  is not valid are errors with a line and a column in the file, and exit code 2, as for the configuration file. So are a
  folder or project-file that cannot be read.
- A file belongs to the project if its path within the folder, written with `/`, matches an `include` and no `exclude`.
  The `glob` crate does the matching, with `*` not crossing a folder, and `**/` matching any number of folders including
  none. The walk skips hidden folders and any subfolder with a `constructo.toml` of its own, which is another project,
  and does not follow a symbolic link to a folder. The files are sorted by path. A project in which nothing matches is
  not an error, since a project may consist only of imports.
- `constructo check --project=FOLDER FILES...` checks the prelude, the types files of the project, and then `FILES` and
  `--stdin` as given. The ordering rule of decision 23 applies to them all, so a types file may follow on the command
  line but not after a values file. Diagnostics name files as the user would type the path.
- A matched file that holds data-values is an error naming the file, with exit code 1.
- `--project` is given once and needs its value. With `--verbose` the project is reported once, before its files, as
  `OK: FOLDER (project NAME)`, or `FAILED:` if one of its files failed.

## Consequences

- The prelude is still added by the command ahead of the project's files, which is a stopgap. It is to become a virtual
  project in the group `local` that every project includes unless it overrides it. Until then a project that declares
  its own `Bool` or `Option` gets the duplicate error, and `--no-prelude` is the way round it. `Project` returns only
  the project's own files so that this goes away without changing it.
- The settings file and the project-file are two files, so a project cannot carry its own settings yet. The local
  configuration file of the task is for later, and needs a name and a rule for finding it.
- A new dependency, `glob`, which has no dependencies of its own.
- Checksums, imports and groups are not there yet. Nothing in the format has to change for them, since a new key or
  table is an addition.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- One file for both the project and the settings. It is simpler for a user, but then `--config` is no longer the only
  source of settings, and the rule that nothing is read implicitly needs an exception.
- A hand-written matcher for `**` and `*` only. It avoids a dependency, but the patterns are then a dialect of our own
  to document and test.
- A single `types` pattern in place of `include` and `exclude`. Excluding a file then needs a cleverer pattern.
- Taking every `.types` file with no patterns. That is the default already, and the patterns are cheap to add now.
