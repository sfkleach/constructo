# Task: Convert to and from XML

## Overview

Add a third notation to `constructo convert`: XML. It is the same idea as `json-exact`, and it follows
[the convert subcommand task](convert-subcommand.md), whose Steps 1 to 8 are the model for these two:

```
constructo convert --to=xml shapes.values > shapes.xml
constructo convert --from=xml shapes.xml > shapes.values
```

There is one format, `xml`, and it is exact, in the sense of `json-exact`: every value is written with its kind, and
each number with the text that it was written with, so the values can be rebuilt from it and no types are needed. The
comments, the layout and the spelling of the escapes in a string are not kept. A natural, typed XML, such as
`<Circle><r>5</r></Circle>`, would need the types to be read back, as `json-typed` does, and is a separate format that
is not part of this task. Because only one XML format exists, the name has no `-exact` suffix. If a typed one is added
later, it is named `xml-typed`, and `xml` is then kept as it is.

These two steps are numbered 1 and 2 within this document. In the stack of branches they follow Step 8 of the convert
subcommand task.

## The decisions that shape both steps

These were settled before the task was written, and Step 1 records them as a decision in `docs/decisions/`.

- **One strategy, exact.** XML text has no numbers, booleans or `null`, so natural XML needs types. The exact form needs
  none.
- **Every value is an empty element.** A leaf is written as an element with its text in an attribute, and a container
  has only elements in it, so there is no text content anywhere. Text content is where XML's treatment of white space is
  inconsistent (it depends on the schema and on the tool whether indentation is part of the value), and an attribute
  keeps a string exactly, spaces included.
- **The names of the attributes are fixed:** `value` holds the text of a literal (`nat`, `int`, `float`, `string`), and
  `name` holds a name (`const`, `constructor` and `field`). A name is never made an element name, because a field name
  is not always a valid XML name and a qualified name such as `shapes::Circle` never is, as XML reads the colons as a
  namespace prefix.
- **The document is always a list.** The root element is `<values>`, which is not a kind of value, and holds zero or
  more values in input order. There is no `--to-array` and no case of one value. A list of no values is valid.
- **The XML is version 1.0, in UTF-8.** XML 1.1 would allow a few more characters as references but still not U+0000,
  and it is not read by many of the tools that read XML.
- **The parser is our own,** as for JSON, in a new module. It reads a small, strict subset, and it takes the processing
  of entities into our own hands, so that no declaration or expansion can happen. See Step 2.

## Step 1: Convert to XML

Implement `--to=xml` in the subcommand `convert`. Example:

```
constructo convert --to=xml shapes.values > shapes.xml
```

### The encoding

The output starts with the declaration `<?xml version="1.0" encoding="UTF-8"?>`, and then the root `<values>`.

| A value           | Is written as                                                                               |
|-------------------|---------------------------------------------------------------------------------------------|
| `5`               | `<nat value="5"/>`                                                                          |
| `+5`, `-7`        | `<int value="+5"/>`, `<int value="-7"/>`                                                    |
| `1.50`            | `<float value="1.50"/>`                                                                     |
| `"hi"`            | `<string value="hi"/>`                                                                      |
| `[1, 2]`          | `<array><nat value="1"/><nat value="2"/></array>`                                           |
| `{ b: 1, a: 2 }`  | `<record><field name="b"><nat value="1"/></field><field name="a">...</field></record>`      |
| `red`             | `<const name="red"/>`                                                                       |
| `Quit`            | `<constructor name="Quit"/>`                                                                |
| `Move { x: 1 }`   | `<constructor name="Move"><record>...</record></constructor>`                               |
| `Write("hi")`     | `<constructor name="Write"><tuple><string value="hi"/></tuple></constructor>`               |
| `Span(1, end: 3)` | `<constructor name="Span"><tuple>...</tuple><named>...</named></constructor>`               |
| `Users[u1, Bob]`  | `<constructor name="Users"><array>...</array></constructor>`                                |

(The examples are shown on one line each, and `...` stands for the elements inside. The pretty output puts one element
on each line, as below.)

- **The correspondence with `json-exact` is one to one.** `nat`, `int`, `float`, `string`, `array`, `record`, `const`
  and `constructor` are the same kinds, a record is a `record` of `field` elements, and a constructor has at most one
  body, `record`, `tuple` or `array`, and then `named` after a `tuple`. The order is always `name`, body, `named`.
- **A field** is `<field name="b">` with exactly one value in it. It is used for the members of a record and for the
  named arguments of a tuple. The fields keep the order in which they were written.
- **An empty container is empty:** `<array/>`, `<record/>`, `<tuple/>`. As in `json-exact`, an empty body is kept, so
  `Ping()` is `<constructor name="Ping"><tuple/></constructor>`, `Move {}` has `<record/>` and `Users[]` has
  `<array/>`, and none of them is the same as no body at all. A `Span(start: 2)` has an empty `<tuple/>` and then
  `<named>`.
- **Numbers keep their text,** exactly as written: `+5` keeps its plus, `1.50` its trailing zero and `-2.5e3` its
  exponent.
- **A name** is kept as it was written, with its qualifier, and is a `const` if the last part of it starts with a
  lower-case letter or `_` and a `constructor` if it starts with a capital letter, as for `json-exact`.
- **The text of an attribute** is written in double quotes. The characters `&`, `<`, `>` and `"` are written as
  `&amp;`, `&lt;`, `&gt;` and `&quot;`. A tab, a newline and a carriage return are always written as `&#9;`, `&#10;`
  and `&#13;`, because an XML parser reads a literal one in an attribute as a space, which would change the string.
  All other characters are written as they are, in UTF-8.
- **A string that XML 1.0 cannot hold is an error.** XML 1.0 has no way to write the control characters U+0000 to
  U+001F other than tab, newline and carriage return, nor U+FFFE and U+FFFF, not even as a numeric reference. A string
  with one of them, such as `"\u0001"`, is an error at the string, with exit code 1, that names the character:
  `ERROR: The string holds U+0001, which XML 1.0 cannot write (shapes.values, line 3, col 9)`. (A name is made of
  letters and digits, and cannot hold one.)

### Layout

- **Pretty, the default:** one element on each line, indented by 2 spaces for each level, and a newline at the end.
  There is no rule about width, so the output for a value never depends on what is around it. Every element with
  nothing in it is self-closing.
- **`--compact`:** the declaration, a newline, and then the whole document on one line with no white space between
  elements.

For example, `Move { x: 1, y: 2 };` and `red;` are written as:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<values>
  <constructor name="Move">
    <record>
      <field name="x">
        <nat value="1"/>
      </field>
      <field name="y">
        <nat value="2"/>
      </field>
    </record>
  </constructor>
  <const name="red"/>
</values>
```

### Inputs, options and errors

- **The inputs are as for `json-exact`:** the `FILES` and then `--stdin`, in that order, and only values files. A types
  file is an error with exit code 1, a file that cannot be read is exit code 2, and with no input at all it is a usage
  error.
- **One document is written for the whole run,** with the values of all the inputs in order, and the root is written
  even when there are none, as `<values/>`, which is exit code 0. The inputs are all read before anything is written,
  so if there is an error, nothing is written to the standard output.
- **The options.** `--compact`, `-x`, `--stdin`, `--config` and `--cache` are accepted. `--to-array` is refused:
  `ERROR: --to-array does not apply to xml, which always writes a list of values`, exit code 2. The options for types
  (`--project`, `--no-prelude` and the others) are refused as they are for `json-exact`, with the same message, since
  `xml` needs no types.
- **A name given twice** in a record or in the named arguments of a tuple is an error at the second, with the messages
  of the type check, as for `json-exact`. XML could hold both, but the two formats are kept to the same inputs, so that
  a values file that converts to one converts to the other.
- **The formats.** `xml` is matched exactly, as the others are. The message for a name that is not a format lists it:
  `ERROR: NAME is not a format; choose json-exact, jsonl-exact, json-typed, jsonl-typed or xml`, and the message for a
  bare `json` is unchanged.
- **The exit codes are those of `json-exact`.** The one new case of exit code 1 is the string that XML cannot hold.

### Tasks

- [ ] A writer of XML in a new module, with the escaping above and the two layouts, and unit tests for every kind of
      value, for the empty containers, for the escapes (each of `&`, `<`, `>`, `"`, tab, newline and carriage return)
      and for the characters that cannot be written.
- [ ] `--to=xml` in the command, with `--to-array` refused, and the message for a format that does not exist updated
      in the code, the spec and the tests.
- [ ] Examples in `syntax/examples/convert/`: `kinds.xml` and `kinds.compact.xml` from `kinds.values`, one with
      strings that need escapes, and one that holds a character that XML cannot hold.
- [ ] Functional tests in `tests/functest.sh`: the output, the compact form, no values, several inputs and `--stdin`,
      the options that are refused, a types file, a duplicate name, and the string that XML cannot hold.
- [ ] The spec: a section for `xml` in `docs/specs/convert-command.md`, the options and exit-code tables, and
      `docs/specs/exit-codes.md` and `docs/specs/config-file.md` where they list the formats.
- [ ] A decision, `docs/decisions/0040-xml-is-written-with-attributes-and-a-values-root.md`, that records the choices
      above and why.
- [ ] `CHANGELOG.md`, `README.md` and `syntax/examples/README.md`.

## Step 2: Convert from XML

Given an XML file that `--to=xml` wrote, translate it back into a data-values file.

```
constructo convert --from=xml shapes.xml > shapes.values
```

### Inputs and options

- `--from=xml` is exclusive of `--to`, as the other formats are, and always writes data-values, to the standard output.
- The inputs are the `FILES` and then `--stdin`. Each is one XML document, and the values of all of them are written
  in order.
- The options for types, `--to-array` and `-x` are refused with `--from`, as for `--from=json-exact`, and so is
  `--type`. `--compact`, `--stdin`, `--config` and `--cache` are accepted.
- An empty file, or one with only white space, is not an XML document, and is a syntax error. `<values/>` is a
  document with no values, and writes nothing and exits with 0.

### Reading a value

The reader is strict. The root is `<values>`, and each element in it is a value in the shape that `--to=xml` writes. A
document whose root is a value, such as `<nat value="5"/>`, is an error at the root with a note that says to wrap it in
`<values>`, and so is any other root element.

- **An element has exactly the attributes of its kind:** `value` for a `nat`, `int`, `float` or `string`, and `name`
  for a `const`, a `constructor` and a `field`. An unknown, a missing or a repeated attribute is an error at the
  attribute, or at the element if one is missing.
- **A leaf has no content.** A `nat` with an element in it, or a `record` with a `value`, is an error.
- **The children are what the kind allows.** An `array` holds values, a `record` holds `field` elements, a `field`
  holds exactly one value, and a `tuple` holds values. A `constructor` holds at most one of `record`, `tuple` and
  `array`, in that position, and then `named` only after a `tuple`. A `named` holds `field` elements. Anything else
  is an error at the element.
- **A repeated `name` among the fields** of a `record` or a `named` is an error at the second, as a repeated key is in
  `json-exact`.
- **A number is checked by the lexer.** The text of `value` on a `nat`, `int` or `float` must read back as a literal
  of that kind: digits for a `nat`, a leading sign for an `int` and a fraction or an exponent for a `float`. Otherwise
  `<int value="5"/>` would be written as `5` and read back as a `Nat`. A number that is the wrong kind is an error at
  the attribute that says what to write, as it is for JSON.
- **A name is checked by the lexer,** as for `json-exact`: a `const` starts with a lower-case letter or `_`, a
  `constructor` with a capital, and a qualifier is allowed. A field or argument name is a lower-case name.

The values are written as for `--from=json-exact`: each ends with `;` and a new line, the layout is the same, with a
value on one line if it is at most 80 characters wide, and `--compact` writes each on one line.

### The XML that is read

The reader accepts a small subset of XML 1.0, and refuses the rest. Entity processing is our own, so that nothing is
declared, expanded or fetched.

- **Accepted:** an optional byte order mark, an optional declaration `<?xml version="1.0" encoding="UTF-8"?>` (the
  encoding may be left out, and any other encoding is an error), comments, and white space (space, tab, newline and
  carriage return) before the root, between elements and after the root. A comment is skipped.
- **Attributes** are in single or double quotes, in any order. A value holds the five predefined entities, `&amp;`,
  `&lt;`, `&gt;`, `&quot;` and `&apos;`, and numeric references, `&#10;` and `&#xA;`, that name a character that XML
  1.0 allows.
- **A literal tab, newline or carriage return in an attribute value is refused,** with a note: `A newline in an
  attribute value is read as a space by an XML parser; write &#10; to keep it`. The writer never writes one, so it
  comes from another tool or a hand edit, and a general parser would turn it into a space and change the string without
  a word. Refusing is also the choice that can be undone, since reading it as a space later is a compatible change.
- **Refused:** a DOCTYPE, so no entity declarations and no external entities, an entity reference other than the five
  (the message names it), CDATA sections, processing instructions other than the declaration, text between elements,
  namespaces (a name with a colon, and `xmlns`), and any name that is not one of the kinds above.
- **A reference to a character that XML 1.0 cannot hold,** such as `&#1;` or `&#0;`, is an error at the reference.
- **The nesting is limited** to 200 levels, as the other parsers are.
- **Every node has its line and column,** so that every problem is reported where it is, in the usual form, for
  example `ERROR: Expected a value attribute on nat (shapes.xml, line 7, col 14)`. A column counts characters.
- A document is read whole, since the closing of the root can only be known at the end, and the values are written
  one by one after it. So a syntax error writes nothing from that document, and an error in a value is found after
  the values before it have been written. The run stops at the first problem with exit code 1. An input that cannot be
  read is exit code 2.

### The parser

A parser of XML of our own, in a new module next to `json.rs`, for the subset above only, with positions. It is a
choice that was made with care. A general crate would give us a byte offset, not a line and column, and would read
what we want to refuse, and above all it would do the processing of entities. We would be left to configure it to stop
what it does by default. The cost is that we own the rules for attribute values, which are the subtle part, and the
tests for them are listed below.

### Tasks

- [ ] A parser of the subset, with positions, and unit tests for each thing accepted and for each thing refused,
      including the entities, a literal newline in a value, a numeric reference to a forbidden character, a DOCTYPE
      with an entity, the nesting limit, and a document that is cut off.
- [ ] The reader of `xml`, with unit tests for every kind of value and for every error, and round trips: every file in
      `syntax/examples/convert/` that `--to=xml` writes reads back as the same values, and a string made of every
      character that XML 1.0 can hold survives the round trip.
- [ ] `--from=xml` in the command, with the refusals above, the format name added to the message for one that is not
      a format, and `--from` and `--to` exclusive as before.
- [ ] Functional tests: the round trip from `--to=xml`, no values, several inputs and `--stdin`, an empty file, a
      root that is a value, the refused XML (a DOCTYPE, CDATA, a literal newline in a value), an unknown and a
      missing attribute, a number of the wrong kind, an error position, and an unreadable input.
- [ ] The spec section for reading back `xml`, the tables, and the exit codes, `docs/specs/exit-codes.md` and
      `docs/specs/config-file.md`.
- [ ] A decision, `docs/decisions/0041-reading-xml-back-is-strict-with-a-parser-of-our-own.md`: the strict subset, the
      refusal of a literal newline, and why the parser is our own.
- [ ] `CHANGELOG.md`, `README.md` and `syntax/examples/README.md`.

## Defaults that were chosen without a question

Each of these is small and can be changed. They are listed so that they are reviewed.

- The XML declaration is always written, and always says `encoding="UTF-8"`.
- `>` is written as `&gt;`, though XML only needs it after `]]`. It costs nothing and is safe.
- The reader accepts single quotes and `&apos;`, though the writer writes neither, because a hand edit may use them.
- The reader accepts a declaration without `encoding`, and refuses `standalone`.
- A BOM is accepted on reading and never written.
- The root element is called `values`, and `field`, `named` and `tuple` are the names of the elements that `json-exact`
  has as keys.
