//! The program's own colours, which existed before Constructo was used.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Colour {
    Red,
    Green,
    Blue,
    /// A colour that the drawing format has no name for.
    Custom(u32),
}
