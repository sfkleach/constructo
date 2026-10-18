# 0001 - All inputs to check share one namespace, 2026-10-18

## Issue

**Fork.** When `constructo check` is given several files (and/or `--stdin`), is each checked independently or do
they form one schema?

## Decision

All inputs form one namespace. A type declared in one input can be used in another, and duplicate types or
constructors across inputs are errors. Declaration order is irrelevant. If any input has a syntax error the
post-parse checks are skipped entirely, since the unparsed input may hold declarations that the others refer to
and checking without it would report spurious unknown names.

## Consequences

This matches the intended use (a schema split across files, later a prelude) and the task's wording
"between declarations". The cost is that a syntax error hides semantic errors until it is fixed.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Each file independent: simple, but a type cannot refer to one in another file.
