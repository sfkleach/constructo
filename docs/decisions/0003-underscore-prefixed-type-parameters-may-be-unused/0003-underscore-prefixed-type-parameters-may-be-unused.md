# 0003 - Underscore-prefixed type parameters may be unused, 2026-10-18

## Issue

**Fork.** Unused type parameters should be an error, but a phantom parameter is sometimes wanted.
`TypeName` must start with `[A-Z]`, so there was no way to mark a parameter as intentionally unused.

## Decision

A type parameter may be written with a leading underscore, `<_T>`, giving the grammar
`TypeParam = '_'? TypeName`. Such a parameter counts towards arity but is exempt from the unused check. Because
`_T` is not a valid `TypeName` it cannot be referred to in the body, so it can never be used. Any other unused
parameter is an error.

## Consequences

This follows the Rust convention, which is familiar to the intended readers. The grammar change is small and is
recorded in `syntax/dt_grammar_tabatkins.txt`.

## Options and Outcome

The chosen option is the one under Decision. Alternatives:

- No exemption (always an error): rules out phantom parameters.
- Make it a warning: warnings are not implemented yet, and hide real mistakes.
- A different marker such as a trailing underscore: unfamiliar and less readable.
