# Constructo

Constructo is a typed, simple-object data notation that grafts Rust's `enum`
syntax onto JSON-style values. The set of type-definitions plays the role of
a schema but is based on very familiar type-checking principles. 
For example, this definition, `enum Env = Dev, Staging, Prod;`, which we hope is 
immediately readable to Rust programmers, says that an `Env` field can only have
one of those 3 values. 

A more elaborate definition might be for a deployable service:
```
enum Deploy =
    Deploy {
        service: String,
        env: Env,
        replicas: Nat,
        canary: Option<Env>,
    };
```

And this could be instantiated with a term such as:

```
Deploy {
    service: "https://predprod.acme.com/export?format=XML",
    env: Staging,
    replicas: 1
    canary: Some(Dev)   // or simply Dev?
}
```

The project provides a Rust library for processing Constructo files and the
`constructo` command-line tool. Today `constructo check` validates the syntax
and consistency of data-type declarations (duplicates, unknown types, generic
arity and so on) and loads a small standard prelude declaring `Bool` and
`Option<T>`; checking data values against those types is planned. See
[docs/vision.md](docs/vision.md) for the intended use and
[syntax/](syntax/grammar.md) for the grammars and examples. It is open source
under the MIT licence.
