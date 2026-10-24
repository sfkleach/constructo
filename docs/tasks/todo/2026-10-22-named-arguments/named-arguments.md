# Task: Implement Named Parameters in Constructor parameter lists

## Overview

Constructor parameter lists are a parenthesised list of positional parameters.
In this task, we will be adding named-parameters as well as positional
parameters.

```
enum Image = Image(
    src: String, 
    name: String,
    hOffset: Nat,
    vOffset: Nat,
    alignment: String
);
```

Unlike record-like parameters, the order of the field does matter because
arguments can be supplied by name or position. Positional arguments are
supplied before any optional arguments are supplied.

```
// Positional arguments come before named arguments when mixed.
Image(
    "~/.local/images/snowy_scene.jpg",  // src is first
    "Snowy Scene with Hidden Santa",    // name is next
    alignment : "left",                 // binds alignment
    hOffset: 100,                       // binds hOffset
    vOffset: 100,                       // binds vOffset
)
```

Parameters that do not have a name must be bound by position. Named parameters
can be bound by name or position.

## Step 1: Implement Named Parameters and Arguments

Implement named parameters in such a way that any number of the parameters can
be given names but names are not mandatory. Positional parameters must precede
named parameters.

### Refinements agreed

Declarations:

- A parameter may have a name, as in `Foo(Nat, x: Nat)`. Unnamed parameters
  must come before named ones, so `Foo(x: Nat, Nat)` is a syntax error.
- Unnamed parameters can only be bound by position. Named parameters can be
  bound by position or by name.
- Names start with a lowercase letter or underscore, the same shape as const
  names, and `const` stays reserved.
- Each constructor has its own namespace for parameter names. A parameter name
  cannot clash with a const, a constructor or a type.
- A duplicate parameter name within one constructor is an error.

Arguments:

- Positional arguments come first. A positional argument after a named one is
  a syntax error.
- Named arguments may appear in any order.
- A named argument must name a parameter of the constructor, otherwise it is
  an error.
- Each parameter can be bound only once. Binding it twice is an error, whether
  by position then name, or by name twice. The error is reported at the
  second binding.
- In this step every parameter must still be supplied, by position or by name.
  A missing one is the error "Missing argument for <name>". Omission arrives
  in Step 2.

Typing and syntax details:

- Parameter names only affect how arguments are matched to parameters, and
  they appear in diagnostics (for example, "Expected Nat for argument
  hOffset"). They are not part of the type, so widening and compatibility are
  unchanged.
- Trailing commas follow whatever the parser does today for positional lists,
  and the same applies to named ones.
- The task's first example had a missing comma after `alignment : "left"`.
  That was a typo and is now fixed.

Deliverables: lexer and parser changes, validation of names and ordering,
argument matching in the type checker, the grammar files with regenerated
PDFs, a decision record, examples, functests and a CHANGELOG entry.

## Step 2: Implement Optionality of Named Arguments

Argument lists can only appear in the context of a constructor, so the type
of the arguments is fully determined. Named arguments may be omitted without
ambiguity. 

