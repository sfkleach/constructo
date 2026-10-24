# 0017 - Tuple constructors may have named parameters, 2026-10-23

## Issue

**Fork.** The parameters of a tuple constructor are only identified by position, so a constructor with many parameters
of the same type, such as `Image`, is hard to read at the point of use. We have to decide how parameters get names, how
arguments refer to them, and what the names mean to the type system.

## Decision

- A parameter may be written `name: Type`. Unnamed parameters come first, so `Foo(x: Nat, Nat)` is a syntax error.
  An unnamed parameter can only be bound by position, and a named one by position or by name.
- A parameter name has the shape of a field name, and `const` stays reserved. Names are per constructor, so they
  cannot clash with consts, constructors or types, but a duplicate within one constructor is an error.
- An argument may be written `name: value`. Positional arguments come first and a positional argument after a named
  one is a syntax error. Named arguments may be in any order.
- A named argument must name a parameter of the constructor, and each parameter can be bound only once, whether by
  position or by name. For now every parameter must be supplied. A missing named parameter is reported as
  `Missing argument for <name>`, and any other shortfall keeps the existing count message.
- The parser tells a name from a const reference by looking one token ahead for the colon, as a lowercase name
  followed by a colon cannot otherwise appear in an argument list.
- Parameter names are not part of the type. They only decide which parameter an argument is for, so widening and
  compatibility are unchanged. They do appear in diagnostics: a value of the wrong type for a named parameter
  gives `Expected Nat for argument hOffset but found String`.

## Consequences

- Existing positional constructors are unaffected.
- The order of the parameters still matters, because arguments may be given by position.
- A constructor can have more than one way to be written, so tools that compare values textually must match
  arguments to parameters first.
- Making named arguments optional is a separate step.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Requiring named parameters to be given in declaration order: this is stricter, but the argument list no longer
  lets the author group related arguments together.
- Making names part of the type: two constructors that differ only in their names would be incompatible, which
  would add a rule and no safety, as constructors are already distinguished by their tags.
