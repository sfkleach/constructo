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

- `name` is required: a quoted string of one or more dot-separated segments.
  Each segment starts with a letter and has letters, digits, `_` and `-`.
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

Implement imports that are included in the `contructo.toml` file. Each import
defines:

- the name of the group it belongs to
- the source of the project, which at this stage is an absolute or relative
  path to the constructo.toml file.

The TOML imports should look somthing like this. The project-name is strictly
speaking redundant but is there to assist with searching and explicitly stating
the intention.
```toml
[[import]]
project-name = P.Q.R
group = pqrxyz
source = ../sister-data-types
```

### Group names and Long names

Within the data-values file, in addition to local names, full-names and 
group names are supported as well.  

### Notes

- The imports are followed transitively to any depth.
- Although any project is only loaded once, it may be imported many times.
- Cycles of inclusion are an error and the diagnostic gives the path around the
  cycle.

## Step 3: Verify the checksums

Checksums of imports are calculated as the hashed digest of the files. They
should be retained in the constructo.lock file.

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

## Step 4: Verbose mode and diagnostics

- In verbose mode `check` reports each project that it loaded, with its version
  and where it was found, in the same style as the files: `OK: ../shared
  (project shared 1.2.0)`.
- The final count says how many files and how many projects were checked.
- Every diagnostic from a file in an included project says which project it is
  from.

## Step 5: Support remote projects

Allow the project source to be a specified version from a remote git-repo. 
N.B. We need a Q&A session to elaborate this step.

## Step 6: Spec files

Add a description of the project and lock format files in
`docs/specs/project-file-format.md` and `docs/specs/lock-file-format.md`
respectively.

The lock-file document should included a description of the checksum algorithms.

The resolution algorithm should be documented in
`docs/specs/resolving-identifiers.md`


## Step 7: Consistency pass

- Update `docs/vision.md` and the README to describe projects.
- Record the design decisions (the project-file format, the digest, the
  namespace rule, how conflicts are handled) in `docs/decisions/`.
- Update the grammar files if the project-file is written in Constructo, the
  examples in `syntax/examples/`, the functional tests and the CHANGELOG.


## Out of scope for this task

- Version ranges and any solving of version constraints.
- Publishing a project.


