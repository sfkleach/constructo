---
Status: WIP
---

# Task: Select the case-convention policy

## Overview

There are five categories of identifier, each with their own policy at 
present:

- Types: initial upper case letter, intention is PascalCase
- Constructors: initial upper case letter, intention is PascalCase
- Field names: initial lower case letter, intention is camelCase
- Parameter names: same as field-names
- `const` names: unrestricted

In this task we want to be able to explicitly apply case-convention 
policies. Policies are drawn from a limited set:

- any
- PascalCase
- camelCase
- snake_case
- SCREAMING_SNAKE_CASE
- flatcase
- UPPERFLATCASE

To set policies we use a new `--case` option like this - note that the values
should be case-insensitive, permitting case-aware spelling like the one I 
have chosen (but not mandating it).
```
--case=type:PascalCase
--case=constructor:PascalCase
--case=field:camelCase
--case=parameter:camelCase
--case=const:any
```
Obviously with such a bulky option, we need to be able to make it more 
convenient. So this task should be prefixed by implementing the `--config=FILE`
option first. 

## Step 1: Implement the `--case` option

The `-c,--case` option may be given repeatedly and has the general form:
```
--case=CATEGORY:CASE_POLICY
```
The categories are types, constructors, fields, named-parameters, and consts. 
The policies are any of the following, where the comparison is case-insensitive: 
any, pascalcase, camelcase, snake_case, screaming_snake_case, flatcase and 
upperflatcase. 

Each option places a restriction on a category of identifiers with the intention
of enforcing a house style.


## Step 2: Implement the [case] config section

We want to be able to specify these options in the configuration file. It
should look something like this:

```
[case]
type=PascalCase
constructor=PascalCase
field=camelCase
parameter=camelCase
const=any
```
