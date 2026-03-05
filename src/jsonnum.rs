/// Result of reading a JSON number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberResult {
    Error,
    Integer,
    Number,
}

/// Read a JSON number from the start of `s`.
/// Returns `(remaining, result)` where remaining is the slice after the number.
/// Uses 8-way unrolled digit scanning (matching Go's jsonnum.ReadNumber).
pub(crate) fn read_number(s: &[u8]) -> (&[u8], NumberResult) {
    let mut s = s;

    // Optional leading minus
    if s[0] == b'-' {
        s = &s[1..];
        if s.is_empty() {
            return (s, NumberResult::Error);
        }
    }

    // Leading zero
    if s[0] == b'0' {
        s = &s[1..];
        if s.is_empty() {
            return (s, NumberResult::Integer);
        }
        match s[0] {
            b'.' => {
                s = &s[1..];
                return read_fraction(s);
            }
            b'e' | b'E' => {
                s = &s[1..];
                return read_exponent(s);
            }
            _ => return (s, NumberResult::Integer),
        }
    }

    // Integer part: first digit must be 1-9
    if s.is_empty() || s[0] < b'1' || s[0] > b'9' {
        return (s, NumberResult::Error);
    }
    s = &s[1..];

    // 8-way unrolled integer digit scanning
    while s.len() >= 8 {
        macro_rules! check_int {
            ($i:expr) => {
                if s[$i] < b'0' || s[$i] > b'9' {
                    if s[$i] == b'e' || s[$i] == b'E' {
                        s = &s[$i + 1..];
                        return read_exponent(s);
                    } else if s[$i] == b'.' {
                        s = &s[$i + 1..];
                        return read_fraction(s);
                    }
                    s = &s[$i..];
                    return (s, NumberResult::Integer);
                }
            };
        }
        check_int!(0); check_int!(1); check_int!(2); check_int!(3);
        check_int!(4); check_int!(5); check_int!(6); check_int!(7);
        s = &s[8..];
    }
    // Fallback for remaining bytes
    for i in 0..s.len() {
        if s[i] < b'0' || s[i] > b'9' {
            if s[i] == b'e' || s[i] == b'E' {
                s = &s[i + 1..];
                return read_exponent(s);
            } else if s[i] == b'.' {
                s = &s[i + 1..];
                return read_fraction(s);
            }
            return (&s[i..], NumberResult::Integer);
        }
    }
    (&s[s.len()..], NumberResult::Integer)
}

fn read_fraction(s: &[u8]) -> (&[u8], NumberResult) {
    let mut s = s;
    // At least one digit required
    if s.is_empty() || s[0] < b'0' || s[0] > b'9' {
        return (s, NumberResult::Error);
    }
    s = &s[1..];

    // 8-way unrolled fraction digit scanning
    while s.len() >= 8 {
        macro_rules! check_frac {
            ($i:expr) => {
                if s[$i] < b'0' || s[$i] > b'9' {
                    if s[$i] == b'e' || s[$i] == b'E' {
                        s = &s[$i + 1..];
                        return read_exponent(s);
                    }
                    s = &s[$i..];
                    return (s, NumberResult::Number);
                }
            };
        }
        check_frac!(0); check_frac!(1); check_frac!(2); check_frac!(3);
        check_frac!(4); check_frac!(5); check_frac!(6); check_frac!(7);
        s = &s[8..];
    }
    // Fallback for remaining bytes
    for i in 0..s.len() {
        if s[i] < b'0' || s[i] > b'9' {
            if s[i] == b'e' || s[i] == b'E' {
                s = &s[i + 1..];
                return read_exponent(s);
            }
            return (&s[i..], NumberResult::Number);
        }
    }
    (&s[s.len()..], NumberResult::Number)
}

fn read_exponent(s: &[u8]) -> (&[u8], NumberResult) {
    let mut s = s;
    if s.is_empty() {
        return (s, NumberResult::Error);
    }
    // Optional sign
    if s[0] == b'-' || s[0] == b'+' {
        s = &s[1..];
    }
    // At least one digit required
    if s.is_empty() || s[0] < b'0' || s[0] > b'9' {
        return (s, NumberResult::Error);
    }
    s = &s[1..];

    // 8-way unrolled exponent digit scanning
    while s.len() >= 8 {
        macro_rules! check_exp {
            ($i:expr) => {
                if s[$i] < b'0' || s[$i] > b'9' {
                    s = &s[$i..];
                    return (s, NumberResult::Number);
                }
            };
        }
        check_exp!(0); check_exp!(1); check_exp!(2); check_exp!(3);
        check_exp!(4); check_exp!(5); check_exp!(6); check_exp!(7);
        s = &s[8..];
    }
    // Fallback for remaining bytes
    for i in 0..s.len() {
        if s[i] < b'0' || s[i] > b'9' {
            return (&s[i..], NumberResult::Number);
        }
    }
    (&s[s.len()..], NumberResult::Number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integers() {
        assert_eq!(read_number(b"0 "), (&b" "[..], NumberResult::Integer));
        assert_eq!(read_number(b"123 "), (&b" "[..], NumberResult::Integer));
        assert_eq!(read_number(b"-42,"), (&b","[..], NumberResult::Integer));
    }

    #[test]
    fn test_fractions() {
        assert_eq!(read_number(b"1.5 "), (&b" "[..], NumberResult::Number));
        assert_eq!(read_number(b"0.0}"), (&b"}"[..], NumberResult::Number));
    }

    #[test]
    fn test_exponents() {
        assert_eq!(read_number(b"1e10 "), (&b" "[..], NumberResult::Number));
        assert_eq!(read_number(b"-9.123e3,"), (&b","[..], NumberResult::Number));
        assert_eq!(read_number(b"1E+2]"), (&b"]"[..], NumberResult::Number));
    }

    #[test]
    fn test_errors() {
        assert_eq!(read_number(b"- ").1, NumberResult::Error);
        assert_eq!(read_number(b"1. ").1, NumberResult::Error);
        assert_eq!(read_number(b"1e ").1, NumberResult::Error);
        assert_eq!(read_number(b"01").1, NumberResult::Integer); // 0 then "1" is trailing
    }

    #[test]
    fn test_long_integer() {
        // Test the 8-way unrolled path with > 8 digits
        assert_eq!(read_number(b"1234567890 "), (&b" "[..], NumberResult::Integer));
        assert_eq!(read_number(b"12345678901234567890 "), (&b" "[..], NumberResult::Integer));
    }

    #[test]
    fn test_long_fraction() {
        // Test unrolled fraction path
        assert_eq!(read_number(b"1.1234567890 "), (&b" "[..], NumberResult::Number));
        assert_eq!(read_number(b"1.12345678e5 "), (&b" "[..], NumberResult::Number));
    }

    #[test]
    fn test_long_exponent() {
        // Test unrolled exponent path
        assert_eq!(read_number(b"1e1234567890 "), (&b" "[..], NumberResult::Number));
    }
}
