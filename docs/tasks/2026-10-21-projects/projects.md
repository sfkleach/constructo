# Task: Projects

## Overview

So far every run of `constructo check` is given its inputs on the command line,
and all of them share one namespace. That will not scale to data-types that are
shared between teams or published for others to use. This task introduces
*projects*.

A project is a folder that contains:

- data-type files, which may also hold consts, and
- a project-file that describes the projects it transitively includes.
- a lock file that records checksums.
- a local configuration file; note that not all options may appear in this file.

The idea is that a project provides data-type files that may reference the
data-types of other projects. Projects are required to be uniquely named and
together the (project name * local name) acts as a canonical reference.

Unlike a normal package/module mechanism, the emphasis is not on selective
hiding because the identifiers are constructors and consts, which typically need
to be globally available! Instead we are concerned about making the system easy
to use and understand.

This is a draft. The questions in each step are to be settled before the step is
implemented, and the steps are in the order that I expect to build them.

### Note: Structured names

There are three kinds of names that we support:

- local names, which are simply normal identifiers e.g. `Data`
- full names, which is the dot-separated project name followed by the
  local name e.g. `com.steelypip.common::Data`
- standard names, which is a group-name followed by the local name
  e.g. `common::Data`.

Group names are local to a project. They a shortend synonym for one-or-more
project names. The current project is automatically put inside the `local`
group. And local names such as `Foo` are considered to be equivalent to the
standard name `local::Foo`. But, as we will see, there is a many-to-many
relationship between groups and projects.

Resolving a standard name means mapping it to its full-name. And it is 
required that there is only one such full-name. To illustrate this, let us
suppose that project P.Q.R contains three constructors `Left`, `Middle` and `Right`
and project X.Y.Z contains three constructors `Down`, `Middle`, and `Up` but
both projects were in group `pqrxyz`. 

- `pqrxyz::Left` is uniquely `P.Q.R::Left`
- `pqrxyz::Up` is uniquely `X.Y.Z::Up`
- but `pqrxyz::Middle` is an invalid reference because it could refer to
  `P.Q.R::Middle` or `X.Y.Z::Middle`.


## Step 0: Type files then values files

In this step, we require that all data-type files are presented before the 
any data-values file is parsed. The idea of mixing files in any order is now
rescinded.

## Step 1: The project-file

The project-file is a TOML file called `constructo.toml` and sits in the root of
the project folder. It is not the settings file of `--config`, which is a
separate file (the examples call it `constructo-settings.toml`) and is still
read only when it is named. It defines:

```toml
[project]
name = "com.example.shapes"
include = ["**/*.types"]
exclude = []
```

- `name` is required: a quoted string of two or more dot-separated segments.
  Each segment starts with a letter and has letters, digits and `_`. (Step 2
  tightened this from one or more segments and allowing `-`; see there.)
- `include` and `exclude` are lists of glob patterns relative to the project
  folder. A file belongs to the project if it matches an `include` and no
  `exclude`. They default to `["**/*.types"]` and `[]`.

Note that other projects is out of scope at this point in development. Now
implement `constructo check --project=FOLDER`, which loads a project and checks it.

- Read the project-file in the folder, then the data-type files that it names.
- The order of the inputs is the prelude, then the project's types files
  sorted by path, then `FILES` and `--stdin` as given. A `FILES` entry may be a
  types file, so long as it comes before any values file (Step 0).
- Files are named in diagnostics as the user would type the path: the folder
  as given, joined to the path within it, such as `../shared/shapes/circle.types`.
- `--project` is given once and needs its value: a repeat, or no value, is a
  usage error (exit 2), and `--project=.` is the current folder.
- Settings are unchanged. They come from the defaults, an explicit `--config`
  and the arguments, and the project-file holds none of them.
- With `--verbose` the project is reported once, before its files, as
  `OK: ../shared (project com.example.shapes)`.
- A project may also be given values files to check, as now: `constructo check
  --project=FOLDER FILES...`.

### Strictness

Nothing is lenient at this stage. An unknown key or table, a missing `name`, a
name that does not follow the rule, a value of the wrong type and a pattern that
is not a valid glob are all errors with a line and column in `constructo.toml`
and exit code 2, as for a config file. So are a folder that does not exist, a
missing or unreadable `constructo.toml`, and an unreadable types file. The
checks may be softened, very slightly, much later.

### Discovery

- Patterns use the `glob` crate, and are matched against the path inside the
  project with `/` as the separator, on every platform. `**/` matches any number
  of folders, including none, so `**/*.types` matches `circle.types` too.
- `**` does not enter a hidden folder (a name starting with `.`), or a subfolder
  that has its own `constructo.toml`, which is another project. The folder of the
  project itself is not skipped for either reason. A symbolic link to a folder is
  not followed.
- A matched file that turns out to hold data-values, such as `notes.types`
  that starts with `42;`, is an error naming the file (exit 1).
- A project in which nothing matches is not an error, since a project may
  consist only of imports (Step 2).

### Known fudge

The prelude is added ahead of the project's files by the command, exactly as it
is for `constructo check FILES`, and `--no-prelude` and `prelude = false` still
apply. This is a stopgap. The prelude will become a virtual project that every
project includes in the group `local` unless it overrides it, and then this goes
away. Until then a project that declares its own `Bool` or `Option` gets the
usual duplicate-constructor error. So the prelude stays outside `Project`, which
returns only the project's own files, and nothing may depend on it being there.

### Notes

- A project can hold other types of files, including data-values files, but
  they are ignored by the `--project` option.
- The library has a `project` module, with `Project::load(folder)`.
- Write a decision record, covering the above.

## Step 2: Implement Imports

Implement imports that are included in the `constructo.toml` file. Each
`[[import]]` has:

- `project-name`: required. The full name of the project that is imported. It is
  there to assist with searching and to state the intention, and it must equal
  the `name` in the project-file that is found at `source`; a difference is an
  error that shows both.
- `source`: required. The folder of the project, as `--project=FOLDER` takes
  one. A relative path is relative to the folder of the project that has the
  `[[import]]`, not to the current folder, and an absolute path is allowed.
- `group`: optional. The group the project is put into. Without one the project
  can only be named by its full name.

```toml
[[import]]
project-name = "P.Q.R"
group = "pqrxyz"
source = "../sister-data-types"
```

One `[[import]]` puts a project in one group. To have a project in two groups,
write two entries with the same `source`. The same project is still loaded once.

### Names

The names are as in the structured-names note above, with these rules.

- A full project name has at least one `.`, and a group name has none. So
  the two can never be mistaken for each other. This changes Step 1, where a
  name with no dot was allowed.
- The parts of a project name, and a group name, start with a letter and are
  letters, digits and `_`, of either case. There is no `-`, because a name has to
  be written in source text, where `-` begins a number. This also changes
  Step 1. A later task revises the case rules (see
  `docs/tasks/todo/2026-10-25-case-convention-selection`).
- A qualified name is written `group::Name` or `full.name::Name`, in a types
  file or a values file, wherever a type, constructor or const is used. A
  declaration is never qualified. The lexer gains `.` and `::`.
- The group `local` holds the project itself, and an import may put a project
  into it with `group = "local"`. That is how a project brings unqualified names
  into its own namespace, and is the way to build a bundle of namespaces.
- A bare `Foo` is `local::Foo`. A name is resolved to its full name, and there
  must be exactly one. Two projects in `local` that both have `Foo`, one of them
  the project itself, make a bare `Foo` an error at the use, naming the
  candidates. Nothing is checked at the declaration.
- Types, and constructors with consts, are separate namespaces (decision 4), and
  a qualified name resolves in the namespace that its use needs.
- A group name may not equal the full name of a project that the same project
  imports, and an unknown group or an unknown name in a group is an error at
  the use.

### Imports are not transitive

If A imports B and B imports C, then C is loaded and checked, because B's types
may name C's. But A can name only its own declarations and those of the projects
in its own `[[import]]` list, by group or by full name. To name C, A must import
C. Inherited imports are a later task and do not appear in this step.

### Resolution

- Every declaration has a canonical name, the project's full name and the local
  name, such as `P.Q.R::Left`. Two projects can both have a `Middle` and they are
  different types. Duplicates are checked within one project, as they are today
  for all the files.
- A values file given to `check --project=P` is resolved in the context of P.
  With no project, `local` holds every file given, as it does now, and there are
  no other groups.
- Diagnostics show a name as it would be written bare in the project of the file
  with the error, and any other name in full, such as `acme.shapes::Shape`. Plain
  `check FILES` keeps all its present messages. If two different types would be
  printed the same, both are shown in full.

### Notes

- Imports are loaded transitively to any depth, each project once, whatever
  the number of imports of it. A project is the same project if it has the same
  folder once it has been made absolute. Two different folders with the same
  project name are an error naming both.
- The projects are loaded in dependency order, depth-first in the order of the
  `[[import]]` entries, and each project's files are sorted as in Step 1. Then
  come `FILES` and `--stdin`.
- Cycles of inclusion are an error and the diagnostic gives the path around the
  cycle, as `a.b -> c.d -> a.b`. Like every other problem with an import it is
  reported at the `[[import]]` in the project-file, with a line and column.
- Everything in `[[import]]` is strict, as the rest of the project-file is: an
  unknown key, a missing `project-name` or `source`, a value of the wrong type,
  or a folder that cannot be read.
- The prelude stays as in Step 1 to the user, with the same behaviour for
  `--no-prelude` and for a project that declares `Bool`. Inside, it is a built-in
  entry of `local` for every project, so that the imported projects can use
  `Option` too. Making it a real virtual project that a project can override is
  the next step.
- A decision record covers the above.

## Step 3: Standard Prelude

With the advent of imports, the standard prelude is a virtual project that every
project imports into the group `local`, unless it says otherwise. The Step 1 and
Step 2 fudge, where the command adds the prelude ahead of the project, goes away.

### Built-in projects

- The `constructo.` prefix is reserved for projects that are built in. The
  standard prelude is `constructo.prelude`, and is held inside the binary.
- A built-in project is a project like any other, with a project-file and types
  files, and its source is a URI that names the project, as a folder would:
  `constructo:/prelude`. The loader has a small table of built-in projects,
  from the URI to the text of its files, so others can be added without new rules.
  The types file of the prelude is shown in diagnostics as
  `constructo:/prelude/prelude.types`, where `<prelude>` was.
- The name of a user project may not be `constructo` or start with
  `constructo.`; that is an error in its project-file. An import whose
  `project-name` starts with `constructo.`, or whose `source` starts with
  `constructo:`, must name a built-in project, with both agreeing, and
  anything else is an error that lists the ones there are.
- Built-in projects are not reported in verbose mode, as the prelude is not today.
  The consts of a built-in project can be referred to without `--allow-refs`,
  as those of the prelude can today.

### The implicit import

```toml
# What a project that has no import of the prelude gets, in effect.
[[import]]
project-name = "constructo.prelude"
group = "local"
source = "constructo:/prelude"
```

- Every project gets this import, and so do the files given without a project.
  It is a direct import, so `constructo.prelude::Bool` always works as well as `Bool`.
- A project with an `[[import]]` of `constructo.prelude` of its own does not get
  the implicit one. That is how it overrides the prelude: importing it into
  another group, such as `group = "std"`, makes bare `Bool` the project's own
  while `std::Bool` is still the prelude's, and importing it into `local` is
  the same as the default.
- `--no-prelude`, and `prelude = false` in the settings file, stop the implicit
  import in every project, and nothing else. An explicit import is the
  project's own statement and stays. With plain files they work as they do now.
- The special rule of Step 2, that declaring a name the prelude has is a duplicate
  at the declaration, is removed. The prelude is a project in `local` like any
  other, so declaring such a name makes each bare use ambiguous (decision 25), and
  the way out is the override above.
- There is always a project. With none named, it is a virtual project with no name
  and no files, with the implicit import of the prelude, and the files and standard
  input that are given are written in it, as they are in a named project. Its
  names stay bare. It has no project-file, so `--no-prelude` is its only way to
  override the prelude.

### Notes

- Step 4 does not checksum a built-in project, as it has no files on disk and
  its content is fixed by the binary.
- Write a decision record, covering the above.

## Step 4: Verify the checksums

Checksums of imports are calculated as the hashed digest of the files. They
should be retained in the constructo.lock file. The built-in projects, such as
`constructo.prelude`, are not checksummed, since their content is fixed by the
binary.

- Compare the checksum recorded for each file, and for each included project,
  with the content that was found. A mismatch is an error, and the diagnostic
  names the file or project and shows both values.
- A missing file, or a file in the folder that the project-file does not list,
  is an error when the files are listed explicitly. (Not applicable if the rule
  is to take every file.)
- Add a way to generate the checksums: `constructo project update [FOLDER]` or
  similar, which (re)generates the project-file's checksums and leaves the rest as
  it was.
- `constructo check` never changes any file.

- Verification of the checksums automatically happens during `constructo check`
  - But verification of the local files can be disabled with the `-d,--dev` option.


- Are the checksums required, or may they be left out for a project under
  development?
- What does a user do to accept a changed dependency? Probably they run the
  update command and review the difference.

## Step 5: Verbose mode and diagnostics

- In verbose mode `check` reports each project that it loaded, with its version
  and where it was found, in the same style as the files: `OK: ../shared
  (project shared 1.2.0)`.
- The final count says how many files and how many projects were checked.
- Every diagnostic from a file in an included project says which project it is
  from.

## Step 6: Support remote projects

Allow the project source to be a specified version from a remote git-repo. 
N.B. We need a Q&A session to elaborate this step.

## Step 7: Spec files

Add a description of the project and lock format files in
`docs/specs/project-file-format.md` and `docs/specs/lock-file-format.md`
respectively.

The lock-file document should included a description of the checksum algorithms.

The resolution algorithm should be documented in
`docs/specs/resolving-identifiers.md`


## Step 8: Consistency pass

- Update `docs/vision.md` and the README to describe projects.
- Record the design decisions (the project-file format, the digest, the
  namespace rule, how conflicts are handled) in `docs/decisions/`.
- Update the grammar files if the project-file is written in Constructo, the
  examples in `syntax/examples/`, the functional tests and the CHANGELOG.


## Out of scope for this task

- Version ranges and any solving of version constraints.
- Publishing a project.


