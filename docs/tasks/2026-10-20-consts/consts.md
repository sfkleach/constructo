# Tasks: const definitions

## Overview

We are adding a new kind of definition that goes in the data-types files. These
can be freely interleaved with `enum` definitions. The general syntax is:

```
const IDENTIFIER [: TYPE_EXPRESSION] = VALUE_EXPRESSION;
```

Note that the semi-colon is a terminator for these definitions, the same as 
enums.

In order for these new definitions to work, it is very helpful to be able to
give types to untagged objects such as `{ left: "c", right: "r" }`. In this
case the type would be `{left: String, right: String}`.

In general, the type of an untagged object is denoted by substituting type
expression for where the value expressions would go.

## Step 1: Add `const` to Constructo data-type files

Extend the `check` command to recognise `const` definitions. The type expression
is optional. 

- Value-expressions are extended to support references to const identifiers
- `const` definitions may not reference `const` identifiers unless the 
  `--allow-refs` option is set
- `const` definitions are evaluated on demand i.e. order does not matter
- `const` definitions may not have a circular dependency
- The type of a const identifier is the type given in its definition
- In this step the type must be present.

Note that Constructo data-type files can now begin with either `enum` or `const`.
This affects the detection of the type of file. 

### Refinements agreed during the step

- Without `--allow-refs`, a reference to a const inside a `const` definition is an error at the reference, and the
  message names the flag. The order and cycle rules only apply when the flag is set. The flag is a `check` option and
  only governs const-to-const references.
- Const references may appear in values files and, with `--allow-refs`, in `const` bodies.
- `const` is a reserved keyword everywhere, so classifying a file is still a first-token test: `enum` or `const`
  means a types file.
- Consts share the constructor namespace and may be mixed-case. This follows Rust's split between the type namespace
  and the value namespace. These are duplicate or clashing-name errors, with the usual "first declared at" note:
  const vs constructor, const vs const (within and across files), and const vs basic type name (as for
  constructors). A const may share a name with an enum type, just as a constructor may, so
  `enum Config = Settings { port: Nat }; const Config: Config = Settings { port: 80 };` is valid.
- In this step the type expression uses only the existing forms: basic types and enums with their generic arguments.
  Type parameters are not in scope. Record-valued consts need a tagged constructor until Step 2.
- A const's value is checked against its declared type with the same type-checker as values files, and a reference to
  a const has the type declared in its definition.

## Step 2: Add untagged object type expressions

To be done

## Step 3: Make the type-expression part of a `const` definition optional

To be done

## Step 4: Update the grammar files

To be done

## Step 5: Extend the standard prelude with constants true, false and null

To be done

## Step 99: Update the CHANGELOG

To be done