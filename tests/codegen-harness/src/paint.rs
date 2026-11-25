//! The program's own colour, which has one that Constructo's `Colour` has no constructor for.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Colour {
    Red,
    Green,
    Blue,
    Custom(u32),
}
