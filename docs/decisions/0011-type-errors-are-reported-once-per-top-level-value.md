# 0011 - Type errors are reported once per top-level value, 2026-10-20

## Issue

**Fork.** We must decide what a type error points at and what `--continue` skips past.

## Decision

One error is reported per top-level value, at the constructor (or literal) of the innermost failing term, saying what
was expected and what was found. By default the first type error stops the run. With `--continue`, checking resumes
at the next top-level value, including in later files.

A syntax error still stops the run, since the parser does not recover (decision 6). Type errors make the exit code 1.
`--no-type-check` skips this step and leaves only the well-formedness check.

## Consequences

- Several independent faults in one value are found over several runs.
- Reporting only the innermost term avoids cascades of follow-on errors from one mistake.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Report all failing sub-terms of a value: more output, with a risk of cascades.
- Report at the outer term: further from the actual fault.
- Make `--continue` pass syntax errors: needs parser recovery, which was ruled out.
