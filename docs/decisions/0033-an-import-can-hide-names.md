# 0033 - An import can hide names, 2026-11-08

## Issue

**Fork.** Decision 32 forbids two projects of a group to declare one name. The only way out was a different group,
which changes how every name of the project is written. A project that imports a bundle into `local` and wants all of
it but one or two names that clash, such as a `Debug` of a specification it does not use, needs to leave those out.

## Decision

- An `[[import]]` may have `hide`, an array of names of the imported project. A hidden name is not visible through that
  import: bare, or with the group. It does not take part in the check for clashes of decision 32.
- A name is hidden in every namespace that has it. There is no finer form.
- The name must be declared by the imported project, so that a typo is an error and not a silent mistake. The error is
  at the name in the project-file.
- Hiding changes what the importing project sees and nothing else. The imported project's own files are unchanged,
  the canonical name of a declaration is the same, and the full name of the project, `a.b.c::Name`, still reaches a
  hidden declaration since it names the project and not the import.
- Nothing is special about any name. If `Option` of the prelude is hidden, `T?` is the bare name `Option` and is an
  unknown type like any other.

## Consequences

- The clash check of decision 32 is made on the names that are visible, for each import.
- A project can import two bundles that share a name into one group if it hides the name from one of them.
- A hidden name cannot be written bare, so a user who hides a name and then wants it must write the full name.
- Renaming, which is the other way out of a clash, is a separate decision.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Hiding by the full name also, so that the hidden declaration cannot be reached at all: it would make the full name a
  property of the import and not of the project, and a project imported twice would have two answers.
- Hiding a name in one namespace only (`hide = ["type Shape"]`): more exact, but one spelling is rarely both, and the
  simple form is easier to read.
