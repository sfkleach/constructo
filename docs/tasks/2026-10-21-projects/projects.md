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
path = "../sister-data-types"
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
builtin = "constructo:/prelude"
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

A project that imports other projects records a digest of each of them in a
lock file, so that a dependency cannot change without being noticed. The lock
file is `constructo.lock`, it sits next to `constructo.toml` in the root of the
project that `check` is run on, and it is written by `constructo project update`
and by nothing else. `constructo check` never changes any file.

### The lock file

It is TOML and has one table for each project that is loaded, other than the
root and the built-in projects (`constructo.prelude` has no files on disk and its
content is fixed by the binary). It covers the whole closure of imports, direct
and indirect, as `Cargo.lock` does: imports are not transitive for names but they
are for loading, and a change deep in the graph must still be caught. The lock
files of the imported projects are ignored. The projects are sorted by name and
the files by path, so that the file does not change unless the content does.

```toml
# This file is generated by `constructo project update`. Do not edit it.
version = 1

[[project]]
name = "com.example.lib"
digest = "sha256:9f2c..."

[project.files]
"constructo.toml" = "sha256:41ab..."
"shapes/circle.types" = "sha256:77e0..."
"shapes/square.types" = "sha256:c3d1..."
```

- The files of a project are the ones that its `include` and `exclude` patterns
  select, and its `constructo.toml`. Including the project-file means that a
  change to what a dependency imports, or to its patterns, is noticed. Values
  files, notes and other files in the folder are not covered.
- A file digest is `sha256:` and the hexadecimal SHA-256 of the file's bytes
  after a leading byte order mark has been removed and each CRLF has been
  replaced with LF. So a project that is checked out on Windows with CRLF files
  matches a lock that was made on Linux, and so does one that was saved by an
  editor that adds a byte order mark. A lone CR is left alone. The mark is
  removed from every file that is read, as `rustc` does for source files (see
  the CHANGELOG), and the digest is of what is left.
- The project digest is `sha256:` and the hexadecimal SHA-256 of the lines
  `<path> <file digest>`, one for each file, sorted by path, each ended by LF. It
  depends only on the paths and the contents. It is a quick check and a single
  value to quote.
- The algorithm is part of the value, so that it can be changed later. `version`
  is the version of the format and is 1.
- The lock file is strict: an unknown key, a value of the wrong type, a bad
  digest and a missing or duplicate project are errors with a line and column in
  `constructo.lock` and exit code 2, as for the project-file.

### Verification

`constructo check --project=FOLDER` verifies the lock after the projects have
been loaded and before anything is type-checked. The problems are reported
together, since each is an independent fact, and the exit code is 1.

- A project or file whose digest differs from the lock is an error that names
  the project and the file and shows both values, at the entry in the lock file:
  `ERROR: Checksum mismatch for shapes/circle.types of project com.example.lib:
  the lock has sha256:77e0... but the file has sha256:12ac... (constructo.lock,
  line 8, col 24)`.
- A loaded project that has no entry is an error at the import that loaded it.
  A file of a project that matches the patterns but is not in the lock is an
  error at the entry of the project.
- The lock must describe exactly what is loaded. A project in the lock that is
  not loaded, and a file in the lock that no longer exists in the project, are
  errors at their entries. `project update` puts all of these right.
- A project with nothing to record, such as one that imports only the built-in
  prelude, needs no lock file. If it has none then nothing is verified. Any
  other project with no lock file is an error at its first import, which says to
  run `constructo project update`.
- A lock file that cannot be read, or that does not parse, is exit code 2.
- `constructo check` without `--project` has no lock and verifies nothing.

### The option `--dev`

`-d, --dev` switches off all verification of the lock, for every project, and a
missing lock file is then not an error. It means "I am developing, trust what is
on disk". It is a setting like the other boolean options (decision 21): `dev` in
the `[check]` table, a negative form `--no-dev`, and it is shown by
`--print-config`. The configuration file is read only when it is named, so it
cannot switch verification off unnoticed. Step 6 narrowed this: `--dev` leaves
out the projects that are folders on disk, and a project from a repository is
always verified (decision 30).

### The command `constructo project update [FOLDER]`

`FOLDER` defaults to the current folder. The command loads the project in the
same way as `check` does, with the same strictness, writes `constructo.lock`
there and prints one line for each difference from the old lock: a project or a
file that was added, changed or removed, with the old and new digest. It prints
nothing if the lock is already up to date. It does not need the old lock to be
valid, since the point is to accept a change, and it does not type-check
anything. A project with nothing to record gets no lock file, and an old one is
removed so that it does not become stale. The user reviews the difference with the
output of the command or with `git diff constructo.lock`.

### Not in this step

- The checksums of values files and of the other files in a project.
- Fetching, and so the checksums of remote projects (Step 6), though the lock
  file needs no change for them, and the source of a project is deliberately not
  recorded in it: the same project may be written with different paths by
  different importers.

## Step 5: Verbose mode and diagnostics

This step only tells the user what is being done on their behalf to ensure
provenance. It is small, and is built after Step 4, because it reports the
result of the verification.

Already done in Step 1 to 3: in verbose mode `check` reports each project that
it loaded, with its name and where it was found, as `OK: ../shared (project
com.example.shapes)`, or as `FAILED:` if one of its files has an error, and it
does not report the built-in projects.

- Each project line also says whether the project was checked against the lock:
  `OK: ../shared (project com.example.shapes, verified)`. With `--dev` it says
  `unverified`, and the root project, which is not in the lock, says neither.
- The final count says how many projects were checked as well as how many
  files: `OK: checked 5 file(s) in 3 project(s)`. The projects counted are the
  root and the imported ones, and not the built-in ones.
- There is no version. The project-file has none, and `version` stays an unknown
  key. A version belongs with remote projects (Step 6), where a git tag or
  revision is what the user is relying on.
- A diagnostic does not say which project it is from. The path of the file is
  shown as the user typed the folder, which says where it came from, and the
  project line in verbose mode connects the folder to the name.

## Step 6: Support remote projects

Allow the project source to be a specified version from a remote git-repo. The Q&A session has been held and the
plan is in [plan-step06.md](plan-step06.md). In short, `source = PATH` becomes exactly one of the keys `path`,
`repo` (with optional `rev` and `subdir`) or `builtin` on the `[[import]]`; only `constructo project update` fetches,
and it pins the commit in the lock file. This step is done, and decisions 29 and 30 record it.

### Links out of a fetched project

For a project on disk, a symbolic link to a file outside the project is allowed.
The file is identified by its path from the project folder, which has no `..`,
and the lock file digests its content whatever the link leads to, so a change
is caught. A shared types file can be linked into several projects.

This does not carry over to a project that is fetched. In a clone, a link that
leaves the clone has a target that is missing or is not the author's, and in an
untrusted repository it could make the tool read a file of the user's, such as
`~/.ssh/config`, whose content a message could quote. So when a project is
fetched, a file whose canonical path is outside the clone, and the
project-file if it is one, is an error that names the file and where it leads.

## Step 7: Spec files

The specs are written, ahead of the other steps, so that they say what the code is built to:

- `docs/specs/project-file-format.md`, the project-file.
- `docs/specs/lock-file-format.md`, the lock file and the checksum algorithms.
- `docs/specs/resolving-identifiers.md`, the resolution of names.

What is left for this step is to check each spec against the code and the examples once the other steps are done, and
to update them for anything that Step 5 adds. The remote sources of Step 6 are already in them.

## Step 8: Consistency pass

- Update `docs/vision.md` and the README to describe projects.
- Record the design decisions (the project-file format, the digest, the
  namespace rule, how conflicts are handled) in `docs/decisions/`.
- Update the grammar files if the project-file is written in Constructo, the
  examples in `syntax/examples/`, the functional tests and the CHANGELOG.


## Out of scope for this task

- Version ranges and any solving of version constraints.
- Publishing a project.


