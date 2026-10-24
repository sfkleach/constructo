# 0018 - An omitted named argument takes the default of its type, 2026-10-24

## Issue

**Fork.** A constructor with many named parameters, such as `Image`, is tedious to write out when most of the values
are the obvious ones. Named arguments should be optional, which means deciding what an omitted argument stands for,
which arguments can be left out, and what happens when no default can be found.

## Decision

- Any named parameter of a tuple constructor may be left out, and the argument then has the default value of the
  parameter's type. Unnamed parameters must still be supplied, with the count message of decision 17 if they are not.
  Positional arguments still bind a prefix of the parameters, so a parameter is skipped by naming the ones after it.
- The default of a type depends on the type alone, so a file means the same whether or not the types are known
  when it is read. There are no declared defaults.
  - `Nat` and `Int` default to `0`, `Float` to `0.0` and `String` to `""`, and an array to `[]`.
  - A record defaults to the record of the defaults of its fields.
  - An enum defaults to its first constructor, filled in with the defaults of its fields or parameters, named or not.
- The prelude is `enum Bool = False, True;` so that the default of `Bool` is `false`, the zero of the booleans.
  `Option<T>` already has `None` first.
- A default that cannot be used is an error where the argument is omitted, and not at the declaration, so that a
  type that is never defaulted is unaffected:
  - `Type List<T> has no default value` if the expansion contains the type itself, directly or through other types.
  - `The default value of T has more than N nodes, which is too large; ...` if it is not recursive but is larger than
    the limit, which happens when each declaration holds two copies of the one before.
  - `Cannot work out the default value of argument x: its type is not known` if the type is an open type parameter.
    The omitted arguments are checked after the others, so that they have fixed the types as far as they can.
- The size of a default is the number of values it contains, and it is counted without building the value, so
  refusing a huge default is quick. The limit is applied to each omitted argument. It is 10,000 by default and
  `--default-limit=N`, or `CheckOptions::default_limit`, changes it. It is separate from `--infer-limit`, because
  it limits a different thing.

## Consequences

- A value can be shorter than the data it stands for. The checker only checks, so it does not write out the omitted
  arguments.
- Changing the order of the constructors of an enum can change the default of every parameter of that type.
- A recursive type with its recursive constructor first cannot be omitted. Writing the base case first, as in
  `enum List<T> = Nil, Cons { ... }`, avoids it.
- Record fields and the parameters of unnamed constructors are not optional.
- There is no way yet to forbid omission. A flag could be added later, for example with the limits on const
  references.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- Declared defaults, as in `alignment: String = "left"`: they need the types to be present to be understood, and a
  default would be implicit, one more thing to remember when reading a value.
- Only `Option<T>` parameters may be omitted, as `None`: this ties the feature to one type of the prelude.
- The first constructor that has a finite default: it never fails for sensible types, but then the default is no
  longer simply the first constructor, and reordering would change it in less obvious ways.
- Checking every declaration for a usable default: it rejects valid types that are never defaulted.
