# 0006 - Stop at the first syntax error and name the enclosing enum, 2026-10-19

## Issue

**Fork.** Step 4 of the `constructo check` task says that when a types file cannot be parsed, checking stops and the
error, the associated enum and the location are reported. Decision 2 had each file report its own first syntax
error, so one run could report several syntax errors.

## Decision

Files are parsed in command-line order and the run stops at the first syntax error. That error is the only
diagnostic, with or without `--verbose`. Where the name of the enum being parsed is known, it is added to the
diagnostic as `; in enum <Name>`. It is not known when the error is in the `enum` keyword or in the type name itself.

This supersedes the "first syntax error in each file" part of decision 2. Its format, exit codes and the
reporting of every validity error are unchanged.

## Consequences

- A user with syntax errors in several files fixes and reruns once per file.
- Inputs are read one at a time and reading stops at the first syntax error, so an unreadable later input cannot
  hide it: a syntax error in an earlier input gives exit code 1, whereas an unreadable earlier input still gives 2.
- The behaviour is the same with and without `--verbose`, so the flag only adds commentary.
- The library reports a per-source `Outcome` (`Ok`, `Failed` or `NotChecked`). Verbose mode lists `OK:` and
  `FAILED:` lines for the user's inputs only, never for the prelude, and ends with `OK: checked N file(s)` on
  success. Files that were not reached are not listed.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- Stop only in verbose mode: behaviour would differ by flag.
- Keep reporting one syntax error per file: simpler to automate over many files, but not what the task asks for.
