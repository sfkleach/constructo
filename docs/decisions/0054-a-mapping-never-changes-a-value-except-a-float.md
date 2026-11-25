# 0054 - A mapping never changes a value, except a Float, 2026-11-25

## Issue

A target maps each Constructo type to a type of its language: `Nat` to Rust's `u64`, `Option<T>` to `Option<T>`, a
`Float` to an `f64`. A value that a mapping is given can then end in one of three ways: it is held exactly, and comes
back as the same value; it is refused, with an error at the value; or it is changed into another value without a word.
The question was which of these a mapping may do, and where an idiomatic form of the language may be used in place of
one that is always exact.

This is a **Leap**, from the draft of the codegen task, where the scenario for Python showed that `Option<X>` as
`X | None` loses `Some(None)`. It is written with the Rust target, which applies it to numbers.
[Decision 45](0045-a-float-is-a-real-number-written-as-a-rational.md) says what a `Float` is.

## Factors

- A value that changes without a word is a bug that is found late, if at all.
- A target's types have ranges: a `u64` holds fewer numbers than a `Nat`, which has no limit.
- A form that the declarations contain, such as `Some(None)` of a `Nat??`, is part of the type, and taking it away
  is not the same as limiting a range.
- Hardware floating point is IEEE 754, and a `Float` is a real number written as a rational. Holding it exactly would
  need a decimal or rational type in every target.

## Decision

**A mapping never changes a value, with one exception, the precision of a `Float`.**

**A mapping may refuse a value that is outside the target's range for its type,** when it is read and when it is
written, with an error at the value. The range is the target's, and is documented. In Rust, a `Nat` that does not fit
a `u64` and an `Int` that does not fit an `i64` are refused, as in `The number 18446744073709551616 does not fit Nat`,
and every `u64` and `i64` is written exactly.

**A mapping may not refuse a form that the declarations contain.** Where an idiomatic form of the language cannot hold
one, the faithful form is used. Rust needs no such choice, since its `Option<Option<T>>` is exact.

**A `Float` is held as the nearest value of the target's binary type,** which is the one change that is allowed:

- only precision is excepted, and not range, so a finite `Float` too large for an `f64`, such as `1.0e400`, is refused,
  while one too small to be held other than as zero is zero;
- it is written as the shortest text that reads back as the same binary value, which in Rust is its own shortest text,
  with `.0` put in where it has no point, as in `1.0e300`;
- there is no negative zero, so `-0.0` is read and written as `0.0`;
- a value that is not finite has no text yet, so writing one is an error, until the task for non-finite values gives
  them a text, when +∞ and −∞ map to the infinities and nullity to NaN.

## Consequences

- A value that a Rust program holds goes through text and back unchanged, except a `Float` that is not finite, which
  cannot be written.
- A long literal in a file, such as `0.1000000000000000055511151231257827`, is read as the nearest `f64` and written
  back in its shortest form, `0.1`. The text changes; the `f64` does not.
- The codec's tests compare its output with `convert`'s, which keeps the text of a number as it was written, so the
  numbers of the corpus are written in their shortest form.
