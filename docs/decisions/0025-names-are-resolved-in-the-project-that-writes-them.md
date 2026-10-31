# 0025 - Names are resolved in the project that writes them, 2026-11-01

## Issue

**Fork.** Until now every declaration of every input was in one namespace. With projects (decision 24) and imports, two
projects can both declare a `Shape`, and a project should be able to say which one it means. We have to decide what a
name refers to, which projects a project can name, how a clash is settled, and how a name is shown in a message.

## Decision

- A name is written bare (`Foo`), with a group (`bundle::Foo`) or with the full name of a project
  (`com.example.shapes::Foo`). A bare `Foo` is `local::Foo`. A declaration is never qualified.
- The group `local` holds the project itself, the projects that it imports with `group = "local"`, and the prelude.
  This is how a project brings the names of another into its own namespace, and how a bundle of namespaces is built.
  Another group holds the projects that were imported into it. A full name refers to the project itself or to one that
  it imports.
- Imports are not transitive. If A imports B and B imports C, C is loaded and checked because B's declarations may name
  it, but A can name only itself and B, and has to import C to name it. Inherited imports are for a later task.
- A name is resolved in the namespace it is used in, which is types, or constructors and consts together
  (decision 4), in the project of the file that it is written in. There must be exactly one declaration. If the
  projects in `local` or in a group declare it more than once, it is an error at the use, naming the candidates, as in
  `The name Middle is ambiguous: it could be example.a::Middle or example.b::Middle`. Declaring a name that another
  project in `local` has is not an error, so a bundle may grow a name without breaking its importers. An unknown group
  or project, and a name that a group does not have, are errors at the use. A bare name that nothing declares is left
  for the checks that follow to report as unknown, as before. The basic types are always bare and are never qualified.
- The files given on the command line with `--project` are written in the root project, as if they were among its own
  files, and the values are checked in its context. Files given without a project are a project with no name and no
  imports, apart from the prelude, so `local::Foo` works for them and no other qualifier does.
- Inside the checker a declaration is known by a canonical name, the local name followed by `@` and the full name of
  its project. A reference is rewritten to the canonical name of what it refers to, and then the validity checks and the
  type-check work as they always did. The prelude and the files without a project keep bare names, so nothing about
  them changes.
- A name in a message is shown as it would be written in the file that the message is about: bare if it resolves to the
  same declaration there, and otherwise in full, as `com.example.shapes::Shape`. So two different types are never
  shown the same, and the messages of a plain `constructo check FILES` are unchanged.
- A name that cannot be resolved in a types file stops the validity checks, as a syntax error does, because the names
  can no longer be relied on. The names in a values file are only resolved and reported when the values are
  type-checked.
- The prelude was a stopgap when this was written: it was in `local` of every project, and a project that declared a
  name that the prelude has got the duplicate error at the declaration. Decision 26 makes it a project that can be
  overridden, and then a clash is an ambiguity at each use, as for any project in `local`.
- A const of another project is a reference to a const, so it needs `--allow-refs` like any other.

## Consequences

- Two projects can use the same names without coordinating, and a project says in its project-file which others it
  uses. A project that wants the old behaviour of a flat namespace imports the others into `local`.
- A project that uses a type of another in its own declarations, as `lib::Shape`, can be affected if that project
  renames the type, which is the same as it is for any dependency.
- The canonical names mean the checker's own diagnostics must be converted for display. This is done once, where the
  diagnostics are collected, so a new message needs nothing special.
- Importing two projects into `local` that declare the same name is fine until someone uses it unqualified.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A bare name that searches every group. It is the easiest to write, but a name can start to fail when an unrelated
  import adds a clash.
- The project's own declarations winning over imported ones. It works like local variables, but it hides a clash that
  the author may not know about. The prelude is overridden by another mechanism, which is the next step.
- Reporting a clash where the name is declared. It catches a clash early, but a bundle that gains a name can then break
  every project that imports it.
- Making imports transitive. A project would then depend on names that it does not list, which disappear silently
  when an import stops importing them.
