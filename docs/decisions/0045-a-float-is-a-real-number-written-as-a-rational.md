# 0045 - A Float is a real number, written as a rational, 2026-11-22

## Issue

**Fork.** The values of `Nat`, `Int` and `Float` were written nowhere. For `Nat` and `Int` there was no doubt: they are
whole numbers of any size, and the checker and `query` already treat them so. For `Float` the doubt was real, since
most programmers take a "float" to be IEEE 754 binary64, and the `codegen` task needed to know what a mapping of a
`Float` to a target keeps and what it may give up. It found the question while working through a scenario in
TypeScript, whose numbers are all binary64.

## Factors

- The notation is decimal. A `Float` is written as digits, a point, digits and perhaps an exponent, which is an integer
  over a power of ten. `0.1` is written, and binary64 has no `0.1`.
- IEEE 754 is not a consistent model of numbers. NaN is not equal to itself, which breaks equality; zero has two signs,
  which are equal and yet not the same; and overflow and underflow are handled by more than one policy, while most
  users believe that there is one. A model that inherits these is incomplete, or does not behave as a function of its
  values.
- Real arithmetic has no values for `1/0` and `0/0`. A format for data needs them, since programs produce them and
  write them out.
- Anderson's transreal arithmetic extends the reals with +∞, −∞ and nullity, is consistent with real arithmetic, and
  lines up with IEEE 754's infinities and NaN, except that nullity is equal to itself.
- The hardware is IEEE 754, and every target language holds its floating numbers in binary. A model that no target can
  hold exactly is only useful if the approximation is stated.

## Decision

- **A `Float` is a real number,** extended by +∞, −∞ and nullity, with transreal arithmetic as their theory. Nullity is
  equal to itself. There is no distinct negative zero value: `-0.0` is a correct text, and it is zero.
- **The notation writes only the rationals among the reals,** an integer over a power of ten, and the value of a text is
  that number exactly. Two texts for the same number, such as `1.5` and `1.50`, are the same value.
- **A target may approximate a `Float`** by the nearest value of its binary type, and the loss of precision is accepted.
  This is the one place where a mapping may change a value, and it is a concession to the hardware, made across the
  industry, and not a part of the model. A finite value too large for the binary type is refused, and is not made an
  infinity.
- The text of the values that are not finite is the subject of a task of its own, and is not settled here.

## Consequences

- The specification of type checking says what a number is, and the specifications of the targets of `codegen` say how
  each approximates a `Float`.
- A tool that holds a `Float` in binary, as `query`'s arithmetic does, is approximating, and its specification says so.
  Comparison in `query` is exact on the value, which is the model.
- Exact decimal or rational types in a target are never required, which would be the price of holding the model
  exactly, and which no user would want.
- A target maps a NaN to nullity once the non-finite values have a text, so every NaN, whatever its bits, is the one
  value, and equal to itself.

## Options and Outcome

1. IEEE 754 binary64, as most languages hold a float.
2. IEEE 754 decimal floating point.
3. The reals, written as rationals, with transreal non-finite values. **Chosen.**

## Pros and Cons of Options

### Option 1: binary64

- Pros: what the hardware and nearly every language hold, so a mapping would be exact.
- Cons: it cannot hold the numbers that the notation writes, such as `0.1`, so a text would not mean the number that it
  shows. It brings NaN's inequality, the signed zero and the policies of overflow and underflow into the format.

### Option 2: decimal floating point

- Pros: holds the numbers that the notation writes, up to its precision.
- Cons: it has a precision, so a long literal still has no exact value, and it keeps IEEE's NaN and signed zero. Few
  targets have it, so it would be approximated in the same way as option 3, with a worse model behind it.

### Option 3: the reals, written as rationals

- Pros: every text means the number that it shows, without limit. The arithmetic of the non-finite values is
  consistent. It agrees with binary64 wherever binary64 is exact, so the approximation is the only difference.
- Cons: no target holds it exactly, so the approximation has to be stated, and is.

## Additional Notes

- J. A. D. W. Anderson, N. Völker and A. A. Adams, "Perspex Machine VIII: Axioms of Transreal Arithmetic", *Vision
  Geometry XV*, Proc. SPIE 6499, 2007.
- The task [non-finite-values](../tasks/todo/2026-11-21-non-finite-values/non-finite-values.md) gives the values that
  are not finite their text.
