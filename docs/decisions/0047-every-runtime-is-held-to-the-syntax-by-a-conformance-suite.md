# 0047 - Every runtime is held to the syntax by a conformance suite, 2026-11-24

## Issue

A runtime is the part of Constructo that a program uses to read and write values: a lexer, a parser, a printer and the
limits. For Rust it is `constructo-runtime` ([decision 46](0046-the-syntax-layer-is-a-crate-constructo-runtime.md)),
which is the code that `check` uses. A target in another language, such as Python or TypeScript, needs a runtime
written in that language, so that it installs wherever the language does. Then there is more than one parser of the
syntax, and the claim that every target reads the same syntax has to be made true some other way.

## Factors

- The grammar in `syntax/grammar.md` says what is valid, but not the message and the position of each error, nor the
  edge cases of the lexer where a parser written by hand most often goes wrong: escapes and surrogates, signs and
  exponents, line endings, and how a column is counted.
- The limits are part of the syntax in practice: a runtime reads data that is not trusted, and one that accepts a
  value that another refuses for its depth makes the two disagree.
- A runtime may be in another repository, and should be able to take the suite with it.
- A format of its own for the expected results would be one more thing for every runtime to read.
- A program may want to read Constructo without generated code or types, as it reads JSON into dictionaries and lists.

## Decision

- **Every runtime is held to the syntax by a conformance suite,** in `syntax/conformance/`. A case is a values file and
  its expected result beside it, and a runtime conforms when it gives the expected result for every case. The suite's
  `README.md` says exactly what that is.
- **The expected results are Constructo text, or the line of an error.** The text is the values, written back one to a
  line in the compact form, with each number and name kept as it was written, so the suite tests the syntax and not
  the model of numbers. The line of an error is the message with the line and the column, which counts Unicode scalar
  values. The suite needs no notation but Constructo's.
- **The expected results are made from the Rust runtime,** which is the first to pass the suite and the one that the
  others are held to. A change to the syntax is a change to the expected results, which shows in review.
- **The suite is whole in itself.** It has a copy of every values example of `syntax/examples/values`, kept the same as
  the examples by a test, beside cases for the depth limit and for the lexer. A case in a folder called `experimental`
  is read with the experimental features on, and a runtime that has none skips it.
- **Every runtime makes the tree of a value public:** a function from text to the trees of its values, and one from a
  tree back to text, with no types and no generated code. The suite covers both, since what it expects is what the
  printer writes for what the parser reads.

## Consequences

- The claim that every target reads the same syntax can be checked, by running the suite in each, in place of auditing
  each parser.
- A new values example needs a copy in the suite and an expected result, made with
  `CONSTRUCTO_BLESS=1 cargo test -p constructo-runtime --test conformance`, or the runner fails.
- The suite tests what is read, and the compact text that is written back. It does not test the indented form of the
  printer, which is a matter of layout.
- The tree is a promise in every runtime once it is published, so its shape is looked at before the first release of
  each runtime.

## Options and Outcome

1. One parser for every target, by a binding to the Rust runtime.
2. A runtime in each language, held to the syntax by the grammar alone.
3. A runtime in each language, held to the syntax by a conformance suite. **Chosen.**

## Pros and Cons of Options

### Option 1: one parser by binding

- Pros: the same syntax everywhere by construction.
- Cons: a native build in every language, which does not install everywhere the language does. It can be added later
  beside a runtime in the language, for speed, as Python's `json` has a module in C.

### Option 2: the grammar alone

- Pros: nothing to maintain beyond the grammar.
- Cons: nothing finds a runtime that differs in an error, a limit or an edge case of the lexer.

### Option 3: a conformance suite

- Pros: one suite to audit in place of a parser in each language, and it travels with a runtime.
- Cons: the expected results have to be kept up to date with the syntax, which the runner's checks make hard to forget.
