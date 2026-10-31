# 0026 - The prelude is a built-in project, 2026-11-01

## Issue

**Fork.** The standard prelude was added to every run by the command, ahead of the user's files, and had its own
rules: it was exempt from `--allow-refs`, its names were duplicates if redeclared, and nothing could override it. With
projects and imports (decisions 24 and 25) it can be what it always was in effect, a project that every other project
imports. We have to decide how it is named and found, how a project replaces it, and what the flags do.

## Decision

- The prelude is the project `constructo.prelude`, held inside the binary. The `constructo.` prefix is reserved for
  built-in projects: a project-file cannot name its project that way, and an import whose `project-name` starts with it,
  or whose `source` starts with `constructo:`, has to name a built-in project by both, or it is an error that lists the
  built-in projects.
- A built-in project is a project like any other, with a project-file and types files. Its source is a URI that names
  the project as a folder would, `constructo:/prelude`, and the loader has a table from the URI to the text of its
  files. Other built-in projects can be added with no new rules. Its types file is shown in diagnostics as
  `constructo:/prelude/prelude.types`, where `<prelude>` was.
- Every project is given an import of the prelude into the group `local`, as its first import, unless it has an
  `[[import]]` of `constructo.prelude` of its own. That import is the project saying what it wants, and the way to
  override the prelude is to import it into another group, such as `std`: bare `Bool` is then the project's own, and
  `std::Bool` is the prelude's. Importing it with no group leaves it to be named by its full name. The question mark on
  a type is `Option` as written in the project, so a project that overrides the prelude has no question mark unless it
  declares an `Option` itself or imports the prelude into `local`.
- `--no-prelude`, and `prelude = false` in the settings file, stop the implicit import in every project and do nothing
  else. An explicit import stays.
- A project that declares a name the prelude has is not an error where it is declared. It is an ambiguity at each bare
  use, as it is for any project in `local` (decision 25), and the fix is the override.
- There is always a project. When none is named the run is of a virtual project with no name, no files and the same
  implicit import of the prelude, and the files and standard input that are given are written in it as if they were its
  own, which is how they are treated when a project is named too. The command has no separate path for plain files. The
  prelude is the same project in both, and `--no-prelude` inhibits the import in both. The project with no name has bare
  names, so the messages of a plain `constructo check FILES` are unchanged apart from the clash with the prelude, and it
  has no project-file, so `--no-prelude` is the only way for it to override the prelude.
- The consts of a built-in project can be referred to without `--allow-refs`, as those of the prelude could. A built-in
  project is never reported in verbose mode, and it is not checksummed, since its content is fixed by the binary.

## Consequences

- The special cases for the prelude in the command are gone, and the prelude takes part in the resolution of names like
  any project does.
- The files of the prelude have a new name in diagnostics.
- A project can now run without the prelude, or with it under another name, and say so in its own project-file, which a
  flag could not do.
- A name that is declared and that the prelude has, for example `Bool`, is no longer caught until it is used, whether it
  is in a project or in the files that were given with none.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A key in the project-file to turn the prelude off. It is a second way of saying what an import says already, and it
  puts a setting in a file that has none.
- Leaving the choice to the flags. Whoever runs the check would then decide, so a project that needs its own `Bool`
  would fail for anyone who forgot the flag.
- Naming the types file as the source, `constructo:/prelude.types`. An import would then name a file and not a project,
  which would need a second kind of source and would not fit a built-in project with several files or imports.
- Keeping the duplicate error for the prelude's names in every project. The override could then never work for the names
  it exists for.
