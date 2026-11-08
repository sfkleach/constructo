# 0032 - The projects of a group share no names, 2026-11-08

## Issue

**Fork.** Decision 31 left one ambiguity: two imports into one group that declare the same name. It was an error only at
a use, so a clash in a group that nothing names went unreported, and the same clash was reported once for each use. It
is also the one case in which a bare name could still be an error after the declarations were all valid. The owner of
the project wants every clash to be found when the projects are loaded.

## Decision

- The projects of a group, and the project itself for the group `local`, may not declare the same name in the same
  namespace. The namespaces are those of decision 4: types, and constructors with consts together. This holds for every
  group of every project of the run, not only for the root.
- A clash is an error at the later declaration, in the form of any other duplicate, with a note that names the first:
  `Duplicate constructor Middle (b.types, line 1, col 10); first declared at a.types, line 1, col 10`. The projects of
  a group are taken in the order of the imports, and the project itself is taken last, so that a clash with an import is
  reported in the project and not in the import. Each declaration is reported once, however many groups it is in.
- Nothing is reported at a use. There is no message that a name is ambiguous, because by the time names are looked up
  every name in a group has one declaration, or the run has failed.
- A name may still be declared by two projects that are in different groups. They are told apart by the group, as in
  `a::Middle` and `b::Middle`.
- The way out of a clash is to put the projects into different groups. Renaming and excluding names on import will
  give another way, and are not part of this decision.

## Consequences

- If a run has no error, every name written in it, bare or qualified, refers to exactly one declaration.
- Two bundles that share a name cannot be imported into one group, even if no one uses the name. A bundle that gains a
  name can break a project that imports it into a group with another that has the name.
- The error is in a file of the project that was imported when the clash is between two imports, so the user of the
  project may be unable to correct it. Until names can be renamed or excluded the answer is the group.
- This replaces the ambiguity of decisions 25 and 26 and the last paragraph of decision 31 on ambiguity at a use. The
  rule of decision 31 on the project's own declarations stands, and is now a case of this one.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Keeping the ambiguity at the use (decision 31): the mistake is hidden until the name is used, and then reported at
  each use.
- Reporting a clash at the import: the import is where the choice was made, but the position of a declaration is the
  position that every other duplicate has, and the files of the imports have no position of that kind to show.
