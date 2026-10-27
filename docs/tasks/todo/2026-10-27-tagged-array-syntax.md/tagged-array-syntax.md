# Task: Tagged Array Syntax

## Overview

We will support enums with a variant that takes a single array type as their
argument with this syntax:

```
enum Users = Users[ User ], ...;
```

The value syntax mirrors this: `Users[User{...}, ...]`. It is roughly
equivalent to this:

```
enum Users = Users([ User ]), ...;
```

## Questions

1. One question this raises is whether the variant `Users([User])` is different
or the same as `Users[User]`. We have determined the answer is that they will be
treated as different forms.

2. Single type: Users[A, B] should be a syntax error, and so should Users[] with
no element type. The body holds exactly one element type.

3. Named parameters: a tuple variant can have named parameters (decision 17).
The array form has no parameter name, so it can't be omitted with a default by
name. 

4. Postfix ?: Users[User]? should be Option<Users>, since ? binds to the whole
primary. That falls out of the existing rule, but it needs a test.

5. Value parsing: Users[1] after a bare constructor is already safe, because
top-level values are separated by semi-colons (a previous decision). The grammar
still needs a Tagged alternative with an array body.


## Step 1:

Implement a new variant form of tagged arrays, every similar to the way 
tagged records work.

```
enum JoinList<T> = Items[T], Join[JoinList<T>]
```

In the value notation the constructors `Item` and `Join` are tags that are
applied to array and pin the types of their members.

```
const j: JoinList<Nat> = JoinList[Items[1, 2], JoinList[Items[3, 4], Items[5, 6]]]
```



## Step 2: CHANGELOG

Bring the CHANGELOG up to date.

