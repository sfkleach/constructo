# 0041 - Reading xml back is strict, with a parser of our own, 2026-11-11

## Issue

**Fork.** `constructo convert --from=xml` reads the XML that `--to=xml` writes, and writes the values as text. XML is a
large language with features that a reader of a data format should not have, and its rules for white space in
attributes are subtle, so there are choices about how much of XML is read, where the parser comes from, and what is done
with a line break inside an attribute value.

## Decision

- **Only a small subset of XML 1.0 is read.** It is what `--to=xml` writes, and a little more, so that a file that was
  edited by hand or passed through another tool still reads: an optional byte order mark and declaration, comments,
  white space between elements, attributes in single or double quotes, and the five entities that XML defines and
  numeric references. A DOCTYPE, CDATA sections, processing instructions, text between elements, namespaces and any
  other entity are refused, and so is a declaration that is not version 1.0 and UTF-8.
- **The parser is our own,** in `src/xml_tree.rs`, as the JSON parser is in decision 38, and not a dependency. What
  decides it is the control of the processing of entities. A general parser reads what we want to refuse, and would have
  to be configured not to declare, expand or fetch entities, which is the "billion laughs" and external-entity family of
  problems. A parser that cannot read a DOCTYPE at all has none of them. It also keeps a line and column on every
  element and attribute, so that every problem in what a value means is reported where it is, as the JSON parser does.
  It is about 460 lines without its tests, and no dependency has to be kept up to date.
- **The reading is strict,** as for `json-exact`. An element has exactly the attribute of its kind and the children that
  the kind allows, in the order that `--to=xml` writes them, and an unknown, missing or repeated attribute, an element
  in the wrong place, and a name that is given twice among the fields are errors at the place. The text of a number and
  a name are checked by the real lexer and parser, so the reader holds no second grammar. The shared checks are in
  `exact.rs`.
- **A tab, a newline or a carriage return that is written as itself in an attribute value is refused.** An XML parser
  reads one as a space. `--to=xml` never writes one, so it can only come from another tool or from a hand edit, and
  reading it as a space would change a string without a word, which is what the format exists to prevent. The error is
  at the character and has a note that says to write `&#10;`. This is the choice that can be undone: reading it as a
  space later is a compatible change, and refusing it after people rely on it would not be.
- **The root is `values`, and a document whose root is a value is an error** with a note that says to wrap it. A
  document is a list, as [decision 40](0040-xml-is-written-with-attributes-and-a-values-root.md) says, so a lone value
  is not guessed to be a list of one.
- **A document is read whole,** since the end of the root is known only at the end, and then the values are written one
  by one. A syntax error writes nothing from the document. An error in a value comes after the values before it, which
  have been written, and the run stops there.
- **A character that XML 1.0 cannot hold is refused,** as itself and as a numeric reference, such as `&#1;`, since a
  reference may only name a character that is allowed in a document.

## Consequences

- The reader refuses some documents that a general XML parser would accept, such as one with a DOCTYPE that declares no
  entities, or one with a processing instruction. Each is refused with a message that says which.
- A document that `--to=xml` wrote reads back to the same values, and the same values as the JSON route gives. The tests
  check that, and that every character that XML 1.0 can hold survives a round trip.
- The rules for attribute values are ours to keep right, and the tests for them are the ones in the task: the entities,
  a literal newline, a numeric reference to a forbidden character, a DOCTYPE with an entity, the nesting limit and a
  document that is cut off.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- A crate such as `quick-xml` or `roxmltree`: less code, and the handling of entities and of the rest of XML to be
  configured and kept right, with the refusals above made by a check of the tree afterwards. The decision was close,
  and the control of entities is what tipped it.
- Reading a literal line break in a value as a space, as the standard says: it follows the standard, and it loses the
  exactness of a string that was passed through a tool that wrapped a line.
- Reading a document whose root is a value as a list of one: more forgiving, and it makes the shape of what is read
  depend on how many values there are, which decision 40 avoids.
