# 0042 - A value can name any project by its full name, 2026-11-13
> Amends [decision 25](0025-names-are-resolved-in-the-project-that-writes-them.md): imports are still not transitive for
> declarations, but the values of a values file can write the full name of any project of the run.

## Issue

**Fork.** Decision 25 says that a full name refers to the project itself or to one that it imports, and gives the
reason that a project should not depend on names that it does not list. A type of a project A can hold a type of a
project B that A does not import, because it comes through C, which A does import. A value of that type has to write
the constructors of B, and A had no way to name them. A values file written by hand could not be written, and
`convert --from=json-typed` wrote the full name, which `check` then refused. A full name is the canonical name of a
declaration, which is absolute: what a project may refer to is a different question from what the name means.

## Decision

- In a values file, and in the values that it holds, a full name can be the name of any project of the run, whether
  or not the project of the file imports it. Those are the projects that were loaded, which are the root, its imports
  and theirs.
- A full name that is not the name of any project of the run is still an error, `Unknown project a.b.c: no project of
  that name is used here`, so a misspelt name is not taken for a project.
- Declarations are not changed. A types file, including the value of a const in it, can name only its own project and
  the projects that it imports, as decision 25 says, so a project still lists what it depends on.

## Consequences

- What `convert` writes for a type that holds a type of a project that is not imported is accepted by `check`.
- A values file can name a project that is not in its project-file. A value cannot add a dependency, since the types
  decide which constructors it may use, and a name that does not fit is an error in the check as before.
- A one-letter slip in a full name is caught if no project has that name, and if one has, by the check of the value
  against its type.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Any full name in any file. It is the simplest, but a declaration would then depend on a project that its
  project-file does not list, which is what decision 25 avoids.
- Making `convert` refuse the case. The value can be correct, and the language could not say it, so the fault was in
  the rule and not in the converter.
- Making imports transitive. It has the same trouble for declarations as above, and it is a larger change.
