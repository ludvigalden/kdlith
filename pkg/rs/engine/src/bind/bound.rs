//! Exact comparison of a number's spelling against an unsigned bound.
//! Numbers stay their source spelling, so the comparison reads the
//! digits rather than routing through a binary float.

use std::cmp::Ordering;

/// The integral part of a magnitude: small enough for `u128`, or larger
/// than any bound can be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Whole {
    Small(u128),
    Huge,
}

/// How a number's spelling compares with `bound`, exactly; `None` for a
/// spelling with no finite value (`#inf`, `#-inf`, `#nan`) or one that
/// is not a number.
pub(super) fn compare(text: &str, bound: u64) -> Option<Ordering> {
    if text.starts_with('#') {
        return None;
    }
    let (negative, unsigned) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (whole, fraction) = magnitude(unsigned)?;
    if negative && (whole != Whole::Small(0) || fraction) {
        return Some(Ordering::Less);
    }
    Some(match whole {
        Whole::Huge => Ordering::Greater,
        Whole::Small(whole) => whole.cmp(&u128::from(bound)).then(if fraction {
            Ordering::Greater
        } else {
            Ordering::Equal
        }),
    })
}

/// An unsigned spelling's integral part and whether a non-zero fraction
/// follows it.
fn magnitude(text: &str) -> Option<(Whole, bool)> {
    let radix = [("0x", 16), ("0o", 8), ("0b", 2)]
        .into_iter()
        .find_map(|(prefix, radix)| text.strip_prefix(prefix).map(|rest| (rest, radix)));
    if let Some((digits, radix)) = radix {
        let digits: String = digits.chars().filter(|c| *c != '_').collect();
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return None;
        }
        let digits = digits.trim_start_matches('0');
        if digits.is_empty() {
            return Some((Whole::Small(0), false));
        }
        let whole = u128::from_str_radix(digits, radix).map_or(Whole::Huge, Whole::Small);
        return Some((whole, false));
    }
    let text: String = text.chars().filter(|c| *c != '_').collect();
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(at) => (&text[..at], exponent(&text[at + 1..])?),
        None => (text.as_str(), 0),
    };
    let (int, frac) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let decimal = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !decimal(int) || !decimal(frac) {
        return None;
    }
    let digits = format!("{int}{frac}");
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return Some((Whole::Small(0), false));
    }
    // The decimal point's position within the significant digits.
    let point = i64::try_from(digits.len()).ok()? + exponent - i64::try_from(frac.len()).ok()?;
    if point <= 0 {
        return Some((Whole::Small(0), true));
    }
    // u128::MAX has 39 digits; anything wider exceeds every bound.
    if point > 39 {
        return Some((Whole::Huge, false));
    }
    let point = usize::try_from(point).ok()?;
    let (whole, rest) = if point >= digits.len() {
        (format!("{digits}{}", "0".repeat(point - digits.len())), "")
    } else {
        (digits[..point].to_owned(), &digits[point..])
    };
    let fraction = rest.bytes().any(|b| b != b'0');
    let whole = whole.parse::<u128>().map_or(Whole::Huge, Whole::Small);
    Some((whole, fraction))
}

/// A signed decimal exponent, clamped far beyond any width that can
/// change a comparison with a `u64`.
fn exponent(text: &str) -> Option<i64> {
    const CLAMP: i64 = 1 << 40;
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = digits.parse::<i64>().unwrap_or(CLAMP).min(CLAMP);
    Some(if negative { -value } else { value })
}
