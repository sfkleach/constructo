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

### Done since: keep the generic arguments that the type check solves (`src/query/types.rs` line 96)

Without `--type`, `run_query` (`src/lib.rs`) now checks each value with `check_value_typed` and carries the type that
`Typed::scheme` infers, with its open types unknown, in `QueryInputs::values`. `run_typed` takes `--type` first, then
that type, and only then the type that the constructor tells. So `Pair(1)` of `enum Pair<T> = Pair(T, other: T);` gives
`0` for `p.other`, a part of an `Option<Float>` field is a `Float`, and a bare `[1, 2.5]` is a `[Float]`. A field
that is misspelt in a bare record is still found only when the query reaches it, since the query is checked statically
only against `--type`. The spec says how a value's type is decided.

### Then

- Push `query_subcommand`, and reply to each of CodeRabbit's comments on the PR, saying what was done
  and, for the FIFO comment, that the quick win covered it.
- Move this task folder to `docs/tasks/done/`, as the last commit before the merge.
