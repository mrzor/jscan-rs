use alloc::vec::Vec;

use crate::error::{error_index, Error, ErrorCode};
use crate::jsonnum::{self, NumberResult};
use crate::strfind;

const DEFAULT_STACK_SIZE: usize = 128;

/// Reusable JSON validator. More efficient than the free functions when validating multiple inputs.
pub struct Validator {
    stack: Vec<u8>, // 1 = object, 2 = array
}

impl Validator {
    /// Create a new validator with the given preallocated stack depth.
    pub fn new(prealloc_stack: usize) -> Self {
        Self {
            stack: Vec::with_capacity(prealloc_stack),
        }
    }

    /// Returns `true` if `s` is valid JSON.
    pub fn valid(&mut self, s: &[u8]) -> bool {
        self.validate(s).is_none()
    }

    /// Validate one JSON value from the start of `s`.
    /// Returns `(remaining, Option<Error>)`.
    pub fn validate_one<'a>(&mut self, s: &'a [u8]) -> (&'a [u8], Option<Error>) {
        self.stack.clear();
        validate_inner(&mut self.stack, s)
    }

    /// Validate that `s` is exactly one valid JSON value (with optional surrounding whitespace).
    pub fn validate(&mut self, s: &[u8]) -> Option<Error> {
        self.stack.clear();
        let (trailing, err) = validate_inner(&mut self.stack, s);
        if let Some(e) = err {
            return Some(e);
        }
        let (trailing, ctrl) = strfind::end_of_whitespace_seq(trailing);
        if ctrl {
            return Some(Error::new(
                ErrorCode::IllegalControlChar,
                error_index(s.len(), trailing.len()),
            ));
        }
        if !trailing.is_empty() {
            return Some(Error::new(
                ErrorCode::UnexpectedToken,
                error_index(s.len(), trailing.len()),
            ));
        }
        None
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new(DEFAULT_STACK_SIZE)
    }
}

/// Returns `true` if `s` is valid JSON.
pub fn valid(s: &[u8]) -> bool {
    validate(s).is_none()
}

/// Validate that `s` is exactly one valid JSON value.
pub fn validate(s: &[u8]) -> Option<Error> {
    Validator::default().validate(s)
}

/// Validate one JSON value from the start of `s`.
pub fn validate_one(s: &[u8]) -> (&[u8], Option<Error>) {
    let mut v = Validator::default();
    v.validate_one(s)
}

// Lookup tables
const LUT_STR: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut i = 0u8;
    while i < 0x20 {
        t[i as usize] = 1;
        i += 1;
    }
    t[b'"' as usize] = 1;
    t[b'\\' as usize] = 1;
    t
};

const LUT_ESCAPE: [u8; 256] = {
    let mut t = [0u8; 256];
    t[b'"' as usize] = 1;
    t[b'\\' as usize] = 1;
    t[b'/' as usize] = 1;
    t[b'b' as usize] = 1;
    t[b'f' as usize] = 1;
    t[b'n' as usize] = 1;
    t[b'r' as usize] = 1;
    t[b't' as usize] = 1;
    t
};

const LUT_HEX: [u8; 256] = {
    let mut t = [0u8; 256];
    t[b'0' as usize] = 1;
    t[b'1' as usize] = 1;
    t[b'2' as usize] = 1;
    t[b'3' as usize] = 1;
    t[b'4' as usize] = 1;
    t[b'5' as usize] = 1;
    t[b'6' as usize] = 1;
    t[b'7' as usize] = 1;
    t[b'8' as usize] = 1;
    t[b'9' as usize] = 1;
    t[b'a' as usize] = 1;
    t[b'b' as usize] = 1;
    t[b'c' as usize] = 1;
    t[b'd' as usize] = 1;
    t[b'e' as usize] = 1;
    t[b'f' as usize] = 1;
    t[b'A' as usize] = 1;
    t[b'B' as usize] = 1;
    t[b'C' as usize] = 1;
    t[b'D' as usize] = 1;
    t[b'E' as usize] = 1;
    t[b'F' as usize] = 1;
    t
};

const STACK_OBJECT: u8 = 1;
const STACK_ARRAY: u8 = 2;

/// Fully iterative validation state machine. No recursion.
fn validate_inner<'a>(st: &mut Vec<u8>, s: &'a [u8]) -> (&'a [u8], Option<Error>) {
    let src_len = s.len();
    let mut s = s;

    macro_rules! err {
        ($code:expr, $s:expr) => {
            return ($s, Some(Error::new($code, error_index(src_len, $s.len()))))
        };
    }

    macro_rules! skip_ws {
        ($s:expr) => {{
            if $s.is_empty() {
                err!(ErrorCode::UnexpectedEOF, $s);
            }
            if $s[0] <= b' ' {
                match $s[0] {
                    b' ' | b'\t' | b'\r' | b'\n' => {
                        let (ns, ctrl) = strfind::end_of_whitespace_seq($s);
                        $s = ns;
                        if ctrl {
                            err!(ErrorCode::IllegalControlChar, $s);
                        }
                    }
                    _ => {}
                }
                if $s.is_empty() {
                    err!(ErrorCode::UnexpectedEOF, $s);
                }
            }
        }};
    }

    // Continuation types encoded in a Vec<u8>: 1 = ObjectMember, 2 = ArrayElement
    const CONT_OBJ: u8 = 1;
    const CONT_ARR: u8 = 2;
    let mut cont: Vec<u8> = Vec::new();

    'outer: loop {
        skip_ws!(s);
        match s[0] {
            b'{' => {
                s = &s[1..];
                skip_ws!(s);
                if s[0] == b'}' {
                    s = &s[1..];
                } else {
                    st.push(STACK_OBJECT);
                    // Parse first key
                    if s[0] != b'"' {
                        if s[0] < 0x20 {
                            err!(ErrorCode::IllegalControlChar, s);
                        }
                        err!(ErrorCode::UnexpectedToken, s);
                    }
                    s = &s[1..];
                    s = match scan_string_body(s, src_len) {
                        Ok(rest) => rest,
                        Err(e) => return (s, Some(e)),
                    };
                    skip_ws!(s);
                    if s[0] != b':' {
                        if s[0] < 0x20 {
                            err!(ErrorCode::IllegalControlChar, s);
                        }
                        err!(ErrorCode::UnexpectedToken, s);
                    }
                    s = &s[1..];
                    cont.push(CONT_OBJ);
                    continue 'outer;
                }
            }
            b'[' => {
                s = &s[1..];
                skip_ws!(s);
                if s[0] == b']' {
                    s = &s[1..];
                } else {
                    st.push(STACK_ARRAY);
                    cont.push(CONT_ARR);
                    continue 'outer;
                }
            }
            b'"' => {
                s = &s[1..];
                s = match scan_string_body(s, src_len) {
                    Ok(rest) => rest,
                    Err(e) => return (s, Some(e)),
                };
            }
            b'-' | b'0'..=b'9' => {
                let rollback = s;
                let (rest, rc) = jsonnum::read_number(s);
                if rc == NumberResult::Error {
                    err!(ErrorCode::MalformedNumber, rollback);
                }
                s = rest;
            }
            b'n' => {
                if s.len() < 4 || &s[..4] != b"null" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                s = &s[4..];
            }
            b'f' => {
                if s.len() < 5 || &s[..5] != b"false" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                s = &s[5..];
            }
            b't' => {
                if s.len() < 4 || &s[..4] != b"true" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                s = &s[4..];
            }
            b if b < 0x20 => {
                err!(ErrorCode::IllegalControlChar, s);
            }
            _ => {
                err!(ErrorCode::UnexpectedToken, s);
            }
        }

        // AFTER_VALUE: check continuation stack
        loop {
            match cont.pop() {
                None => return (s, None),
                Some(CONT_OBJ) => {
                    skip_ws!(s);
                    match s[0] {
                        b',' => {
                            s = &s[1..];
                            skip_ws!(s);
                            if s[0] != b'"' {
                                if s[0] < 0x20 {
                                    err!(ErrorCode::IllegalControlChar, s);
                                }
                                err!(ErrorCode::UnexpectedToken, s);
                            }
                            s = &s[1..];
                            s = match scan_string_body(s, src_len) {
                                Ok(rest) => rest,
                                Err(e) => return (s, Some(e)),
                            };
                            skip_ws!(s);
                            if s[0] != b':' {
                                if s[0] < 0x20 {
                                    err!(ErrorCode::IllegalControlChar, s);
                                }
                                err!(ErrorCode::UnexpectedToken, s);
                            }
                            s = &s[1..];
                            cont.push(CONT_OBJ);
                            continue 'outer;
                        }
                        b'}' => {
                            s = &s[1..];
                            st.pop();
                            continue; // check next continuation
                        }
                        _ => {
                            if s[0] < 0x20 {
                                err!(ErrorCode::IllegalControlChar, s);
                            }
                            err!(ErrorCode::UnexpectedToken, s);
                        }
                    }
                }
                Some(CONT_ARR) => {
                    skip_ws!(s);
                    match s[0] {
                        b',' => {
                            s = &s[1..];
                            cont.push(CONT_ARR);
                            continue 'outer;
                        }
                        b']' => {
                            s = &s[1..];
                            st.pop();
                            continue;
                        }
                        _ => {
                            if s[0] < 0x20 {
                                err!(ErrorCode::IllegalControlChar, s);
                            }
                            err!(ErrorCode::UnexpectedToken, s);
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}

/// Scan through a JSON string body (after the opening `"`).
/// Uses 16-way unrolled LUT checks for the fast path (matching Go's validate.go).
fn scan_string_body(s: &[u8], src_len: usize) -> Result<&[u8], Error> {
    let mut s = s;
    loop {
        // 16-way unrolled fast path: skip regular string characters
        'unroll: while s.len() > 15 {
            macro_rules! check {
                ($i:expr) => {
                    if LUT_STR[s[$i] as usize] != 0 {
                        s = &s[$i..];
                        break 'unroll;
                    }
                };
            }
            check!(0);
            check!(1);
            check!(2);
            check!(3);
            check!(4);
            check!(5);
            check!(6);
            check!(7);
            check!(8);
            check!(9);
            check!(10);
            check!(11);
            check!(12);
            check!(13);
            check!(14);
            check!(15);
            s = &s[16..];
            continue;
        }

        // Handle special character or end of chunk
        if s.is_empty() {
            return Err(Error::new(ErrorCode::UnexpectedEOF, src_len));
        }
        match s[0] {
            b'"' => return Ok(&s[1..]),
            b'\\' => {
                if s.len() < 2 {
                    return Err(Error::new(ErrorCode::UnexpectedEOF, src_len));
                }
                if LUT_ESCAPE[s[1] as usize] == 1 {
                    s = &s[2..];
                    continue;
                }
                if s[1] != b'u' {
                    return Err(Error::new(ErrorCode::InvalidEscape, src_len - s.len()));
                }
                if s.len() < 6
                    || LUT_HEX[s[2] as usize] == 0
                    || LUT_HEX[s[3] as usize] == 0
                    || LUT_HEX[s[4] as usize] == 0
                    || LUT_HEX[s[5] as usize] == 0
                {
                    return Err(Error::new(ErrorCode::InvalidEscape, src_len - s.len()));
                }
                s = &s[6..];
            }
            _ if s[0] < 0x20 => {
                return Err(Error::new(ErrorCode::IllegalControlChar, src_len - s.len()));
            }
            _ => {
                s = &s[1..];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_values() {
        for input in &[
            r#"null"#,
            r#"true"#,
            r#"false"#,
            r#"0"#,
            r#"-123"#,
            r#"1.5e10"#,
            r#""""#,
            r#""hello""#,
            r#""esc\n\t\\""#,
            r#""unicode\u00Af""#,
            r#"{}"#,
            r#"[]"#,
            r#"{"a":1}"#,
            r#"[1,2,3]"#,
            r#"{"a":{"b":[1,true,null,"x"]}}"#,
            r#"  { "key" : "value" }  "#,
        ] {
            assert!(valid(input.as_bytes()), "should be valid: {}", input);
        }
    }

    #[test]
    fn invalid_values() {
        for input in &[
            r#""#,
            r#"{"#,
            r#"[1,]"#,
            r#"{"a"}"#,
            r#"nul"#,
            r#"tru"#,
            r#"fals"#,
            r#"1."#,
            r#""\x""#,
            r#"{"a":1} {"b":2}"#,
        ] {
            assert!(!valid(input.as_bytes()), "should be invalid: {}", input);
        }
    }

    #[test]
    fn validate_one_multiple() {
        let s = br#"123"hello"null"#;
        let (rest, err) = validate_one(s);
        assert!(err.is_none());
        assert_eq!(rest, br#""hello"null"#);

        let (rest, err) = validate_one(rest);
        assert!(err.is_none());
        assert_eq!(rest, b"null");

        let (rest, err) = validate_one(rest);
        assert!(err.is_none());
        assert!(rest.is_empty());
    }

    #[test]
    fn deeply_nested() {
        // 100k deep nesting must not stack overflow
        let depth = 100_000;
        let mut input = Vec::with_capacity(depth * 2);
        for _ in 0..depth {
            input.push(b'[');
        }
        // This is invalid (no closing brackets), but must not crash
        assert!(!valid(&input));
    }
}
