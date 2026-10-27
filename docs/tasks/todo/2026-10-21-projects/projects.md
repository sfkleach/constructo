# Task: Projects

## Overview

So far every run of `constructo check` is given its inputs on the command line,
and all of them share one namespace. That does not scale to data-types that are
shared between teams or published for others to use. This task introduces
*projects*.

A project is a folder that contains:

- data-type files, which may also hold consts, and
- a project-file that describes the projects it transitively includes, along with other meta-details such as checksums.
- a local configuration file; note that not all options may appear in this file.

The aim is that the types a project needs can be found, pinned to known content,
and checked as one namespace without listing every file by hand.

This is a draft. The questions in each step are to be settled before the step is
implemented, and the steps are in the order that I expect to build them.

### Note: Structured names

Projects may import other projects. Each import is given a 'nickname' that 
is used to refer to identifiers in the namespace of the imported project.
For example, foo::bar refers to the identifer `bar` from the import `foo`.

- Imports may allow the nickname to be required or optional.
- Imports may allow or forbid the shadowing of identifiers.
- Imports may be included into the importing namespace, and hence 
  transitively visible, or not included and hence only visible locally.

For example
```
Import{
  nickname: "foo",
  project: Path(...),
  implicit: true,
  shadowing: true,
  include: false
}
```

## Step 1: The project-file

Define the project-file, which sits at the root of a project folder.

- Its name and location. For example `constructo.project` at the root of the
  folder.
- Its syntax. The preference is to reuse Constructo itself, so that the
  project-file is a data-values file that is checked against a data-types file
  that is built in to the tool, as the prelude is. The alternatives are JSON or
  TOML, which would need a dependency or a second parser.
- Its contents, for the project itself:
  - the name and version of the project,
  - the data-type files that belong to it, or a rule for finding them (every
    `*.types` file in the folder?), and
  - the checksum of each of those files.
- Its contents, for each included project:
  - the name and version,
  - where to find it (a relative path to a folder, for a start), and
  - the checksum of the included project.
- A `constructo check` of a project-file alone reports its errors in the usual
  `ERROR:` form, with the file, line and column.

Questions:

- What is the checksum of? One file is easy. A project needs a defined digest of
  its files: a sorted list of names and file digests, say. Line endings and the
  order of files must not change it.
- Which hash? SHA-256 is the default choice. A hash function needs a dependency
  or a hand-written implementation, and the project has no dependencies except
  for `clap`.
- Is the version a number the tool understands, or only a label? If the tool
  does not compare versions then two projects of the same name but different
  versions can only be reported as a conflict.

## Step 2: Find and load a project

Add `constructo check --project FOLDER`, which loads a project and checks it.

- Read the project-file in the folder, then the data-type files that it names.
- Follow the included projects, and the projects that they include, to any
  depth. Each project is loaded once, however many times it is included.
- All the files of all the projects form one namespace, as with files on the
  command line (decision 1), and duplicates are errors. The diagnostics name the
  project as well as the file.
- Cycles of inclusion are an error and the diagnostic gives the path around the
  cycle.
- The standard prelude is still loaded first, unless `--no-prelude` is set.
- The order of the sources is fixed: the included projects first, in dependency
  order, and the project's own files last. That matters because a values file
  sees only the types that come before it.
- A project may also be given values files to check, as now: `constructo check
  --project FOLDER FILES...`.

Questions:

- What if two included projects have the same name? The same name and version
  from two places is one project if the checksums agree and a conflict if they
  do not. The same name with two versions is a conflict for now.
- Is a project's namespace flat, so that all the names are visible everywhere,
  or does a project hide the names that it includes? Flat is the simplest and
  agrees with decision 1. It means that a project cannot use a name that an
  included project already has, even if the project was only a distant
  dependency.
- Can a project hold values files, or only types?

## Step 3: Verify the checksums

- Compare the checksum recorded for each file, and for each included project,
  with the content that was found. A mismatch is an error, and the diagnostic
  names the file or project and shows both values.
- A missing file, or a file in the folder that the project-file does not list,
  is an error when the files are listed explicitly. (Not applicable if the rule
  is to take every file.)
- Add a way to write the checksums: `constructo project update [FOLDER]` or
  similar, which regenerates the project-file's checksums and leaves the rest as
  it was. Whether the tool should rewrite a hand-written file is a question to
  be settled, since it may lose comments and layout.
- `constructo check` never changes any file.

Questions:

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

## Step 5: Consistency pass

- Update `docs/vision.md` and the README to describe projects.
- Record the design decisions (the project-file format, the digest, the
  namespace rule, how conflicts are handled) in `docs/decisions/`.
- Update the grammar files if the project-file is written in Constructo, the
  examples in `syntax/examples/`, the functional tests and the CHANGELOG.

## Out of scope for this task

- Fetching a project from a remote location, such as a Git repository or a
  registry. The checksums are there so that this can be added later without
  changing the format.
- Version ranges and any solving of version constraints.
- Publishing a project.
