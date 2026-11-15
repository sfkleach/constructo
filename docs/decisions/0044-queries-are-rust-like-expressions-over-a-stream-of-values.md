# 0044 - Queries are Rust-like expressions over a stream of values, 2026-11-15

## Issue

**Fork.** `docs/vision.md` lists, for later, "selecting sub-values from a file or stdin, which needs a path notation".
Exploring it showed that selecting and transforming are one problem: pulling a field out of each value and changing one
field while keeping the rest need the same paths. The decision is what kind of language a user writes to extract data
from a stream of values and to transform it.

This record settles the shape of the language. The command is `query`. Its command line and the remaining details of
the language are settled in the implementation task, `docs/tasks/2026-11-15-query-subcommand/query-subcommand.md`.

## Factors

- **Familiarity.** The language should be readable by people who have not learned it, and guessable by those who know
  a mainstream language. Constructo is already influenced by Rust's `enum`.
- **Constructo's values are richer than JSON.** They have constructors with record, tuple (positional and named) or
  array bodies, literals whose text decides their kind (`5`, `+5`, `5.0`), consts, and a type for every value. A path
  needs steps for all of these.
- **Types are optional in practice.** It is hugely helpful to query a file without knowing its types, but the types
  can make a query safer than any untyped tool can.
- **Change one thing, keep the rest.** Most real edits are of this kind, and doing them by rebuilding a value is where
  mistakes are made.
- **Literal text survives.** As `convert` guarantees, a value that is not changed keeps its text.

## Decision

**The language is Rust-like, not Rust.** It takes Rust's form (closures, method chains, iterator adaptors, patterns,
`as`, struct update) and gives it Constructo's types and meanings. Where we deviate from Rust, the deviation has a name
of its own, so a Rust programmer is never misled by a familiar name with a new meaning.

### The stream

- **The whole input is one lazy iterator, `#values`.** The values of every input, in order, as for `check` and
  `convert`. A query is an expression over it: `#values.map(|m| m.x)`, `#values.take(1)`, `#values.count()`.
- **The output rule:** if the result is an iterator, each item is written as a value; otherwise the one result is
  written. So `#values.take(0)` writes nothing and `#values.count()` writes one number.
- **Laziness** lets `#values.take(1)` stop reading early, as `convert` streams lines input.
- **The name** is `#values` until something forces a change: a sigil cannot clash with a const (lower case) or a
  constructor (capitalised), `#` is not expanded inside a shell's quotes as `$` is, and `values` is Constructo's own
  word for what the stream holds. The sigil was `@` at first, but `@` is also the binding of Rust's patterns,
  `p @ Poly[..]`, and the lexer took `p@Poly` for the sigil `@Poly`, which Rust reads as a binding. `#` has no
  meaning in Rust's expressions or patterns, where it only starts an attribute, nor in a values file, so the stream
  and the binding no longer share a character, and the spacing of a binding does not matter.
- **Rust's adaptors keep their meanings:** `map`, `filter`, `filter_map`, `flat_map`, `take`, `skip`, `nth`, `next`,
  `count`, and so on. `next()` and `nth(n)` give an `Option` (`Some(v)` or `None`, from the prelude), while
  `take`/`skip` give the bare value or nothing. `.collect()` always makes an array, since Constructo has one sequence
  type, and `#values.collect()` gathers the whole stream into one value.

### Reaching inside values

- Records, bare or as a constructor's body: `v.x`. Tuples: `v.0` by position, `v.end` by name. Arrays, bare or as a
  body: `v[i]`, which is an error when out of range, `v.get(i)` and `v.first()`, which give an `Option`, `v.len()`,
  and `v.iter()`, since an array is not an iterator, as in Rust.
- A missing field or argument, or the wrong shape (`Quit has no body`), is an error at the value.
- Constructor tests use Rust's `match` and `matches!`, with patterns written in Constructo's value syntax
  plus `..`, `_` and bindings.
- **`filter_match`** is our addition for the commonest extraction: its closure parameter is a refutable pattern, a
  value that does not match is dropped, and the body's result is kept: `#values.filter_match(|Move { x, .. }| x)`.
  In every Rust-named method a closure parameter must always match, as in Rust, so `filter_map` keeps exactly Rust's
  meaning.
- **Braces need only ever denote records.** It is always possible to write a query in which braces denote only
  records, bare or after a constructor, and never blocks. Constructo's records are anonymous, so `{ x: 1 }` is the
  value syntax; Rust can read a bare `{` as a block only because its struct literals are named. There are no blocks,
  and patterns bind names, so `let` is not needed.
- **`if` and `match` have outfix forms,** `if TEST then A elseif TEST then B else C endif` and
  `match EXPR with ARMS endmatch`, in which every brace is a record. Rust's braced forms, `if TEST { A } else { C }`
  and `match EXPR { ARMS }`, are accepted too, so a Rust programmer's first guess works, but no one is forced to use
  braces for anything but records. A new construct with a braced Rust form needs an outfix form as well.
- **Field shorthand only after a constructor name,** as in Rust: `Move { x, y }` is `Move { x: x, y: y }`, and a bare
  record names its fields, `{ x: x }`. A Rust programmer who writes `|m| { m }` out of habit gets an error, not a
  record that quietly wraps the value. Patterns keep Rust's shorthand, as in `Move { x, .. }`.
- There are no anonymous tuples, as Constructo has none: a pair of fields is a record.

### Types are optional: two modes

- **Untyped** means only that there are no types: the query works on the values alone, since they describe their own
  structure. The expression is still type-checked. What it fixes is checked before any data is read, so
  `if v.ok then 1 else "failed" endif` is refused; what comes from the data is checked as each value arrives, by its
  kind (`Nat`, `Int`, `Float`, `String`, record, array or constructor), and a mismatch is an error at the value.
- **Typed,** with `--type` or a project, the query is checked before any data is read (`m.z` is refused up front),
  omitted named arguments take their defaults, references to consts are resolved, names are resolved to their
  constructors, and the type of the result is known.
- **The principle:** checked and unchecked processing give the same result whenever both succeed. Either may fail
  where the other succeeds: the types find errors that the values alone cannot show, such as mixing constructors of
  different enums, and supply what the text leaves out, such as defaults, consts and widening. Where the text alone
  cannot answer, the untyped mode reports an error that says to supply the types, and does not guess. For example,
  untyped `v.0` is the first argument written by position and typed `v.0` is the first parameter however it was
  written; positional arguments bind a prefix of the parameters, so the two always agree when both answer.
- **Untyped, constructor names match by their exact text:** the same text is the same constructor and different text
  a different one. Unchecked processing is obliged to assume the distinctions that the text shows, so the principle
  is strictly true only when the types agree with them. They may not: `Circle` and `shapes::Circle`, or a name and its
  renaming in an import, can be one constructor, and then supplying the types gives the right answer.

### Numbers

- **Strict, as in Rust.** Operands of a comparison or of arithmetic must be of the same kind. Literals adapt to their
  context (`m.count + 1`, `m.y > 0.0` for a `Float`), but values do not: a `Nat` compared with a `Float` needs
  `m.y as Float > 0.0`. When a source writes `5` where `5.0` is meant, the cast records that the reader knows it.
- **`as` names a Constructo type:** `as Nat`, `as Int`, `as Float`. Rust's `as f64` or `as u64` would suggest
  machine sizes, and Constructo's integers are unbounded. A cast to the same kind does nothing, so a cast written to be
  safe works on every spelling of a field. A narrowing cast that would change the number, such as `-1 as Nat` or
  `1.5 as Int`, is an error at the value.
- In typed mode a `5` in a `Float` field is already a `Float`, by the checker's widening, so it needs no cast there.
- **Arithmetic:** the result has the operands' kind. `Nat` and `Int` are unbounded, so nothing overflows. A `Nat` below
  zero is an error, so cast to `Int` to go negative. Integer `/` truncates and `%` is the remainder, as in Rust, and
  dividing by zero is an error. `Float` arithmetic is `f64`: programmers expect floating point to be inexact, and exact
  decimals would cost a great deal for a benefit few users would notice. Converting a literal too large for `f64`
  (`1e400`) is an error, not an infinity. Untouched literals still compare exactly on their text.
- **The text of a new number:** a `Nat` is plain digits; an `Int` always has a sign (`+8081`, `-3`); a `Float` is the
  shortest text that reads back as the same `f64`, always with a `.` or an exponent (`3.0`, `1.5e300`), so each reads
  back as its own kind.

### Transformation

- **One language serves extraction and transformation.** The two were a soft distinction, made to focus the design.
- **Struct update keeps the rest of a record:** `#values.map(|c| Config { port: 8080, ..c })`, and `{ port: 8080, ..r }`
  for a bare record.
- **`update` is copy and assign.** Rust's idiom is `let mut c = c.clone(); c.server.tls.port = 1; c`, but it needs
  blocks and statements. Our `update` method takes a closure that assigns to a path and returns the changed value:
  `c.update(|c| c.server.tls.port = 1)`. Values are immutable and cannot be aliased, so the copy is conceptual, and an
  implementation shares the parts that do not change. Several changes chain:
  `c.update(|c| c.a = 1).update(|c| c.b.c = 2)`.
- Anything the access syntax can reach can be assigned. A missing field, an index out of range or the wrong shape is
  an error: up front when typed, at the value when untyped. In typed mode the new value must fit the type at the path,
  so a transformed stream stays valid against the types. Compound assignment (`+=` and the rest) follows the rules
  for arithmetic.
- **Deleting a field is not an operation.** JSON needs it because objects are used as maps, and Constructo keeps maps
  and records apart. An element of an array is removed with `v.iter().filter(...).collect()`. How maps are modelled
  is a separate design task, `docs/tasks/todo/2026-11-14-model-maps/model-maps.md`.

## Consequences

- A Rust programmer can read a query without learning a new language, and anyone who knows a language with closures
  and method chains can guess it. The deviations (`#values`, `filter_match`, `update`, `{` for records, the outfix
  `if` and `match`, `as` with Constructo's types) are few and named.
- Queries can be checked against the types before reading any data, and a typed transformation cannot produce a value
  that the types refuse. No untyped tool can offer that.
- The same query works with or without the types, and where both succeed they agree, given types that agree with the
  distinctions the text shows.
- Strict numbers mean that, untyped, a field that is spelled `5` in one value and `5.0` in another fails only when the
  odd value is reached, which may be late. The message suggests the cast, and a cast written to be safe avoids it.
- Expressions are longer than jq's for the simplest cases (`#values.map(|m| m.x)` against `.x`), in exchange for
  readability beyond them.
- Deep changes are written with `update` and a path. jq's ability to update every location that a predicate selects,
  such as every number anywhere, has no direct equivalent yet.

### Still open

The command line, casts, `match`, the operators and the string methods are settled in the implementation task, and
the command is specified in `docs/specs/query-command.md`, with its exit codes. Left open:
- A way to update every location that a predicate selects, as jq's `|=` does, if real use shows the need.

## Options and Outcome

1. A jq-like language for Constructo values.
2. Fixed verbs on paths, such as `get`, `set` and `delete`, with no language.
3. Patterns and templates in Constructo's own syntax.
4. A small query language with named variables and comprehensions, in the style of SQL or XQuery's FLWOR.
5. A library only, leaving scripting to general-purpose languages.
6. **Rust-like expressions over a stream of values. Chosen.**

## Pros and Cons of Options

### Option 1: a jq-like language

- Pros: jq is powerful. Paths are values, and `|=` changes every selected location while keeping the rest.
- Cons: its point-free style, in which every filter is a generator and nothing is named, is as unpleasant to read as
  `awk`, and few people have jq skills to carry over. It has no types, and its errors about data have no positions.
- Interesting: its two best ideas, paths as values and changing in place while keeping the rest, are kept here in
  another form (access paths, struct update and `update`).

### Option 2: fixed verbs on paths

- Pros: no language to learn.
- Cons: it covers only the simplest cases, and anything else has to leave the tool.

### Option 3: patterns and templates

- Pros: it reuses the value grammar, and patterns survive here inside `match`, `matches!` and `filter_match`.
- Cons: on its own it cannot express iteration, counting or arithmetic.

### Option 4: a small query language

- Pros: the familiarity of SQL, and named variables throughout.
- Cons: a new grammar to design and to learn, and less familiar to programmers than method chains.

### Option 5: a library only

- Pros: no language to design.
- Cons: it does not answer the command-line use, and `convert` to JSON already offers jq to those who want it.

### Option 6: Rust-like expressions

- Pros: familiar, every value named, typed when the types are given, and consistent with Constructo's existing
  influence.
- Cons: more words than jq for the simplest queries, and the places where it deviates from Rust must be learned.
