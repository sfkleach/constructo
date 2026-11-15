# Task: Design how a map is modelled

## Overview

Constructo has records, whose fields are a fixed set of names that the types declare, and arrays, whose elements are
all of one type. It has no map: a collection of entries whose keys are data, not declared names.

JSON has no map either, and JSON documents fill the gap by using an object as one, with the keys as data. That is the
source of JSON's need to delete a field, and of much of its confusion: the same syntax means both "a record with these
fields" and "a lookup table with these keys", and a reader cannot tell which. Constructo keeps those apart, so a record
never has a field deleted. That leaves a real question about how a lookup table should be written, typed and
processed.

The question came up while exploring the query language for `#values` (path extraction and transformation): deleting
a field was set aside as not a Constructo operation, and deleting an entry belongs to maps instead.

This task is to design the model. Its implementation will be a separate task.

## Questions to settle

- **Is a map a new kind of value, or a convention?** It could be a prelude type such as `Map<K, V>` with syntax of its
  own, or an array of entries such as `[Entry(key, value)]` or `[{ key: k, value: v }]` with nothing new in the
  grammar. A convention needs no new syntax, but it does not say that the keys are unique, and a reader cannot tell a
  map from a list.
- **The syntax.** If a map is a kind of value, how is it written? A record is `{ name: value }` with a name before the
  colon, so `{ "a": 1 }`, with a literal before the colon, could be a map. Is that distinct enough, or too easily
  confused with a record? How is an empty map written, given that `{}` is already the empty record?
- **The keys.** Only strings, or any type, such as `Nat`, constructors or records? If any type, what makes two keys
  equal (for example `5` and `5.0`, or `Circle` and `shapes::Circle`, which the query exploration treats as different
  when untyped)?
- **Order and uniqueness.** Is the order of the entries kept, as it is for the fields of a record and in the output of
  `convert`? Is a repeated key an error, as a repeated field is, and where is it reported?
- **The types.** How a map type is declared and checked, its default when an argument of that type is omitted, and how
  it interacts with generics and widening.
- **The formats.** How a map is written by `json-exact`, `json-typed` (a JSON object when the keys are strings, and
  otherwise?), `jsonl-*` and `xml`, and read back by `--from`.
- **The query language.** How a map is read and changed in `#values` expressions: lookup (`m["key"]` and `m.get(k)`,
  as in Rust's `HashMap`), iteration over entries without anonymous tuples (which Constructo does not have), and
  inserting and removing entries with `update`.

## Steps

1. Survey how other data formats model maps, and how they keep them apart from records: for example TOML tables,
   YAML mappings, EDN maps, Dhall's `Map` (a list of `{ mapKey, mapValue }`), Protocol Buffers' `map<K, V>`, and
   serde's handling of maps against structs.
2. Write examples of maps in values files, in types files and in `#values` expressions, including the awkward cases:
   an empty map, non-string keys, a repeated key, and a map inside a record.
3. Choose between a new kind of value and a convention, and settle the questions above.
4. Record the decision in `docs/decisions/`, and write the implementation task with its TDD-style examples.

## Dependencies

None, but the outcome affects the query language for `#values`, which is being explored at the same time, and the
design of `convert` for every format.
