# 0031 - A declaration that clashes with its own scope is a duplicate, 2026-11-08

> Amended by [decision 32](0032-the-projects-of-a-group-share-no-names.md): two imports in one group that declare a name are a duplicate where it is declared, and a name is never ambiguous at its use.

## Issue

**Fork.** Decisions 25 and 26 made it legal for a project to declare a name that a project in its `local` group has,
and in particular a name of the prelude, and reported an ambiguity at each bare use. That hides the mistake until the
name is used, and a declaration that is never used is never reported. Redeclaring `Bool` or `True` was an error in
decision 5, and it should stay one.

## Decision

- The scope of a project is the project itself and the projects in its group `local`, the prelude among them unless it
  has been imported elsewhere. A declaration of a project that has the name of a declaration in another project of its
  scope, in the same namespace, is a duplicate. It is an error at the declaration, in the form of any other duplicate:
  `Duplicate type Bool (app.types, line 1, col 6); first declared at constructo:/prelude/prelude.types, line 1, col 6`.
  The namespaces are those of decision 4: types, and constructors with consts together.
- A use of such a name means the project's own declaration, so the mistake is reported once, where it was made, and not
  again at each use.
- An ambiguity is now only for a name that two or more projects of one group declare, none of them the project itself.
  That happens when two imports share a group name, and it is an error at the use, as in decision 25. Declaring a name
  that such a group has is no error, since the group is not the project's own scope, and neither is importing two
  projects that happen to share a name.
- The override of decision 26 is unchanged. A project that imports the prelude into another group, such as `std`, has
  its own `Bool` and the prelude's is `std::Bool`, and there is no clash because they are not in one scope.
  `--no-prelude` does the same for a run with no project-file.

## Consequences

- If a run reports no error, every bare name in it refers to exactly one declaration. A name that the project and an
  import into `local` both declare is a duplicate, and one that two imports into `local` declare is an ambiguity at its
  use. Either way it is an error, so it never resolves silently to one of several.

- A project that declares a name of the prelude, and a project that declares a name of a project it imports into
  `local`, now fails with exit code 1 even when the name is never used. The way out is the group.
- A bundle that gains a name can break a project that imports it into `local` and declares the same name. A project that
  wants to stay apart imports the bundle into a group of its own.
- The checks after name resolution, the validity and type checks, do not run when this error is found, as for any other
  error in a name.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Keeping the ambiguity for the project's own declarations (decisions 25 and 26): an unused declaration was never
  reported, and a user who redeclared `Bool` was told only when they used it.
- Making every clash in a group a duplicate, whoever imports it: two imports would then fail on a name that nobody uses,
  and a project could not import two bundles that share a name without choosing between them.
