# Task: Fuzz testing

## Overview

Check that `constructo check` never panics, overflows the stack, or runs for a long time, whatever it is given, and
that its outcome changes in the right way when the input or an option does. The tests are on the library
(`check_sources_with`, `parse_file`, `Config::parse`) and not on the binary, so that an input costs a function call and
not a process.

Most of the bugs so far were of this kind: the nesting-limit bypass by a postfix `?`, the default that was expanded
1000 levels deep, and the hash that detects a recursive default. Each was found by a reviewer who read the code.

## Design points

- **Proptest first.** `proptest` runs under `cargo test` on stable Rust, shrinks a failure to a small input, and needs
  no extra tooling, so it is the part that runs in CI on every change. The case count is small by default and is raised
  with `PROPTEST_CASES` for a longer run.
- **A small stack.** Each input is checked on a thread whose stack is smaller than the usual one, so that recursion
  which is too deep fails here and not on a user's machine. A stack overflow aborts the process, so proptest cannot
  shrink it. To find the input, bisect with the binary and `ulimit -s`.
- **Three kinds of input.**
  1. The examples in `syntax/examples/`, mutated by deleting, duplicating and inserting snippets.
  2. Text generated from the grammar, which gets much further into the checker.
  3. Text that is nested to a chosen depth in each of the places that nest.
- **Metamorphic properties.** These find wrong answers without an oracle. The properties are: reordering declarations
  does not change the outcome (decision 1); and switching on widening, the experiments, `--allow-refs` or larger limits,
  or switching off the type-check, never turns a success into a failure.
- **`cargo-fuzz` later.** It needs nightly Rust, so it is for an occasional longer run and is not in CI.

## What the first run found

A first version of `tests/robustness.rs` was run for about 100,000 mutated examples, 250,000 generated texts, 20,000
cases of each metamorphic property, and 20,000 nested texts. There was no panic, hang or broken property. It found:

- **Stack use at the nesting limit.** The parser uses about 6 KB of stack for each level of nesting in a debug build,
  so the limit of 200 levels needs 1.2 to 1.5 MB. A release build needs about 256 KB. Linux gives the main thread 8 MB,
  but Windows gives it 1 MB, and a thread that is started with the Rust default has 2 MB. The nesting that overflows a
  1 MB stack is `A(A(A(...` and `{ x: { x: ...`, 199 levels deep. On Windows a debug build can be crashed by a
  small file. Either the frames are made smaller, or the parser runs on a thread with a stack of its own, or the limit
  is lower.
- **The generated text is mostly rejected.** About 2% of it is accepted, so the metamorphic properties are exercised
  less than they look. Generating a value from the type it is for would fix that.

## Step 1: The harness

Land `tests/robustness.rs` and the `proptest` dev-dependency, with the properties above and a default case count that
keeps `cargo test` under about ten seconds.

## Step 2: Stack use

Decide how to deal with the stack use at the nesting limit, and add a test that fails if the stack needed for 200 levels
grows. Decision record if the limit or the design changes.

## Step 3: Type-directed generation

Generate values from the types that are declared, so that most of the input is accepted and the checker is exercised
past its first error. Add properties that need accepted input, such as `--continue` never changing the first error.

## Step 4: Config files

Add `Config::parse` to the harness, for the slicing of the text at the position of an error and for the keys that are
not known.

## Step 5: `cargo-fuzz`

Add `cargo-fuzz` targets for `parse_file` and `check_sources_with`, with the corpus from `syntax/examples/`, and say how
to run them in the developer documentation. This is not run in CI.

## Step 99: CHANGELOG

Bring the CHANGELOG up to date if anything that users see has changed.
