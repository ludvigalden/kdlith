//! Validation of bare KDL number spellings.

use crate::event::{NumberKind, Radix};

#[derive(Clone, Copy)]
enum State {
    Start,
    Signed,
    MaybeBased,
    DigitsStart,
    Digits,
    FractionStart,
    Fraction,
    ExponentStart,
    ExponentSigned,
    Exponent,
}

/// Classify a bare number spelling; `None` when it is not a KDL number.
pub(crate) fn classify(text: &str) -> Option<NumberKind> {
    use State::{
        Digits, DigitsStart, Exponent, ExponentSigned, ExponentStart, Fraction, FractionStart,
        MaybeBased, Signed, Start,
    };
    let mut radix = Radix::Decimal;
    let mut state = Start;
    for byte in text.bytes() {
        let digit = radix.is_digit(byte);
        let decimal = radix == Radix::Decimal;
        state = match (state, byte) {
            (Start, b'+' | b'-') => Signed,
            (Start | Signed, b'0') => MaybeBased,
            (Start | Signed, b'1'..=b'9') => Digits,
            (MaybeBased, b'x') => {
                radix = Radix::Hexadecimal;
                DigitsStart
            }
            (MaybeBased, b'o') => {
                radix = Radix::Octal;
                DigitsStart
            }
            (MaybeBased, b'b') => {
                radix = Radix::Binary;
                DigitsStart
            }
            (MaybeBased, b'0'..=b'9' | b'_') => Digits,
            (MaybeBased, b'.') => FractionStart,
            (MaybeBased, b'e' | b'E') => ExponentStart,
            (DigitsStart | Digits, _) if digit => Digits,
            (Digits, b'_') => Digits,
            (Digits, b'.') if decimal => FractionStart,
            (Digits, b'e' | b'E') if decimal => ExponentStart,
            (FractionStart | Fraction, _) if digit => Fraction,
            (Fraction, b'_') => Fraction,
            (Fraction, b'e' | b'E') => ExponentStart,
            (ExponentStart, b'+' | b'-') => ExponentSigned,
            (ExponentStart | ExponentSigned | Exponent, b'0'..=b'9') => Exponent,
            (Exponent, b'_') => Exponent,
            _ => return None,
        };
    }
    match state {
        MaybeBased | Digits => Some(NumberKind::Integer(radix)),
        Fraction | Exponent => Some(NumberKind::Float),
        _ => None,
    }
}
