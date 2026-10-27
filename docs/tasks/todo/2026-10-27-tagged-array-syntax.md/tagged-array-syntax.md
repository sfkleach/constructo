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

## Question

The question this raises is whether the variant `Users([User])` is different
or the same as `Users[User]`. I believe it must be consistent with whether
`Users{user: User}` is identical to `Users({user: User})`. My intuition says
that they should be. Are they?

