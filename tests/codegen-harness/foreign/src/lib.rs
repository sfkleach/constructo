//! A stand-in for a crate of a third party, such as `uuid`: its types are not the program's, so the orphan rule
//! applies.

/// A UUID, as sixteen bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uuid(pub [u8; 16]);

impl Uuid {
    /// Parses the hyphenated form, or gives why not.
    pub fn parse(text: &str) -> Result<Uuid, String> {
        let hex: String = text.chars().filter(|c| *c != '-').collect();
        if hex.len() != 32 || text.len() != 36 {
            return Err(format!("{:?} is not a UUID", text));
        }
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|_| format!("{:?} is not a UUID", text))?;
        }
        Ok(Uuid(bytes))
    }

    /// The hyphenated form, in lower case.
    pub fn hyphenated(&self) -> String {
        let hex: String = self.0.iter().map(|b| format!("{:02x}", b)).collect();
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    }
}

/// A point of another crate, which has no names for its parts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point(pub f64, pub f64);
