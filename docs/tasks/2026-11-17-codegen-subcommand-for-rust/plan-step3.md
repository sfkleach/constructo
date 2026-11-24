# Plan: step 3 of codegen, the model

This plans step 3 of the [implementation plan](implementation-plan.md): a description of the declarations of a run,
the same for every target, which step 4 writes Rust types from and step 5 a codec. Nothing is generated yet, there is
no command, and no behaviour of `constructo` changes, except as the first section says.

## What the code has today

- **The run.** Every typed entry point of `src/lib.rs` (`convert_typed`, `convert_typed_back`, `query_typed`) calls
  `check_with_layout` with a `ValueMode`, which parses, resolves, validates and type-checks the sources, and then does
  what the mode asks. `ValueMode::Read` hands its caller a `ReadContext`: the `Env`, a target type and a `show`
  function. A run with an error reports it, and the mode does nothing.
- **Names.** `resolve::resolve` rewrites every name, in place, to its canonical name, `Shape@com.example.shapes`, or
  the bare name in a run with no project. `Resolution::show` rewrites canonical names in a text as a file would write
  them: bare where the name resolves to the same declaration, and otherwise `com.example.shapes::Shape`. It never
  writes a name through a group. `Names::show` does this, and the messages and `convert`'s writer both use it.
- **The layout.** `Layout` has the projects, each with its full name, its imports (group, target, hides, renames) and
  whether it is built in, the owner of each source, and the root.
- **Types.** The checker's `Ty` has `Nat`, `Int`, `Float`, `Str`, `Array`, `Enum(name, args)` and `Record(fields)`, and
  variables that only inference needs. The declarations are `ast::EnumDecl` and `ast::ConstDecl`, with `ast::Type` for
  the types that are written, and `typecheck::instantiate` turns one into a `Ty`.
- **Consts.** `check_consts` infers the type of a const that does not declare one, and `Env::inferred_type` gives its
  `Scheme`.
- **Defaults.** `Checker::default_nodes` works out the default of one type, with its arguments, when an argument is
  left out of a value. It counts nodes against `--default-limit`, refuses a type nested more deeply than
  `MAX_TYPE_DEPTH`, and finds a type that is met again while its own default is being worked out ("Type X has no default
  value").

## First: names through a group

The model holds, for each constructor, the form in which the root project writes it: **bare, else through a group,
else in full**. `Names::show` gives only bare or full. The task [convert-writes-names-through-a-group][group] adds the
group, and the outline puts it before step 5, so that `convert` and the codec write the same text.

**Decided:** that task is done first, as the first commits of this step, since the model needs the same rule and it
should be written once.

- A function of `Resolution`, `written(file, local, project) -> Written`, gives `Bare(name)`, `Group(group, name)` or
  `Full`, with the rules of the task: renames count, a hidden name is not visible through its group, and the first
  import in the project-file wins. `convert`'s writer uses it, and so does the model.
- **Messages keep the form they have,** bare or full, so that no test of a message changes for this, and a message
  does not depend on the groups of the project. Whether they should change too is left to the task's own question,
  answered no here.
- The task moves to `docs/tasks/` when it starts and to `docs/tasks/done/` with this task, as `CLAUDE.md` asks.

## The model

**Where:** `src/codegen/model.rs`, with `src/codegen/mod.rs`, in the library, as the outline says. The entry point is
in `src/lib.rs`, beside the others:

```rust
pub fn build_model(sources: &[Source], layout: &Layout, options: &CheckOptions) -> Result<Model, Vec<Diagnostic>>
```

It runs `check_with_layout` with a new mode, `ValueMode::Model`, which is given the `Env`, the layout and the
resolution once the types are checked, as `Read` is. **Every value of the values files is checked, as `check` checks
them,** and an error in any of them is an error of the run, which builds no model: the command writes code for a
project, and a project whose sample values are wrong is not one to write code for. That is stricter than
`convert_typed`, which checks only the values that it converts, and it matches the draft's "a run that has errors
writes nothing". *Decided,* over checking the types alone, which would let `codegen` succeed on inputs that `check`
refuses.

**What it holds.** The types are the model's own and not the checker's, since they are an interface for targets, and
the checker's `Ty` has variables that only inference needs and canonical names with `@`:

```rust
pub struct Model {
    pub projects: Vec<Project>,      // in the order of the layout
    pub root: usize,
    pub enums: Vec<Enum>,            // in the order of the files and of the declarations
    pub records: Vec<Record>,        // the record types without a name, in the order they are first met
    pub consts: Vec<Const>,
}

pub struct FullName { pub project: String, pub local: String }   // shown as `project::local`

pub enum TypeRef {
    Nat, Int, Float, String,
    Array(Box<TypeRef>),
    Enum(FullName, Vec<TypeRef>),
    Record(usize, Vec<TypeRef>),     // an index into `records`, and the arguments for its parameters
    Param(String),
}
```

- **A project:** its full name, empty for files given without one; its imports, each with its group, its target, its
  hides and its renames; and whether it is built in. The module name of a target is derived from the full name by the
  target, in step 4, and is not in the model.
- **An enum:** its full name, the index of its project, its parameters (with whether each is written with `_`), and
  its constructors in order. A constructor has its full name, its body, and its written form. The body is none, a tuple
  (each parameter with its name, if it has one, and its type), a record (each field with its name and type) or an
  array (its element type). Each field and each parameter has `boxed`, below.
- **A record without a name:** its fields, sorted by name, each with its type and `boxed`; its parameters, which are
  the parameters of the enum that occur in its fields, in the enum's order; its name, below; and the projects that use
  it.
- **A const:** its full name, its project, its parameters, its type (declared, or inferred as `check` infers it) and
  its value, as the syntax tree with every name in full. A reference to another const stays a reference, which the
  codec resolves when it reads, as the draft's section on consts says.

**What it works out**, so that no target does it again:

- **`boxed`,** for each field and parameter: whether its edge is on a cycle of types held by value, as the draft's
  section on recursion says. The graph has a node for each enum, each constructor and each record without a name. A
  type holds another by value directly, in an `Option` (which is an enum like any other), in a field of a tuple, or
  through a type argument where the declaration holds that parameter by value. That last is worked out first, as a
  fixed point over the declarations. An array holds nothing by value.
- **Whether an enum has a default at all.** The outline said "the default of every type", but a default depends on
  the arguments of a generic type: the default of `Styled<T>` needs that of `T`. So the model holds what does not:
  whether the default of an enum would expand forever whatever its arguments, which it does when its first
  constructor holds, through its fields and records but not through arrays, an enum that leads back to it. Such an enum
  has no default, and the generated code gives `check`'s message for it, `Type X has no default value`. Every other
  default is finite for finite arguments, and the generated code builds it, against `--default-limit` as `check` does.
- **Whether a type can be the absent value**, for a target such as Python: a `TypeRef` that is the prelude's `Option`,
  or a parameter, can be; any other cannot. It is a function on `TypeRef`, and step 4 does not use it.

## Record types without a name

These are the decisions of the draft's section, which this step implements and writes as a decision:

- one record for each distinct type, by its fields and their types, in any order;
- its name is the field names, sorted and joined with `_`, or `empty` for `{}`;
- where two distinct records have the same name, the first met keeps it and the others are `_2`, `_3` and so on, in
  the order of the files and of the declarations.

**Decided:** a record whose fields differ only in the names of their parameters is the same record. With

```
enum Foo<T> = Foo { pos: { x: T } };
enum Bar<U> = Bar { at: { x: U } };
```

the two fields have one record, `x<A>`, used as `x<T>` in `Foo` and as `x<U>` in `Bar`, so a program can pass the
`pos` of a `Foo<Nat>` as the `at` of a `Bar<Nat>`, as Constructo allows. A record is identified by its fields, sorted
by name, with the parameters renamed in the order in which they first occur in them, so `{ a: T, b: U }` and
`{ a: U, b: T }` are one record used with its arguments swapped. The other choice, a record for each set of names of
parameters, was simpler, and would have given two `struct`s for one type.

The draft's open question, a limit on the length of a long name, is left open: no example needs it.

## The tests

Unit tests in `src/codegen/model.rs`, built with `build_model` from sources and layouts, as the tests of `resolve` are:

- the projects of `scenario-shapes/projects`: two projects, the group `ids`, the written forms `Uuid` bare in `ids`
  and `ids::Uuid` in `shapes`, `Styled<T>` with its record `dx_dy`, and the const `unit_circle`;
- names: a rename through a group, a hidden name, a project in two groups, and a name that must be written in full;
- recursion: `Expr` and `Term` boxed at both sites, `mutual.types` with no box, a cycle through an `Option`, a cycle
  through a type argument (`Styled<Node>` in `Node`), a cycle broken by an array, and `Nested<T>`;
- defaults: `List<T>` whose first constructor is `Cons` has none, `List<T>` whose first is `Nil` has one, and a
  generic enum whose default depends on its argument has one;
- records: the same record in two projects, the same names with different types (`x` and `x_2`), `cpu_mem` both ways,
  `{}`, and the same record under two names of parameters;
- consts: a const that refers to another, a generic const (`null`), an inferred type, and a value with names in full;
- every valid types example and project example builds a model, and a run with an error in a value builds none.

## Decisions

- **0048: the model of the declarations, and how a record type without a name is named.** What every target is given,
  and why it is not the checker's types; the records, their names and their identity; that boxing is on every field
  of a cycle; and that a default is worked out by the generated code, with the model saying only which enums have
  none. The draft's entry is "How a record type without a name is named, and consts and defaults".
- The group task writes nothing new as a decision: the rule is the draft's, and the spec of `convert` says it.

## The commits

1. Names through a group: `Resolution::written`, `convert`'s writer using it, its tests and the spec of `convert`.
2. The model's types and `build_model`, with projects, enums, constructors, records and consts.
3. What it works out: `boxed`, the enums with no default, and the absent value.
4. The decision, and the draft and the outline brought up to date.

## Done when

- The model of every valid types example and project example can be built, and the tests above pass.
- `just test` passes, which runs the unit tests, clippy, fmt, the audit and the functests, with no test changed except
  those that expect a full name where a group would do.

[group]: ../2026-11-21-convert-writes-names-through-a-group/convert-writes-names-through-a-group.md
