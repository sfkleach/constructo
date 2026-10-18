# Vision: Constructo in use

Priya owns the service `harbour`; Tomás runs its deployments. They are tired of
YAML configs that fail at 2am because a port was written as a string.

## Day one: Priya writes the schema

Priya writes `harbour.types`:

```
enum Env = Dev, Staging, Prod, Dev
enum Config =
  Config {
    name: String,
    env: Env,
    port: Nat,
    replicas: Nat,
    owner: Option<String>,
  };
```

She runs the tool on the types file alone.

```
> constructo check harbour.types
ERROR: Expected semi-colon (harbour.types, line 2, col 1)
```

It points at the file, line and column of a missing `;`. Once fixed, the tool also
runs the post-parse checks and tells her she declared a variant twice (Dev):

```
> constructo check harbour.types
ERROR: Duplicate constructor Dev (harbour.types, line 1, col 32); first declared at line 1, col 12
```

The corrected text reads:

```
enum Env = Dev, Staging, Prod;
enum Config =
  Config {
    name: String,
    env: Env,
    port: Nat,
    replicas: Nat,
    owner: Option<String>,
  };
```

## Day two: Tomás writes values

Tomás writes `prod.values`:

```
Config { name: "harbour", env: Prod, port: +8080, replicas: 3, owner: null }
```

He validates it against `harbour.types`. 

```
> constructo check harbour.types prod.values
WARNING: Literal Int (non-negative) supplied where Nat expected (prod.values, line 1, col 45)
```

The tool warns him that he has used a signed integer literal where a natural
number was expected. A non-negative signed literal such as `+8080` is accepted
for a `Nat` with a warning; a negative one such as `-1` is an error. Once he
removes the sign, `constructo
check` reports that the syntax is fine and the value is type-correct. 

## Day three: CI

The pipeline validates every `*.values` file against the schema, and a generated
config piped on stdin. A non-zero exit fails the build, so mistakes are caught
before deployment.

## Capabilities established

- Validate the syntax of a types file.
- Validate the syntax of one or more values files.
- Validate values files against a types file (syntax plus type-correctness).
- Validate stdin against a types file.
- Post-parse checks: duplicate variants or fields, unknown types, wrong generic
  arity.
- Warnings (e.g. `+8080` where a `Nat` is expected) are distinguished from errors.
- Precise error locations; machine-friendly exit codes.

## Later

Selecting sub-values from a file or stdin, which needs a path notation. Files
that hold both types and values may also be considered.

Syntax: see `syntax/dt_grammar_tabatkins.txt`, `syntax/dv_grammar_tabatkins.txt`
and `syntax/examples/`.
