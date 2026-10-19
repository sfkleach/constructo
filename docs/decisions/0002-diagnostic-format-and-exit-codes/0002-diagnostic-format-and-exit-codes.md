# 0002 - Diagnostic format and exit codes, 2026-10-18

> Amended by [decision 6](../0006-stop-at-the-first-syntax-error-and-name-the-enclosing-enum/0006-stop-at-the-first-syntax-error-and-name-the-enclosing-enum.md): the run now stops at the first syntax error.

## Issue

**Leap.** Choose the diagnostic format and exit codes for `check`, extending the single example in
`docs/vision.md`.

## Decision

Diagnostics are written to stderr as `ERROR: <message> (<file>, line L, col C)`, with `<stdin>` naming standard
input. A duplicate adds `; first declared at [<file>, ]line L, col C`. The parser reports only the first syntax
error in each file; the post-parse checks report every error, ordered by file, line and column. Nothing is
written on success unless `--verbose` (`-v`) is given, which prints a success message to stdout. Exit code 0
means no errors, 1 means the inputs contain errors, and 2 means a usage or I/O problem such as an unreadable file.

## Consequences

Filenames are needed once several inputs are accepted. The codes let CI tell a bad schema from a bad
invocation. Giving up parser error recovery keeps the parser small: a file with a syntax error shows one error at
a time. An unreadable file stops the run at once with exit code 2, so errors in later files are not reported.
