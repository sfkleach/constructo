# Remaining work: CodeRabbit's review of PR 23

CodeRabbit reviewed [PR 23](https://github.com/sfkleach/constructo/pull/23) in
[review 5911300372](https://github.com/sfkleach/constructo/pull/23#pullrequestreview-5911300372): 14 inline comments
and 11 minor ones in the summary. This file records where the work on them stopped, on 2026-11-15.

## Done

These commits are on `query_subcommand`. They are committed locally and **not pushed**, and **no reply has been
posted** on the PR.

- `d1267b8` All the quick wins: 9 of the inline comments and the 11 minor ones.
- `8364519` Heavy lift: `update` checks the parts of a typed new value (array elements, record fields, constructor
  bodies), at the value and, with `--type`, before the query runs.
- `d90fb31` Heavy lift: the links of operator, postfix and cast chains count towards the nesting limit.
- `e2fe924` Heavy lift: the conditions of `if` and `match` are memoised, so that nesting them is not exponential.

The heavy lift about a FIFO among the inputs (`src/main.rs` line 683) needed no work of its own: the quick win for
non-seekable inputs fixed it, since the query is now checked before any input is looked at, and only a regular file is
probed for being a types file.

## To do

### Keep the generic arguments that the type check solves (`src/query/types.rs` line 96)

With `enum Pair<T> = Pair(T, other: T);` and the input `Pair(1)`, the check infers `T = Nat`. Without `--type`, a typed
query then rebuilds the value's type from its constructor alone, as `Pair<?>`, so `@values.map(|p| p.other)` fails in
`default_value` instead of giving `0`.

The recommended fix, which was proposed but not yet agreed:

1. In `run_query` (`src/lib.rs`), load each value with `check_value_typed`, as `convert --to=json-typed` does, instead
   of `check_value`, and take its type from `Typed::scheme(value, infer_limit)`, with the open types (`Ty::Gen`) filled
   with `query::types::unknown()`.
2. Carry that type in `QueryInputs::values`, as `(usize, Value, Option<Ty>)`, and use it in `run_typed` in place of
   `types.type_of(&value)`. `--type`, when given, still wins.
3. Add a test that `Pair(1)` gives `0` for `p.other`, and one that a part of an `Option<Float>` field is a `Float`.

The alternative, inferring the arguments again in the query from the value's parts, was rejected as duplicating the
type checker. The cost of the fix is that `check_value_typed` records the type of every part of each value while it
checks it, which is freed after each value.

### Then

- Run `just test`, push `query_subcommand`, and reply to each of CodeRabbit's comments on the PR, saying what was done
  and, for the FIFO comment, that the quick win covered it.
