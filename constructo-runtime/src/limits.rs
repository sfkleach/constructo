//! The limits on what the runtime reads. The text that it parses may come from anywhere, so it is not trusted, and the
//! recursive parser would otherwise grow its stack with the nesting of the input until the process crashed.
//!
//! `constructo` shows this limit with its own, in its module `limits`, so that they can all be seen together.

/// The deepest nesting of values, and of types, in the text of a source, which keeps the stack of the recursive parser
/// bounded. Each value is a level, a scalar included, so `[[1]]` is three levels deep.
pub const MAX_DEPTH: u32 = 200;
