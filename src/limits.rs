//! The limits that constructo puts on what it reads and on the work that it does, gathered in one place so that they
//! can be seen, and changed, together.
//!
//! Every limit is here to protect against one of two things: a recursive part of the program growing its stack until
//! the process crashes, or a small input expanding into an excessive amount of work. A limit is not added for any
//! other reason.
//!
//! The depth limit of the parser of data-values, `MAX_DEPTH`, is defined in `constructo-runtime`, which the parser is
//! part of, and is shown here with the rest.
//!
//! The depths are related. The JSON and XML parsers spend more than one level on a value, so their limits are derived
//! from `MAX_DEPTH`, so that they do not refuse a document that `convert` writes from a value that the parser
//! accepted. See `docs/decisions/0043-depth-limits-are-aligned-across-formats.md`.

pub use constructo_runtime::limits::MAX_DEPTH;

/// The deepest nesting of arrays and objects that the JSON parser accepts, which keeps the stacks of the parser and of
/// the readers bounded. `json-exact` spends at most two levels on a value, as in `{"array": [...]}`, and a document of
/// several values is an array of them, which is one more.
pub const MAX_JSON_DEPTH: u32 = 2 * MAX_DEPTH + 1;

/// The deepest nesting of elements that the XML parser accepts, which keeps the stacks of the parser and of the reader
/// bounded. `xml` spends at most three elements on a value, as in `<constructor><record><field>`, and the root element
/// `values` is one more.
pub const MAX_XML_DEPTH: u32 = 3 * MAX_DEPTH + 1;

/// The most levels of values, and of the consts that they refer to, that `json-typed` encodes. A const can refer to
/// another, and a long chain of them would otherwise be as deep as it is long. Each level takes a few kilobytes of
/// stack in a debug build, so the limit is chosen to stay well inside the stack of a test thread, which is the
/// smallest that the encoder runs on.
pub const MAX_TYPED_NESTING: usize = 300;

/// The most nodes that the `json-typed` encoding of one value may have. Depth is limited by `MAX_TYPED_NESTING`, but a
/// const that is referred to twice by another const is written out in full each time, so the size can double at every
/// level.
pub const MAX_TYPED_NODES: usize = 1_000_000;

/// The most levels of arrays, records and type arguments that a type may have when its default value is worked out.
/// Substituting arguments can make a type deeper than any that is written, so this is larger than `MAX_DEPTH`. It is
/// chosen to keep well inside the smallest stack that the checker might run on: freeing a type uses a little under
/// 200 bytes of stack per level in a debug build, so this takes about 200 KB.
pub const MAX_TYPE_DEPTH: u32 = 1000;

/// The default for the most nodes that the inferred type of a const may have. Types share their parts, so a short value
/// can have a type that is exponentially large when written out, and generalising it would take as long.
pub const DEFAULT_INFER_LIMIT: usize = 10_000;

/// The default for the most nodes that the default value of an omitted argument may have. A default is the first
/// constructor of an enum, filled in all the way down, so a few short declarations can make one that doubles in size
/// at each level.
pub const DEFAULT_VALUE_LIMIT: usize = 10_000;

/// The most digits that a whole number made by a query may have. Integers are unbounded, so a few multiplications, or
/// a cast of a `Float` with a large exponent such as `1.0e999999999 as Nat`, could otherwise ask for more memory and
/// time than any real query needs.
pub const MAX_NUMBER_DIGITS: usize = 1_000_000;
