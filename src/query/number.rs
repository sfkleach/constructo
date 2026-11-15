//! Numbers in queries: comparison, arithmetic and casts, as decision 44 and the query task settle them.
//!
//! A number keeps its text, as it is written in a values file, so that an untouched literal is written back unchanged.
//! `Nat` and `Int` are unbounded. `Float` arithmetic is `f64`. Comparison is exact on the text, so `1.50 == 1.5`, and a
//! new number is written so that it reads back as its own kind: a `Nat` as plain digits, an `Int` always with a sign,
//! and a `Float` as the shortest text that reads back as the same `f64`, always with a `.`.

use super::syntax::{BinOp, NumKind};
use crate::limits::MAX_NUMBER_DIGITS;
use num_bigint::BigInt;
use num_traits::{Signed, Zero};
use std::cmp::Ordering;

/// The kind of a number from its text, which is as the lexer of a values file wrote it.
pub fn kind_of(text: &str) -> NumKind {
    if text.contains('.') {
        NumKind::Float
    } else if text.starts_with('+') || text.starts_with('-') {
        NumKind::Int
    } else {
        NumKind::Nat
    }
}

/// A number as a sign, its significant digits and an exponent: the value is `0.digits × 10^exp`. Zero has no digits.
/// It is exact for any text whose exponent fits an `i64`, so it is how numbers are compared. The exponent is kept as an
/// `i128`, so that adding the length of the digits to it cannot overflow.
#[derive(Debug, PartialEq, Eq)]
struct Decimal {
    negative: bool,
    digits: Vec<u8>,
    exp: i128,
}

impl Decimal {
    fn parse(text: &str) -> Result<Decimal, NumError> {
        let (negative, rest) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        let (mantissa, exponent) = match rest.find(['e', 'E']) {
            Some(at) => match rest[at + 1..].parse::<i64>() {
                Ok(exponent) => (&rest[..at], i128::from(exponent)),
                // A number is never changed, so an exponent that cannot be held is an error, not a different number.
                Err(_) => return Err(format!("The exponent of {} is too large to work with", text)),
            },
            None => (rest, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).map(|b| b - b'0').collect();
        let mut exp = exponent + whole.len() as i128;
        let leading = digits.iter().take_while(|d| **d == 0).count();
        digits.drain(..leading);
        exp -= leading as i128;
        while digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            return Ok(Decimal {
                negative: false,
                digits,
                exp: 0,
            });
        }
        Ok(Decimal { negative, digits, exp })
    }

    fn cmp(&self, other: &Decimal) -> Ordering {
        let sign = |d: &Decimal| {
            if d.digits.is_empty() {
                0
            } else if d.negative {
                -1
            } else {
                1
            }
        };
        let (a, b) = (sign(self), sign(other));
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        let magnitude = self.exp.cmp(&other.exp).then_with(|| self.digits.cmp(&other.digits));
        if a < 0 {
            magnitude.reverse()
        } else {
            magnitude
        }
    }

    /// Whether the number is a whole number.
    fn is_whole(&self) -> bool {
        self.exp >= self.digits.len() as i128
    }
}

/// Compares two numbers of the same kind, exactly. It is an error only for an exponent too large to hold.
pub fn compare(a: &str, b: &str) -> Result<Ordering, NumError> {
    Ok(Decimal::parse(a)?.cmp(&Decimal::parse(b)?))
}

/// Why a number could not be made.
pub type NumError = String;

fn big(text: &str) -> BigInt {
    let digits = text.strip_prefix('+').unwrap_or(text);
    digits
        .parse()
        .expect("the text of a whole number is digits with an optional sign")
}

fn nat_text(n: &BigInt) -> String {
    n.to_string()
}

fn int_text(n: &BigInt) -> String {
    if n.is_negative() {
        n.to_string()
    } else {
        format!("+{}", n)
    }
}

/// The text of a new `Float`: the shortest that reads back as the same `f64`, always with a `.`, so that it reads
/// back as a `Float`.
pub fn float_text(f: f64) -> String {
    let text = format!("{:?}", f);
    match text.find('e') {
        Some(at) if !text[..at].contains('.') => format!("{}.0{}", &text[..at], &text[at..]),
        _ => text,
    }
}

/// The `f64` of a number's text. A number too large for an `f64` is an error, not an infinity.
pub fn to_f64(text: &str) -> Result<f64, NumError> {
    let f: f64 = text.parse().map_err(|_| format!("{} is not a number", text))?;
    if f.is_infinite() {
        return Err(format!("{} is too large to be a Float", text));
    }
    Ok(f)
}

/// Converts a number to another kind. A cast that would change the number is an error, except that `as Float` rounds
/// to the nearest `f64`, as `Float` arithmetic does. A cast to the same kind does nothing.
pub fn cast(text: &str, to: NumKind) -> Result<String, NumError> {
    let from = kind_of(text);
    if from == to {
        return Ok(text.to_string());
    }
    if to == NumKind::Float {
        return Ok(float_text(to_f64(text)?));
    }
    let decimal = Decimal::parse(text)?;
    if !decimal.is_whole() {
        return Err(format!(
            "{} is not a whole number, so it cannot be cast to {}",
            text,
            to.name()
        ));
    }
    if decimal.negative && to == NumKind::Nat {
        return Err(format!("{} is negative, so it cannot be cast to Nat", text));
    }
    if decimal.exp > MAX_NUMBER_DIGITS as i128 {
        return Err(format!("{} has too many digits to be cast to {}", text, to.name()));
    }
    let mut digits: String = decimal.digits.iter().map(|d| char::from(b'0' + d)).collect();
    if digits.is_empty() {
        digits.push('0');
    } else {
        digits.extend(std::iter::repeat_n('0', decimal.exp as usize - decimal.digits.len()));
    }
    let n: BigInt = digits.parse().expect("digits make a whole number");
    let n = if decimal.negative { -n } else { n };
    Ok(match to {
        NumKind::Nat => nat_text(&n),
        NumKind::Int => int_text(&n),
        NumKind::Float => unreachable!("handled above"),
    })
}

/// Applies an arithmetic operator to two numbers of the same kind.
pub fn arithmetic(op: BinOp, kind: NumKind, a: &str, b: &str) -> Result<String, NumError> {
    arithmetic_within(op, kind, a, b, MAX_NUMBER_DIGITS)
}

/// As `arithmetic`, with the most digits that a whole number may have, so that the limit can be tested cheaply.
fn arithmetic_within(op: BinOp, kind: NumKind, a: &str, b: &str, limit: usize) -> Result<String, NumError> {
    if kind == NumKind::Float {
        let (x, y) = (to_f64(a)?, to_f64(b)?);
        let result = match op {
            BinOp::Add => x + y,
            BinOp::Sub => x - y,
            BinOp::Mul => x * y,
            BinOp::Div | BinOp::Rem if y == 0.0 => return Err("Division by zero".to_string()),
            BinOp::Div => x / y,
            BinOp::Rem => x % y,
            _ => unreachable!("only arithmetic operators reach here"),
        };
        if !result.is_finite() {
            return Err(format!(
                "The result of {} {} {} is too large to be a Float",
                a,
                op.symbol(),
                b
            ));
        }
        return Ok(float_text(result));
    }
    let (x, y) = (big(a), big(b));
    // A bound on the bits, which is cheap, keeps the work bounded; the limit itself is on the decimal digits, and is
    // checked on the text of the result. A decimal digit is more than 3 bits, so this bound never refuses a result
    // that the limit allows.
    let too_long = |n: &BigInt| n.bits() > (limit as u64) * 4;
    let result = match op {
        BinOp::Add => x + y,
        BinOp::Sub => x - y,
        BinOp::Mul => {
            if x.bits() + y.bits() > (limit as u64) * 4 {
                return Err(format!("The result of {} * {} would have too many digits", a, b));
            }
            x * y
        }
        BinOp::Div | BinOp::Rem if y.is_zero() => return Err("Division by zero".to_string()),
        BinOp::Div => x / y,
        BinOp::Rem => x % y,
        _ => unreachable!("only arithmetic operators reach here"),
    };
    if too_long(&result) {
        return Err(format!("The result of {} {} {} has too many digits", a, op.symbol(), b));
    }
    let text = match kind {
        NumKind::Nat if result.is_negative() => {
            return Err(format!(
                "{} {} {} is below zero, which a Nat cannot be; cast to Int with `as Int` to go negative",
                a,
                op.symbol(),
                b
            ))
        }
        NumKind::Nat => nat_text(&result),
        _ => int_text(&result),
    };
    let digits = text.trim_start_matches(['+', '-']).len();
    if digits > limit {
        return Err(format!("The result of {} {} {} has too many digits", a, op.symbol(), b));
    }
    Ok(text)
}

/// Negates a number of a kind. A `Nat` cannot be negated, except zero.
pub fn negate(text: &str, kind: NumKind) -> Result<String, NumError> {
    match kind {
        NumKind::Float => Ok(float_text(-to_f64(text)?)),
        NumKind::Int => Ok(int_text(&-big(text))),
        NumKind::Nat if big(text).is_zero() => Ok(text.to_string()),
        NumKind::Nat => Err(format!(
            "-{} is below zero, which a Nat cannot be; cast to Int with `as Int`",
            text
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare_ok(a: &str, b: &str) -> Ordering {
        compare(a, b).unwrap()
    }

    #[test]
    fn an_exponent_too_large_to_hold_is_an_error_and_not_another_number() {
        assert!(compare("1.0e9223372036854775808", "1.0")
            .unwrap_err()
            .contains("too large"));
        assert!(cast("1.0e9223372036854775808", NumKind::Nat).is_err());
        assert_eq!(
            compare_ok("1.0e9223372036854775807", "1.0e9223372036854775806"),
            Ordering::Greater
        );
    }

    #[test]
    fn the_limit_on_digits_is_in_decimal_digits() {
        // 10 digits times 10 is 11 digits, which the bound on the bits allows but the limit does not.
        assert_eq!(
            arithmetic_within(BinOp::Mul, NumKind::Nat, "1000000000", "1", 10).unwrap(),
            "1000000000"
        );
        let e = arithmetic_within(BinOp::Mul, NumKind::Nat, "1000000000", "10", 10).unwrap_err();
        assert!(e.contains("too many digits"), "{}", e);
    }

    #[test]
    fn comparison_is_exact_on_the_text() {
        assert_eq!(compare_ok("1.50", "1.5"), Ordering::Equal);
        assert_eq!(compare_ok("+5", "5"), Ordering::Equal);
        assert_eq!(compare_ok("-0", "0"), Ordering::Equal);
        assert_eq!(compare_ok("1.5e3", "1500.0"), Ordering::Equal);
        assert_eq!(compare_ok("0.1", "0.10000000000000001"), Ordering::Less);
        assert_eq!(compare_ok("-2.0", "-10.0"), Ordering::Greater);
        assert_eq!(compare_ok("99", "100"), Ordering::Less);
        assert_eq!(compare_ok("0.001", "0.01"), Ordering::Less);
    }

    #[test]
    fn arithmetic_keeps_the_kind() {
        assert_eq!(arithmetic(BinOp::Add, NumKind::Nat, "8080", "1").unwrap(), "8081");
        assert_eq!(arithmetic(BinOp::Sub, NumKind::Int, "+3", "+5").unwrap(), "-2");
        assert_eq!(arithmetic(BinOp::Add, NumKind::Int, "+3", "+5").unwrap(), "+8");
        assert!(arithmetic(BinOp::Sub, NumKind::Nat, "3", "5")
            .unwrap_err()
            .contains("below zero"));
        assert_eq!(
            arithmetic(BinOp::Add, NumKind::Nat, "18446744073709551615", "1").unwrap(),
            "18446744073709551616"
        );
        assert_eq!(arithmetic(BinOp::Div, NumKind::Nat, "7", "2").unwrap(), "3");
        assert_eq!(arithmetic(BinOp::Rem, NumKind::Nat, "7", "2").unwrap(), "1");
        assert_eq!(arithmetic(BinOp::Div, NumKind::Int, "-7", "+2").unwrap(), "-3");
        assert_eq!(
            arithmetic(BinOp::Add, NumKind::Float, "0.1", "0.2").unwrap(),
            "0.30000000000000004"
        );
        assert_eq!(arithmetic(BinOp::Mul, NumKind::Float, "1.5", "2.0").unwrap(), "3.0");
        assert!(arithmetic(BinOp::Div, NumKind::Nat, "1", "0").is_err());
        assert!(arithmetic(BinOp::Mul, NumKind::Float, "1.0e300", "1.0e300").is_err());
    }

    #[test]
    fn new_floats_read_back_as_floats() {
        assert_eq!(float_text(1e300), "1.0e300");
        assert_eq!(float_text(1.5e300), "1.5e300");
        assert_eq!(float_text(-2.0), "-2.0");
    }

    #[test]
    fn casts_that_keep_the_number() {
        assert_eq!(cast("5.0", NumKind::Nat).unwrap(), "5");
        assert_eq!(cast("+5", NumKind::Nat).unwrap(), "5");
        assert_eq!(cast("1.0e3", NumKind::Int).unwrap(), "+1000");
        assert_eq!(cast("-0", NumKind::Nat).unwrap(), "0");
        assert_eq!(cast("5", NumKind::Float).unwrap(), "5.0");
        assert_eq!(cast("1.50", NumKind::Float).unwrap(), "1.50");
        assert_eq!(
            cast("12345678901234567890123", NumKind::Float).unwrap(),
            "1.2345678901234568e22"
        );
        assert!(cast("-1", NumKind::Nat).unwrap_err().contains("negative"));
        assert!(cast("1.5", NumKind::Int).unwrap_err().contains("not a whole number"));
        assert!(cast(&format!("1{}", "0".repeat(400)), NumKind::Float)
            .unwrap_err()
            .contains("too large"));
        // A cast to the same kind does nothing, so a Float keeps its text.
        assert_eq!(cast("1.0e400", NumKind::Float).unwrap(), "1.0e400");
    }
}
