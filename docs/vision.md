# Vision: Constructo in use

Priya owns the service `harbour`; Tomás runs its deployments. They are tired of
YAML configs that fail at 2am because a port was written as a string.

## Day one: Priya writes the schema

Priya writes `harbour.types`:

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

She runs the tool to check the syntax of the types file alone. It points at the
line and column of a missing `;`. Once fixed, the tool also runs the post-parse
checks and tells her she declared a variant twice.

## Day two: Tomás writes values

Tomás writes `prod.values`:

```
Config { name: "harbour", env: Prod, port: 8080, replicas: 3, owner: null }
```

He validates it against `harbour.types`. The tool reports the syntax is fine and
the value is type-correct. When he writes `port: +8080` the tool says an `Int`
was supplied where `Nat` is expected, with the location.

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
- Precise error locations; machine-friendly exit codes.

## Later

Selecting sub-values from a file or stdin, which needs a path notation. Files
that hold both types and values may also be considered.

Syntax: see `syntax/dt_grammar_tabatkins.txt`, `syntax/dv_grammar_tabatkins.txt`
and `syntax/examples/`.
