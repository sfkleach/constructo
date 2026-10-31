# 0023 - Types files come before values files, 2026-10-31

## Issue

**Fork.** Until now `constructo check` accepted its inputs in any order. A values file saw only the types files that
came before it, but a types file could follow a values file, and was then used by the later values only. Projects
(see the projects task) hand the checker all of a project's types files and then the values to check, so the order of
the two kinds is about to be fixed by the caller. We have to decide whether the mixed order stays legal.

## Decision

- Every types file must come before every values file. A types file that follows a values file is an error:
  `A types file must come before the values files, but this follows FILE`, reported at line 1, column 1 of the types
  file, naming the first values file. The exit code is 1, as for other errors in the inputs, and the error is reported
  whether or not the values are type-checked.
- A types file with no declarations, such as one with only comments or an empty one, is not an error wherever it is,
  because it cannot change what any values file sees.
- The order of the types files among themselves still does not matter (decision 1), and neither does the order of the
  declarations in a file.
- Because of this rule a values file always sees all of the types, so the type-checker no longer builds a separate
  scope for each values file.

## Consequences

- Command lines that put a values file ahead of a types file, which used to fail anyway whenever the values used the
  types, now fail with a message that says why.
- A library caller must order its sources: the types files first, and then the values.
- The rule is what a project needs: the project's types files are one group, and the values are checked against all
  of them.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Keep the mixed order and give each values file the types that precede it. It works, but the order is then
  meaningful for values and not for types, which is hard to explain and does not suit a project whose types are
  collected from a folder.
- Quietly reorder the inputs so the types come first. That hides a mistake on the command line, and a file's position
  would no longer match the order in which its diagnostics appear.
