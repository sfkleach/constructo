# Task: Limit references between consts

## Overview

Step 1 of the consts task (`docs/tasks/done/2026-10-20-consts/consts.md`) lets the definition of a `const` refer to
another `const` only if `--allow-refs` is set. This task replaces that flag, first with a simple rule about "leaf"
consts and later with a limit on the size of the serialised value.

The reason for restricting references is that a chain of references can make the expansion of a const very much larger
than its text. A definition such as `const B = [A, A]; const C = [B, B]; ...` doubles in size at each level.

## Step 1: Allow references to leaf consts and rename the flag

This is a holding position while the size-based limit in Step 2 is designed.

A *leaf* const is a const whose value contains no references to consts. The rule is:

- A `const` definition may refer to a leaf const without any option. This covers the consts of the standard prelude
  (`null`, `true` and `false`), which are all leaves.
- A `const` definition may refer to a non-leaf const only if `--allow-non-leaf-refs` is set. This replaces
  `--allow-refs`, which is removed.
- Values files are unaffected: they may refer to any const.
- The other rules are unchanged: with the flag the order does not matter, a circular definition is an error, and a
  reference has the type declared in the definition of its const.

Notes:

- A reference to a leaf const is allowed whatever its position, so `const Total: Nat = Base;` is valid when `Base`
  is `const Base: Nat = 10;`. That makes `syntax/examples/types/parses-but-invalid/const-reference-needs-flag.types`
  valid, and it needs a chain of two references to illustrate the flag.
- A const is a leaf by what its own value says, not by what it refers to indirectly, so a leaf has no dependencies and
  cannot be part of a cycle.
- Update the examples, the functional tests, decision 13 and the CHANGELOG, and remove the exemption for prelude consts
  that Step 2 of the consts task describes, as it is no longer needed.

## Step 2: Replace the flag with a limit on the serialised size

Calculate the size that the value of a const would have when serialised with its references expanded. A reference is
allowed as long as the size stays below a threshold.

To be refined, including:

- What is measured: characters, bytes or nodes of the value, and whether the size is that of the expanded text.
- The default threshold, and an option to change it, which replaces `--allow-non-leaf-refs`.
- How the size is calculated without expanding the value, since the point is to reject a definition before it grows.
- The diagnostic, which should state the size and the limit.
- Whether the same limit applies to the consts that values files refer to.
