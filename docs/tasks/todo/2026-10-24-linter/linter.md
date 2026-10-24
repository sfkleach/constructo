# Task: Implement a `lint` subcommand

## Overview

`constructo check` reports what is wrong with the inputs. A linter reports what is legal but probably not what the
programmer meant. This task adds a `lint` subcommand that reads the same inputs as `check` and reports warnings.

The first rules are about default values (decision 18). A named argument that is left out takes the default of its
type, and the default of an enum is its first constructor, filled in with the defaults of its fields. An enum has no
default if that expansion is recursive, is too large, or reaches a type that is not known. Such an enum is legal, but
omitting an argument of that type is an error, and the error is found only where the argument is omitted.

## Rules

### Rule 1: An enum has no default but could be reordered to have one

Warn when an enum definition has no default value, and some other ordering of its constructors would give it one. The
usual case is a recursive type with its base case written second:

```
enum List<T> = Cons { head: T, tail: List<T> }, Nil;
```

The warning names the constructor to move to the front, for example "Enum List has no default value; it would have
one if Nil came first".

An enum that has no default whatever the order of its constructors is not reported by this rule.

### Rule 2: An enum without a default is the type of a named parameter

Warn when the type of a named parameter of a tuple constructor is an enum that has no default value, because the
argument can never be omitted. The warning is at the parameter, and names the enum.

```
enum Holder = Holder(items: List<Nat>);
```

## Step 1: Add the subcommand and the warning machinery

Add `constructo lint [OPTIONS] FILES...`, which loads the files and the prelude as `check` does and reports warnings
in the diagnostic style of `check`, with `WARNING:` in place of `ERROR:`. It needs the options that decide what a
default is, such as `--default-limit`, and `--no-prelude`.

## Step 2: Implement the two rules

Implement rules 1 and 2 above. The calculation of the default of a type is the one that the type checker uses, so it
should be shared rather than copied.

## Open questions

These are for the grilling when the task is started:

- Whether the linter first runs the checks of `check` and stops if they fail, or reports only warnings and assumes
  that the inputs are valid.
- The exit codes, and whether there is a way to make warnings fail the run.
- What counts as "could be reordered" for an enum with several recursive constructors, and which constructor the
  warning suggests when more than one would do.
- Whether a default that is only too large, as opposed to recursive, is covered by the rules.
- How a rule is switched off for a declaration or a run.
