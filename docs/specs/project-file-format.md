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
source = "../base"

[[import]]
project-name = "constructo.prelude"
group = "local"
source = "constructo:/prelude"
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
| `source`       | string | yes      | Where the project is: a folder, or the URI of a built-in project.    |
| `group`        | string | no       | The group that the project is put into. Without one, only by its full name. |

- `project-name` states the intention and helps with searching. It must equal the `name` in the project-file that is
  found at `source`, and a difference is an error that shows both.
- `source` that is a folder may be absolute, or relative to the folder of the project that has the `[[import]]`, not to
  the current folder. A source that begins with `constructo:` is a built-in project (see
  [Built-in projects](#built-in-projects)). A source that is empty is an error.
- One `[[import]]` puts a project in one group. To put a project in two groups, write two entries with the same
  `source`. The project is still loaded once. The same project in the same group twice, or with no group twice, is an
  error.

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
- Every file of a project, and its `constructo.toml`, must be inside the project: its canonical path, which has no links
  or `..`, must lie in the canonical path of the project folder. A link to another file of the project is fine, and a
  link to a file outside it is an error that names the file and where it leads, with exit code 2. A file is a part of
  the project, and is digested for the lock file, only if the folder really contains it.
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

A built-in project is a project like any other, but it is held inside the binary. Its source is a URI, as a folder
would be, and the files of the project are shown in diagnostics under it, as `constructo:/prelude/prelude.types`.

| Source                | Name                 | Contents                              |
|-----------------------|----------------------|---------------------------------------|
| `constructo:/prelude` | `constructo.prelude` | `Bool`, `Option`, `null`, `true` and `false` |

An import whose `project-name` starts with `constructo.`, or whose `source` starts with `constructo:`, has to name a
built-in project with both, and anything else is an error that lists the built-in projects. A built-in project is not
reported in verbose mode and is not checksummed.

The implicit import is, in effect:

```toml
[[import]]
project-name = "constructo.prelude"
group = "local"
source = "constructo:/prelude"
```

To override the prelude a project imports it into another group. Then bare `Bool` is the project's own, and `std::Bool`
is the prelude's. `--no-prelude`, or `prelude = false` in a settings file, stops the implicit import in every project
and nothing else. An explicit import stays.

## Errors

Every problem with the project-file is an error in the usual form, with the position of the offending value in
`constructo.toml`:

```text
ERROR: missing field `source` (constructo.toml, line 5, col 1)
```

A problem with an import is reported at the `[[import]]`. A folder that does not exist, a missing or unreadable
`constructo.toml` and an unreadable types file are errors too. The exit code is 2 for all of these, as it is for a
configuration file, and 1 for the errors that are found in the types files and values files themselves.

## Related

- [Resolving identifiers](resolving-identifiers.md), for what a group and a qualified name mean.
- [The lock file](lock-file-format.md), which records the checksums of the projects that are imported.
- Decisions 24 to 26, in `docs/decisions/`.
