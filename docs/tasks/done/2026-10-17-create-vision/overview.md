# Overview of Constructo

- `Contructo` documents have two parts, a set of enum declarations called the
  _data-types_ and the list of values called the _data-values_.
- The syntax of the data-values is closely related to JSON but with the addition
  of tagged records and parenthesized tuples.
- The syntax of the data-types is closely related to Rust enum declarations but
  adapted to avoid overloading braces, since they have such a well-established
  role in the value notation.


## Data-types

- Named product types and Rust-like enums.
- Mutual recursion.
- Parametric generics without constraints.

Examples:
```
// Comments are supported.
enum Shape =
  Circle { radius: Float },                 // commas separate.
  Box { width: Float, height: InFloatt };   // semi-colon terminates.

// Recursive types are supported.
enum List<T> =
  Nil,
  Cons { head: T, tail: List<T> };

// Product (tuples) are suppoprted.
enum Message =
    Quit,
    Move { x: Int, y: Int },
    Write(String),
    ChangeColor(Nat, Nat, Nat),
    Error([String]);
```

## Data-values

- JSON-like tagged and untagged objects, arrays, strings and numerical types. 
- Booleans and null are pre-made enums.

Examples:
```
Circle { radius: 3.0 }
Box { width: 7.0, height: 1.0 }
Cons { head: True, tail: Nil }
Move { x: +8, y: -1 }
ChangeColor(255, 255, 255)
Error(["C9602", ""C9678])
```

Additionally, untagged objects are supported:
```
{ x: 17.0, y: 18.2, description: "X marks the spot" }
```

