# Task: Make the type depth limit an option

## Overview

When `constructo check` works out the default value of an omitted argument, it refuses to expand a type that is nested
more than 1,000 levels deep (`MAX_TYPE_DEPTH` in `src/typecheck.rs`). The limit exists because freeing, cloning and
walking a type recurses once per level, and a type whose arguments keep growing, such as
`enum Nest<T> = Nest(next: Nest<[T]>)`, would otherwise be expanded until the node limit and overflow the stack.

The value is fixed and was chosen to stay well inside the smallest stack the checker is likely to meet. It costs about
100 bytes of stack per level in a debug build, so 1,000 levels take about 100 KB. A constant is the wrong long-term
home for it: the safe value depends on the platform and the stack, and a user with an unusual need should be able to
change it without rebuilding.

This task makes it a command-line option and a config file setting, in the same way as `--default-limit`.

## Steps

1. Add `CheckOptions::depth_limit`, defaulting to 1,000, and use it in place of `MAX_TYPE_DEPTH`.
2. Add the option `--depth-limit=N` to `constructo check`, with the layering of the config file task: the built-in
   default, then the `depth-limit` key in the `[check]` table of the config file, then the command line. Include it in
   the output of `--print-config`.
3. Reject a limit of zero, and decide the maximum. A large value brings back the stack overflow, so consider refusing
   one that cannot be honoured, or running the check on a thread whose stack is sized from the limit.
4. Update the error message, which names the limit, and say in the `--depth-limit` help that raising it can overflow
   the stack.
5. Add examples and functional tests for the default, the config file, the option overriding the file, and a limit
   that is too small for a deep type.
6. Record the decision, and update the CHANGELOG.

## Questions to settle

- Should the limit apply only to default values, or to every walk over a type? Today only the default expansion needs
  it, as the nesting limit of the parser bounds the types that are written.
- Is `--depth-limit` the right name, given that `--infer-limit` and `--default-limit` count nodes and this counts
  levels?
- Should the parser's `MAX_DEPTH` of 200 become an option too, or stay fixed?

## Dependencies

This follows the config file task (`docs/tasks/done/2026-10-25-config-file/config-file.md`), which provides the config
file and `--print-config`.
