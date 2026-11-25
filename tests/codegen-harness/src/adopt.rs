//! The functions of the adoptions, which the generated code calls: each from the arguments of a constructor, or from
//! the generated enum, and back, and each may refuse.

use crate::paint::Colour;
use crate::scenario;

pub fn uuid_from_string(text: String) -> Result<foreign::Uuid, String> {
    foreign::Uuid::parse(&text)
}

pub fn uuid_to_string(id: &foreign::Uuid) -> Result<String, String> {
    Ok(id.hyphenated())
}

/// The arguments of `Point(x: Float, y: Float)`, as a tuple in the order of the parameters.
pub fn point_from((x, y): (f64, f64)) -> Result<foreign::Point, String> {
    Ok(foreign::Point(x, y))
}

pub fn point_to(point: &foreign::Point) -> Result<(f64, f64), String> {
    Ok((point.0, point.1))
}

pub fn colour_from_generated(colour: scenario::Colour) -> Result<Colour, String> {
    Ok(match colour {
        scenario::Colour::Red => Colour::Red,
        scenario::Colour::Green => Colour::Green,
        scenario::Colour::Blue => Colour::Blue,
    })
}

pub fn colour_to_generated(colour: &Colour) -> Result<scenario::Colour, String> {
    match colour {
        Colour::Red => Ok(scenario::Colour::Red),
        Colour::Green => Ok(scenario::Colour::Green),
        Colour::Blue => Ok(scenario::Colour::Blue),
        Colour::Custom(rgb) => Err(format!("The colour #{:06x} has no Constructo form", rgb)),
    }
}
