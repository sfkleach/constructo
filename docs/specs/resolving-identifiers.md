# Resolving identifiers

A name in a types file or a values file refers to a declaration: a type, or a constructor or a const. This document says
which declaration a name refers to, and how the name is shown in a message. A name is resolved in the project of the
file that writes it. Groups are defined in the project-file, which is described in
[the project-file](project-file-format.md).

## Kinds of name

There are three ways to write a name.

| Kind     | Written                       | Example                       |
|----------|-------------------------------|-------------------------------|
| local    | the identifier alone          | `Shape`                       |
| standard | a group name, `::`, the name  | `common::Shape`               |
| full     | a project name, `::`, the name | `com.example.shapes::Shape`  |

A local name `Shape` is the standard name `local::Shape`. A group name is one part with no dot, and a full name has one
or more dots before the `::`, so the two can never be mistaken for each other. A declaration is never qualified: it
declares a name in its own project.

A qualifier is allowed wherever a type, a constructor or a const is used, in a types file and in a values file. Its
parts are identifiers of either case separated by dots. A part may be `enum` or `const`. The basic types `Nat`, `Int`,
`Float` and `String` are always written bare and are never qualified.

## Groups

A project puts the projects that it imports into groups, with the `group` of an `[[import]]`. A group is local to the
project that defines it. The relation is many to many: a group holds one or more projects, and a project may be in
several groups of the same importer, by writing several imports of it.

- The group `local` holds the project itself, the projects that are imported with `group = "local"`, and, unless the
  project says otherwise, the prelude.
- An import with no group puts the project in no group. It can be named only by its full name.
- A full name refers to the project itself, or to a project that it imports directly.

Imports are not transitive. If A imports B and B imports C, C is loaded and checked because B's declarations may name
it, but A can name only its own declarations and those of B. To name C, A has to import C.

## The algorithm

To resolve a name `N` written in a file of project P:

1. Find the **candidate projects**.
   - For a bare name, or `local::N`, they are the projects in the group `local` of P.
   - For `g::N` they are the projects in the group `g` of P. If P has no group `g` it is an error.
   - For `a.b.c::N` the candidate is the project named `a.b.c`, which must be P itself or a project that P imports
     directly. If it is not, it is an error.
2. Choose the **namespace** from the use. There are two: types, and constructors and consts together (decision 4). A
   name where a type is expected is looked up among the types, and any other name among the constructors and consts.
3. Keep the candidates that declare `N` in that namespace, leaving out those whose import hides `N` and those whose
   import renames `N`, and including those whose import renames another name to `N`. A candidate that is named by its
   full name is not filtered, since the full name is the project and not the import.
4. If one remains, that is the declaration. If none remains, the name is unknown. More than one cannot remain: no two
   projects of a group declare one name, as below.

The use decides the namespace, so `Shape` can be both a type and a constructor and there is no clash.

### Errors

A name that two projects of a group declare is an error where it is declared (see below). The others are at the use of
the name.

- A group that the project does not have: `Unknown group g`.
- A full name that is neither the project nor a project that it imports:
  `Unknown project a.b.c: a project can name itself and the projects it imports`.
- A qualifier whose projects do not declare the name: `Unknown constructor Foo in g`, or `Unknown type` or
  `Unknown const` for the other namespaces.
- A bare name that nothing declares is not a resolution error. The checks that follow report it as unknown.

### No clashes in a group

The projects of a group, and for `local` the project itself, share one scope, so none of them may declare a name that
another of them declares, in the same namespace. A clash is a duplicate, and is reported where it is declared, like any
other, with a note that names the first declaration: `Duplicate constructor Middle (b.types, line 1, col 10); first
declared at a.types, line 1, col 10`. This is found for every group of every project in the run when the projects are
loaded, whether or not the name is used, and nothing is reported again at a use. The projects of a group are taken in
the order of the imports, and the project itself last, so that a clash with an import is reported in the project. The
prelude is in `local`, so a project cannot declare `Bool` unless it puts the prelude in another group. A name that the
import hides is not in the scope, and so does not clash, and a name that it renames is in the scope under its new
name. The names of projects in different groups do not clash, and a
project and a group other than `local` do not either. Decision 32 records this, and
decisions 33 and 34 hiding and renaming.

### Example

Suppose project `P.Q.R` declares the constructors `Left`, `Middle` and `Right`, and project `X.Y.Z` declares `Down`,
`Centre` and `Up`, and a project imports both into the group `pqrxyz`.

| Written           | Resolves to                                                                |
|-------------------|----------------------------------------------------------------------------|
| `pqrxyz::Left`    | `P.Q.R::Left`                                                              |
| `pqrxyz::Up`      | `X.Y.Z::Up`                                                                |
| `pqrxyz::Centre`  | `X.Y.Z::Centre`                                                            |
| `P.Q.R::Middle`   | `P.Q.R::Middle`                                                            |
| `Left`            | `P.Q.R::Left` if `P.Q.R` is in `local`, otherwise unknown                  |

## The prelude

Every project is given an import of `constructo.prelude` into `local`, as if its project-file had

```toml
[[import]]
project-name = "constructo.prelude"
group = "local"
builtin = "constructo:/prelude"
```

unless it has an `[[import]]` of `constructo.prelude` of its own. So `Bool`, `Option`, `null`, `true` and `false` can
be written bare, and `constructo.prelude::Bool` always works as well. A project that declares its own `Bool`, `True`
or any other name of the prelude has a duplicate at the declaration, and the way out is to import the prelude into
another group, such as `std`, so that `Bool` is the project's and `std::Bool` is the prelude's. `--no-prelude` stops
the implicit import in every project.

## Files without a project

When no project is named, the run is of a virtual project with no name, no files and no project-file, with the implicit
import of the prelude. The files and standard input that are given are written in it, as they are in a named project.
So `local::Foo` works for them and no other qualifier does, and the names in messages stay bare.

When a project is named with `--project`, the files and standard input that are given as well are written in that
project, as if they were among its own files, and are resolved in its context.

## Canonical names

Inside the checker a declaration is known by a canonical name, the local name followed by `@` and the full name of its
project, as in `Shape@com.example.shapes`. Resolution rewrites each reference to the canonical name of what it refers
to, and the validity checks and the type-check then work on canonical names only. The prelude and the files without a
project keep bare names, so nothing about them changes. Canonical names never appear in a message.

## Names in messages

A name in a message is shown as it would be written in the file that the message is about. It is shown bare if it
resolves to the same declaration there, and otherwise in full, as `com.example.shapes::Shape`. So two different types
are never shown the same, and the messages of a plain `constructo check FILES` are unchanged. A file name that is
quoted in a message is left as it is, even if it contains an `@`.

## Related

- [The project-file](project-file-format.md), where groups and imports are defined.
- Decisions 4, 25, 26, 31 to 34, in `docs/decisions/`.
