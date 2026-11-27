//! The functions that the adoption file names. They are the program's own code.

use crate::{paint, shapes};

pub fn uuid_from_string(text: String) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(&text).map_err(|e| format!("{text:?} is not a UUID: {e}"))
}

pub fn uuid_to_string(id: &uuid::Uuid) -> Result<String, String> {
    Ok(id.hyphenated().to_string())
}

pub fn colour_from_generated(colour: shapes::Colour) -> Result<paint::Colour, String> {
    Ok(match colour {
        shapes::Colour::Red => paint::Colour::Red,
        shapes::Colour::Green => paint::Colour::Green,
        shapes::Colour::Blue => paint::Colour::Blue,
    })
}

pub fn colour_to_generated(colour: &paint::Colour) -> Result<shapes::Colour, String> {
    match colour {
        paint::Colour::Red => Ok(shapes::Colour::Red),
        paint::Colour::Green => Ok(shapes::Colour::Green),
        paint::Colour::Blue => Ok(shapes::Colour::Blue),
        paint::Colour::Custom(rgb) => Err(format!("The colour #{rgb:06x} has no Constructo form")),
    }
}
