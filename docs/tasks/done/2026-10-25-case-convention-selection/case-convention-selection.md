---
Status: Abandoned
---

# Task: Select the case-convention policy

## Overview

There are five categories of identifier, each with their own lexical rules at 
present:

- Types: initial upper case letter
- Constructors: initial upper case letter
- Field names: initial lower case letter
- Parameter names: same as field-names
- `const` names: unrestricted

In this task we want to be able to explicitly apply case-convention policies
which override the lexical rules on a per-project basis. Policies are drawn from
a limited set:

- any
- PascalCase
- camelCase
- snake_case
- SCREAMING_SNAKE_CASE
- flatcase
- UPPERFLATCASE
- First_Letter_Uppercase
- first_Letter_Lowercase

Policy names are canonised by removing underscores and converting to lower
case so our policies are:

- any
- pascalcase
- camelcase
- snakecase
- screamingsnakecase
- flatcase
- upperflatcase
- firstletterlowercase
- firstletteruppercase

The lexical categories are:
- type
- constructor
- field
- parameter
- const 

Without overrides, the defaults should be:

- type: firstletteruppercase
- constructor: firstletteruppercase
- field: firstletterlowercase
- parameter: firstletteruppercase
- const: any


## Step 1: Implement the `--case` option

To set policies by options, we use a new `--case` option like this - note that
the values should be case-insensitive, permitting case-aware spelling like the
one I have chosen (but not mandating it).
```
--case=type:First_Letter_Uppercase
--case=constructor:First_Letter_Uppercase
--case=field:first_Letter_Lowercase
--case=parameter:first_Letter_Lowercase
--case=const:any
```

The `-c,--case` option may be given repeatedly and has the general form:
```
--case=CATEGORY:CASE_POLICY
```


## Step 2: Implement the [case] config section

We want to be able to specify these options in the configuration file. It
should look something like this:

```
[case]
type=firstletteruppercase
constructor=firstletteruppercase
field=firstletterlowercase
parameter=firstletteruppercase
const=any
```

## Step 3: Adjust the grammar

The grammar needs updating so that the lexeme for each category is essentially
drawn from `[a-zA-Z_][a-zA-Z_0-9]*` (or near offer) with `enum` and `const` 
excluded.

## Step 4: Tidy documentation

- [ ] Adjust the CHANGELOG.md
- [ ] Modify the docs/specs/project-file-format.md

## Outcome: abandoned

The task was abandoned before any code was written, because of a problem with the grammar that Step 3 would cause.

**The problem.** Today the case of a bare name in a value tells the parser what it is: an uppercase name is a
constructor or a const, which the checker decides between, and a lowercase name can only be a const. Once the lexeme of
every category is a general identifier, a bare name such as `circle` could be a constructor or a const, and nothing in
the text of the value says which. It can only be told by looking at the declarations. That undermines the ability to
read a values file without a project: the names in it could no longer be classified from the file alone.

Tagged values with a body (`circle { r: 1.0 }`, `circle(1)`, `circle[1]`) could be told by the token that follows. The
bare form cannot, and a rule that told it by lookup would make the meaning of a values file depend on the project that
checks it.

**What was settled before it was dropped,** in case the idea is taken up again:

- The two policies `First_Letter_Uppercase` and `first_Letter_Lowercase` are exactly today's lexical rules, so the
  defaults keep today's behaviour. The default for `parameter` is `firstletterlowercase`, as for a field, and not the
  `firstletteruppercase` that the list above has by mistake.
- A leading `_` is accepted by `firstletterlowercase` and by no other policy, apart from a phantom type parameter,
  which keeps `_T`.
- The patterns: `any` is `[A-Za-z_][A-Za-z0-9_]*`; `firstletteruppercase` starts `[A-Z]` and `firstletterlowercase`
  starts `[a-z_]`, each with any identifier characters after; `pascalcase` is `[A-Z][A-Za-z0-9]*`; `camelcase` is
  `[a-z][A-Za-z0-9]*`; `snakecase` is `[a-z][a-z0-9]*(_[a-z0-9]+)*`; `screamingsnakecase` is
  `[A-Z][A-Z0-9]*(_[A-Z0-9]+)*`; `flatcase` is `[a-z][a-z0-9]*`; `upperflatcase` is `[A-Z][A-Z0-9]*`.
- A category is case-sensitive. A policy is case-insensitive, with its underscores dropped, and a hyphen is not
  normalised.
- A project declares its policy in a `[case]` table of its project-file. The order of precedence for the root project is
  the defaults, then the project-file, then the settings file (which only packages options), then `--case`, per
  category. An imported project has only its own project-file. A virtual project has no project-file.
- Names in a values file are never checked against a policy, so that a values file can be parsed without one.
- The category `parameter` is easy to confuse with a generic type parameter. A name such as `named-parameter` would
  be clearer.
- The check for a hidden or renamed name in `project-file-format.md` says that a type or a constructor starts with a
  capital letter. If the rules are ever relaxed, that check has to go, and the new name checked by the policy.

**If it is taken up again,** the bare name has to stay classifiable from the text of the values file alone. A design in
which the case of the first letter stays structural, and the policies only choose among styles that respect it, would
do that, but it was not explored.
