# The project-file

A project is a folder with a project-file in its root. The project-file is called `constructo.toml`. It gives the
project its name, says which files belong to it and lists the other projects that it imports. It is read by
`constructo check --project=FOLDER` and by every project that imports this one.

The project-file is not the settings file of [`--config`](config-file.md), which is a separate file that is read only
when it is named. The project-file holds no settings.

## Example

```toml
[project]
name = "com.example.shapes"
include = ["**/*.types"]
exclude = ["old/**"]

[[import]]
project-name = "com.example.base"
group = "base"
path = "../base"

[[import]]
project-name = "constructo.prelude"
group = "local"
builtin = "constructo:/prelude"
```

## Format

The file is in [TOML](https://toml.io), in UTF-8. A byte order mark at the start is removed. It is strict: an unknown
key or table, a missing required key, a value of the wrong type and a malformed name or pattern are all errors. The
file has the table `[project]` and any number of `[[import]]` tables, and nothing else.

### `[project]`

| Key       | Type             | Required | Default         |
|-----------|------------------|----------|-----------------|
| `name`    | string           | yes      |                 |
| `include` | array of strings | no       | `["**/*.types"]` |
| `exclude` | array of strings | no       | `[]`            |

`name` is the full name of the project. See [Names](#names).

`include` and `exclude` are glob patterns. A file belongs to the project if it matches an `include` and matches no
`exclude`. An `include` that is an empty array selects nothing, which is not the same as leaving it out. A project
in which nothing matches is not an error, since a project may consist only of imports.

### `[[import]]`

| Key            | Type   | Required | Meaning                                                              |
|----------------|--------|----------|----------------------------------------------------------------------|
| `project-name` | string | yes      | The full name of the project that is imported.                       |
| `path`         | string | one of   | A folder that holds the project.                                     |
| `builtin`      | string | one of   | The URI of a built-in project.                                       |
| `repo`         | string | one of   | The URL of a git repository, as [below](#projects-from-a-repository). |
| `rev`          | string | no       | With `repo` only: a tag, branch or commit. Without one, the default branch. |
| `subdir`       | string | no       | With `repo` only: the folder inside the repository that holds the project. |
| `group`        | string | no       | The group that the project is put into. Without one, only by its full name. |

Exactly one of `path`, `builtin` and `repo` says where the project is. Two of them, or none, is an error at the
import, and so is a `rev` or a `subdir` without a `repo`.

- `project-name` states the intention and helps with searching. It must equal the `name` in the project-file that is
  found at the `path`, and a difference is an error that shows both.
- `path` may be absolute, or relative to the folder of the project that has the `[[import]]`, not to the current
  folder. A `path` that is empty is an error, and so is one that begins with `constructo:`, since a built-in project
  (see [Built-in projects](#built-in-projects)) is imported with `builtin`.
- One `[[import]]` puts a project in one group. To put a project in two groups, write two entries with the same
  `path`. The project is still loaded once. The same project in the same group twice, or with no group twice, is an
  error.

### Projects from a repository

An import with a `repo` takes the project from a git repository.

```toml
[[import]]
project-name = "com.example.shapes"
repo = "https://github.com/example/shapes.git"
rev = "v1.2.0"
subdir = "constructo"
group = "shapes"
```

- `repo` is `https://HOST/PATH`, `ssh://[USER@]HOST/PATH` or the short form `[USER@]HOST:PATH`, `file://PATH`, or a path
  to a folder. A URL with a password, or a token in place of a user name, is an error, since it would be written in the
  lock file and in messages: use a credential helper or an ssh key. `ext::` and the other remote helpers, `git://`,
  `http://` and `ftp://` are errors, and so is a URL that starts with `-`. The error is at the `repo`.
- `rev` is a tag, a branch or a commit, and may not be empty. `subdir` is a path inside the repository, written with
  `/`, with no empty part, `.` or `..`.
- `constructo check` does not fetch. The commit comes from the [lock file](lock-file-format.md) and the files from the
  cache, and `constructo project update` is what gets them. A `check` that finds no lock, no entry, or no checkout
  in the cache is an error at the import, with exit code 1.
- A project from a repository stays inside the clone. A link in the repository that leads out of it, for the
  project-file, the folder or a types file, is an error at the import, with exit code 2, that says where it leads. A
  link that matches a pattern is checked even if its target does not exist, and then it is an error that it leads
  nowhere, so that a repository is not accepted on one machine and refused on another. A
  `path` import in such a project must stay inside the clone by the path as written, and so must the projects it
  reaches. It may import other repositories and built-in projects.
- The `git` program is used, so it has to be installed. The cache folder is given by `--cache`, by `cache` in the
  `[global]` table of the configuration file, or by `$CONSTRUCTO_CACHE`. Without any of them the cache is
  `constructo` in `$XDG_CACHE_HOME` or `~/.cache`, and `%LOCALAPPDATA%` on Windows. See
  [the cache](cache-management.md) and [how git is run](git-management.md).

## Names

A project name is two or more parts separated by dots, as in `com.example.shapes`. A part starts with a letter and has
letters, digits and `_`, of either case. There is no `-`, because a name has to be written in source text, where `-`
begins a number. The dot tells a project name from a group name, which has none.

A group name is one part, with the same rule. The group `local` has a special meaning: it holds the project itself and
is where unqualified names are looked up. See [Resolving identifiers](resolving-identifiers.md).

The words `enum` and `const` are keywords, but they may be a part of a project name or a group name, as in
`example.enum::Shape`, because the parser takes a keyword as a part of a qualifier when a dot or `::` follows it.

The `constructo.` prefix is reserved for built-in projects. A project-file cannot name its project `constructo` or
anything that starts with `constructo.`.

## Which files belong to a project

- Patterns are matched against the path of the file inside the project folder, written with `/` as the separator on
  every platform. `**/` matches any number of folders, including none, so `**/*.types` matches `circle.types` too.
- `**` does not enter a hidden folder, whose name starts with `.`, nor a subfolder that has a `constructo.toml` of its
  own, which is another project. The folder of the project itself is not skipped for either reason.
- A symbolic link to a folder is not followed, so that a loop cannot make the walk endless.
- The files of a project are sorted by path.
- A file that matches but holds data-values, such as a file `notes.types` that starts with `42;`, is an error that names
  the file. Other kinds of file in the folder are ignored.

## Loading

- The projects that a project imports are loaded in turn, to any depth, each once. A project is the same project if its
  folder is the same once it has been made absolute, with no links and no `..`. Two different folders with the same
  project name are an error that names both.
- The order is depth-first in the order of the `[[import]]` entries: a project's imports come before its own files, and
  they in turn bring theirs. Then come the files and standard input that were named on the command line.
- A cycle of imports is an error, and the diagnostic gives the path around the cycle, as `a.b -> c.d -> a.b`.
- Every project gets an implicit `[[import]]` of the prelude into `local` unless it has an `[[import]]` of
  `constructo.prelude` of its own. See [Built-in projects](#built-in-projects).

## Built-in projects

A built-in project is a project like any other, but it is held inside the binary. Its `builtin` is a URI, as a folder
would be, and the files of the project are shown in diagnostics under it, as `constructo:/prelude/prelude.types`.

| Source                | Name                 | Contents                              |
|-----------------------|----------------------|---------------------------------------|
| `constructo:/prelude` | `constructo.prelude` | `Bool`, `Option`, `null`, `true` and `false` |

An import whose `project-name` starts with `constructo.`, or that has a `builtin`, has to name a built-in project with
both, and anything else is an error that lists the built-in projects. A built-in project is not reported in verbose
mode and is not checksummed.

The implicit import is, in effect:

```toml
[[import]]
project-name = "constructo.prelude"
group = "local"
builtin = "constructo:/prelude"
```

To override the prelude a project imports it into another group. Then bare `Bool` is the project's own, and `std::Bool`
is the prelude's. `--no-prelude`, or `prelude = false` in a settings file, stops the implicit import in every project
and nothing else. An explicit import stays.

## Errors

Every problem with the project-file is an error in the usual form, with the position of the offending value in
`constructo.toml`:

```text
ERROR: An import needs one of `path`, `repo` and `builtin` to say where the project is (constructo.toml, line 5, col 1)
```

A problem with an import is reported at the `[[import]]`. A folder that does not exist, a missing or unreadable
`constructo.toml` and an unreadable types file are errors too. The exit code is 2 for all of these, as it is for a
configuration file, and 1 for the errors that are found in the types files and values files themselves.

## Related

- [Resolving identifiers](resolving-identifiers.md), for what a group and a qualified name mean.
- [The lock file](lock-file-format.md), which records the checksums of the projects that are imported.
- Decisions 24 to 26, 29 and 30, in `docs/decisions/`.
