# 0034 - An import can rename names, 2026-11-08

## Issue

**Fork.** Hiding (decision 33) removes a clashing name, but a project that wants both names of two bundles cannot hide
either of them. It needs to write one of them differently.

## Decision

- An `[[import]]` may have a table `[import.rename]`. Each entry is `NAME_HERE = "NAME_THERE"`: the key is the name that
  the importing project writes, and the value is the name that the imported project declares. The words export and
  import are not used for these, because they read either way.
- Through the import the declared name is not visible and the new name stands for the declaration, bare and with the
  group. A name is renamed in every namespace that has it, as it is hidden.
- The declared name must be declared by the imported project. The new name may not be a name that the project
  declares, unless that name is itself hidden or renamed. A name is not both hidden and renamed, and is renamed once.
  A type or constructor is renamed to a name with a capital letter, and a const to one without.
- The check for clashes of decision 32 is made on the new name, and a message shows a name as the importing file writes
  it.
- The full name of the project is not renamed. `a.b.c::Name` is a declaration of the project.

## Consequences

- A project can import two bundles that share a name into one group and keep both, by renaming one.
- The importing project cannot write the old name bare, which stops one declaration having two names there.
- `T?` is the bare name `Option`, so renaming `Option` of the prelude leaves `T?` an unknown type, as with hiding.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- The table the other way round, `OLD = "NEW"`: the project-file would list the names of the imported project as keys,
  so a key would not be unique to the importing project, and two names could be renamed to one.
- Allowing both names to be written: it would let one declaration have two spellings in one file.
