# 0040 - XML is written with attributes and a values root, 2026-11-11

## Issue

**Fork.** `constructo convert --to=xml` writes values as XML. XML has no numbers, booleans or `null`, every leaf is
text, and its rules for white space differ between tools, so there are choices about which strategy it follows, where
the text of a value goes, what the document is, and which version of XML is written.

## Decision

- **One strategy, exact, and the format is named `xml`.** It keeps the kind of every value and the text of its numbers
  and needs no types, as `json-exact` does. A natural XML, such as `<Circle><r>5</r></Circle>`, would need the types to
  be read back, as `json-typed` does, and is a separate format that is not part of this decision. If it is added, it is
  `xml-typed`, and `xml` keeps its meaning. So the name has no `-exact` suffix.
- **The text of a value is an attribute, and every element is empty or holds only elements.** `value` holds a number or
  a string, and `name` holds a name, so there is no text content. Text content is where XML's treatment of white space
  is inconsistent: whether the indentation between tags is part of a value depends on the schema and on the tool, and
  a pretty-printer cannot lay out an element with mixed content without changing it. An attribute keeps a string
  exactly, with the spaces at its ends.
- **A name is never an element name.** A field name is not always a valid XML name, and a qualified name such as
  `shapes::Circle` never is, since XML reads the colons as a namespace prefix. With a `name` attribute nothing needs to
  be mangled, and the order of the fields is kept.
- **A tab, a newline and a carriage return in a string are written as `&#9;`, `&#10;` and `&#13;`.** A parser reads one
  that is written as itself in an attribute as a space, so the string would change. `&`, `<`, `>` and `"` are written
  as entities, and everything else as it is, in UTF-8.
- **A string that XML 1.0 cannot hold is an error.** XML 1.0 cannot write the control characters U+0000 to U+001F other
  than tab, newline and carriage return, nor U+FFFE and U+FFFF, not even as a numeric reference, since a reference may
  only name a character that is allowed in a document. The error is at the string, exit code 1, and names the
  character. XML 1.1 is not used: it allows most of these as references but still not U+0000, and a document that
  declares it is refused by many tools.
- **The document is always a list: its root is `values`.** `values` is not a kind of value, so a document with one
  value and one with many have the same shape, there is no `--to-array` (it is refused, exit code 2), there is no
  warning that more values follow, and no values give `<values/>` with exit code 0. A list of nothing is a valid list,
  and nothing is wrong with the inputs, which differs from `json-exact`, where no value is an error.
- **A name given twice is an error, as for `json-exact`,** though XML could hold both. The two formats are kept to the
  same inputs, so that a file that converts to one converts to the other.
- **One element on each line, always, with no rule about width.** The output for a value never depends on what is around
  it, and a change to one value changes one line. `--compact` writes the document on one line, after the declaration.
- **The document is written whole.** Every input is read and every value is encoded before anything is written, so an
  error leaves the standard output empty, as for the other formats that write one document.

## Consequences

- The XML is longer than the JSON for small values, since a record of three fields takes eight lines.
- A string that has a control character cannot be converted to XML, though it can be to JSON. That is a limit of XML,
  and the error says which character it is.
- Reading the XML back is [Step 2 of the task](../tasks/2026-11-08-convert-subcommand/to_xml.md), and a decision of its
  own, with a parser that reads only the subset that is written here.

## Options and Outcome

The chosen option is the one under Decision. Alternatives considered:

- The text of a value as the content of an element, as in `<nat>5</nat>`: shorter, and it has the white space problem
  above, for a string especially.
- The names as element names, as in `<Move><x>1</x></Move>`: closer to what people expect of XML, and it fails for a
  qualified name and for a field name that is not an XML name.
- XML 1.1: it can write a few more control characters, and still not all of them, in exchange for output that part of
  the audience cannot read.
- A root that is the value itself when there is one value: it needs `--to-array` and the warning back, and the shape of
  the document then depends on how many values there are.
