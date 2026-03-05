use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::error::{error_index, Error, ErrorCode};
use crate::jsonnum::{self, NumberResult};
use crate::keyescape;
use crate::strfind;
use crate::ValueType;

const DEFAULT_STACK_SIZE: usize = 64;

#[derive(Clone, Copy)]
#[repr(u8)]
enum StackNodeType {
    Object = 1,
    Array = 2,
}

struct StackNode {
    node_type: StackNodeType,
    key_index: isize,
    key_index_end: isize,
    arr_len: usize,
}

/// Provides access to the current JSON value during scanning.
pub struct Iterator<'a> {
    stack: Vec<StackNode>,
    src: &'a [u8],
    pointer_buf: Vec<u8>,

    value_type: ValueType,
    value_index: usize,
    value_index_end: isize, // -1 for objects/arrays during traversal
    key_index: isize,
    key_index_end: isize,
    array_index: isize,
}

impl<'a> Iterator<'a> {
    /// Depth level of the current value (0 = root).
    #[inline]
    pub fn level(&self) -> usize {
        self.stack.len()
    }

    /// Array index of current element, or -1 if not inside an array.
    #[inline]
    pub fn array_index(&self) -> isize {
        self.array_index
    }

    /// The type of the current value.
    #[inline]
    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// Start index of the current value in the source.
    #[inline]
    pub fn value_index(&self) -> usize {
        self.value_index
    }

    /// End index of the current value in the source.
    /// Returns -1 for objects and arrays (end unknown during traversal).
    #[inline]
    pub fn value_index_end(&self) -> isize {
        self.value_index_end
    }

    /// Start index of the object member key in the source, or -1 if not a member.
    #[inline]
    pub fn key_index(&self) -> isize {
        self.key_index
    }

    /// End index of the object member key in the source, or -1 if not a member.
    #[inline]
    pub fn key_index_end(&self) -> isize {
        self.key_index_end
    }

    /// The object member key (including quotes), or empty slice if not a member.
    #[inline]
    pub fn key(&self) -> &'a [u8] {
        if self.key_index < 0 {
            return &[];
        }
        &self.src[self.key_index as usize..self.key_index_end as usize]
    }

    /// The raw value slice, or empty for objects/arrays.
    #[inline]
    pub fn value(&self) -> &'a [u8] {
        if self.value_index_end < 0 {
            return &[];
        }
        &self.src[self.value_index..self.value_index_end as usize]
    }

    /// The raw value as `&str`. Panics if the source is not valid UTF-8.
    #[inline]
    pub fn value_str(&self) -> &'a str {
        core::str::from_utf8(self.value()).expect("value is not valid UTF-8")
    }

    /// The object member key as `&str` (including quotes), or `""` if not a member.
    /// Panics if the source is not valid UTF-8.
    #[inline]
    pub fn key_str(&self) -> &'a str {
        core::str::from_utf8(self.key()).expect("key is not valid UTF-8")
    }

    /// JSON Pointer (RFC 6901) for the current value.
    pub fn pointer(&self) -> String {
        let mut buf = Vec::new();
        self.write_pointer(&mut buf);
        // SAFETY: JSON pointer content is always valid UTF-8 — composed of '/',
        // ASCII digits (array indices), and JSON string key bytes (which are valid
        // UTF-8 by the JSON spec, with only ~0/~1 escaping applied).
        unsafe { String::from_utf8_unchecked(buf) }
    }

    /// Write the JSON Pointer into the provided buffer.
    pub fn write_pointer(&self, buf: &mut Vec<u8>) {
        for node in &self.stack {
            if node.key_index >= 0 {
                buf.push(b'/');
                let key =
                    &self.src[(node.key_index + 1) as usize..(node.key_index_end - 1) as usize];
                keyescape::append_escaped(buf, key);
            }
            if node.node_type as u8 == StackNodeType::Array as u8 {
                buf.push(b'/');
                let idx = if node.arr_len > 0 {
                    node.arr_len - 1
                } else {
                    0
                };
                buf.extend_from_slice(idx.to_string().as_bytes());
            }
        }
        if self.key_index >= 0 {
            buf.push(b'/');
            let key = &self.src[(self.key_index + 1) as usize..(self.key_index_end - 1) as usize];
            keyescape::append_escaped(buf, key);
        }
    }
}

/// Reusable parser. More efficient than the free functions for multiple inputs.
pub struct Parser {
    stack: Vec<StackNode>,
    pointer_buf: Vec<u8>,
}

impl Parser {
    /// Create a new parser with the given preallocated stack depth.
    pub fn new(prealloc_stack: usize) -> Self {
        Self {
            stack: Vec::with_capacity(prealloc_stack),
            pointer_buf: Vec::new(),
        }
    }

    /// Scan one JSON value, calling `f` for each encountered value.
    /// Returns `(remaining, Option<Error>)`.
    pub fn scan_one<'a>(
        &mut self,
        s: &'a [u8],
        f: impl FnMut(&Iterator<'a>) -> bool,
    ) -> (&'a [u8], Option<Error>) {
        // Take ownership of buffers to build Iterator<'a> without lifetime transmutation
        let mut stack = core::mem::take(&mut self.stack);
        let mut pointer_buf = core::mem::take(&mut self.pointer_buf);
        stack.clear();
        pointer_buf.clear();
        let mut iter = Iterator {
            stack,
            src: s,
            pointer_buf,
            value_type: ValueType::Null,
            value_index: 0,
            value_index_end: -1,
            key_index: -1,
            key_index_end: -1,
            array_index: 0,
        };
        let result = scan_inner(&mut iter, f);
        // Return buffers to parser for reuse
        self.stack = iter.stack;
        self.pointer_buf = iter.pointer_buf;
        result
    }

    /// Scan `s` as exactly one complete JSON value, calling `f` for each encountered value.
    pub fn scan<'a>(&mut self, s: &'a [u8], f: impl FnMut(&Iterator<'a>) -> bool) -> Option<Error> {
        let (trailing, err) = self.scan_one(s, f);
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

impl Default for Parser {
    fn default() -> Self {
        Self::new(DEFAULT_STACK_SIZE)
    }
}

/// Scan `s` as one complete JSON value, calling `f` for each encountered value.
/// Returns `None` on success, or `Some(Error)` on failure.
pub fn scan<'a>(s: &'a [u8], f: impl FnMut(&Iterator<'a>) -> bool) -> Option<Error> {
    Parser::default().scan(s, f)
}

/// Scan one JSON value from the start of `s`, calling `f` for each encountered value.
/// Returns `(remaining, Option<Error>)`.
pub fn scan_one<'a>(
    s: &'a [u8],
    f: impl FnMut(&Iterator<'a>) -> bool,
) -> (&'a [u8], Option<Error>) {
    Parser::default().scan_one(s, f)
}

// Lookup tables (same as validator)
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

/// Scan a JSON string body (after opening `"`), return remaining after closing `"`.
/// Uses 16-way unrolled LUT checks for the fast path (matching Go's scan.go).
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

/// The core scan state machine. Mirrors the Go `scan` function closely.
fn scan_inner<'a>(
    i: &mut Iterator<'a>,
    mut f: impl FnMut(&Iterator<'a>) -> bool,
) -> (&'a [u8], Option<Error>) {
    let src_len = i.src.len();
    let s: &'a [u8] = i.src;

    match parse_one_value(i, s, src_len, &mut f) {
        Ok(rest) => (rest, None),
        Err((rest, e)) => (rest, Some(e)),
    }
}

/// Parse exactly one JSON value (dispatching on first byte), invoke callbacks,
/// and handle nested containers iteratively.
fn parse_one_value<'a>(
    i: &mut Iterator<'a>,
    s: &'a [u8],
    src_len: usize,
    f: &mut impl FnMut(&Iterator<'a>) -> bool,
) -> Result<&'a [u8], (&'a [u8], Error)> {
    let mut s = s;

    macro_rules! err {
        ($code:expr, $s:expr) => {
            return Err(($s, Error::new($code, error_index(src_len, $s.len()))))
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

    macro_rules! invoke_callback {
        () => {{
            i.array_index = -1;
            if let Some(top) = i.stack.last_mut() {
                if top.node_type as u8 == StackNodeType::Array as u8 {
                    i.array_index = top.arr_len as isize;
                    top.arr_len += 1;
                }
            }
            if f(&*i) {
                let idx = i.value_index;
                return Err((s, Error::new(ErrorCode::Callback, idx)));
            }
            i.key_index = -1;
        }};
    }

    // We use a loop + explicit stack to handle arbitrarily nested containers
    // without Rust stack recursion.
    //
    // `pending` tracks what we need to do after parsing a value.
    // When we encounter an object/array, we push onto `pending` and loop
    // to parse the contained values.

    enum Cont {
        // After parsing a value inside an object, check for , or }
        ObjectMember,
        // After parsing a value inside an array, check for , or ]
        ArrayElement,
    }

    let mut continuations: Vec<Cont> = Vec::new();

    'outer: loop {
        skip_ws!(s);
        match s[0] {
            b'{' => {
                i.value_type = ValueType::Object;
                i.value_index = src_len - s.len();
                i.value_index_end = -1;
                s = &s[1..];
                skip_ws!(s);
                let ks = i.key_index;
                let ke = i.key_index_end;

                invoke_callback!();

                if s[0] == b'}' {
                    s = &s[1..];
                    // empty object, fall through to after_value
                } else {
                    i.stack.push(StackNode {
                        node_type: StackNodeType::Object,
                        key_index: ks,
                        key_index_end: ke,
                        arr_len: 0,
                    });
                    // Parse object key
                    skip_ws!(s);
                    if s[0] != b'"' {
                        if s[0] < 0x20 {
                            err!(ErrorCode::IllegalControlChar, s);
                        }
                        err!(ErrorCode::UnexpectedToken, s);
                    }
                    s = &s[1..];
                    i.value_index = src_len - s.len() - 1;
                    s = scan_string_body(s, src_len).map_err(|e| (s, e))?;
                    i.key_index = i.value_index as isize;
                    i.key_index_end = (src_len - s.len()) as isize;
                    // Expect ':'
                    skip_ws!(s);
                    if s[0] != b':' {
                        if s[0] < 0x20 {
                            err!(ErrorCode::IllegalControlChar, s);
                        }
                        err!(ErrorCode::UnexpectedToken, s);
                    }
                    s = &s[1..];
                    continuations.push(Cont::ObjectMember);
                    continue 'outer; // parse the member value
                }
            }
            b'[' => {
                i.value_type = ValueType::Array;
                i.value_index = src_len - s.len();
                i.value_index_end = -1;
                s = &s[1..];
                let ks = i.key_index;
                let ke = i.key_index_end;

                invoke_callback!();

                i.stack.push(StackNode {
                    node_type: StackNodeType::Array,
                    key_index: ks,
                    key_index_end: ke,
                    arr_len: 0,
                });

                skip_ws!(s);
                if s[0] == b']' {
                    s = &s[1..];
                    i.stack.pop();
                    // fall through to after_value
                } else {
                    continuations.push(Cont::ArrayElement);
                    continue 'outer; // parse first element
                }
            }
            b'"' => {
                s = &s[1..];
                i.value_index = src_len - s.len() - 1;
                s = scan_string_body(s, src_len).map_err(|e| (s, e))?;
                i.value_index_end = (src_len - s.len()) as isize;
                i.value_type = ValueType::String;
                invoke_callback!();
            }
            b'-' | b'0'..=b'9' => {
                i.value_index = src_len - s.len();
                let rollback = s;
                let (rest, rc) = jsonnum::read_number(s);
                if rc == NumberResult::Error {
                    err!(ErrorCode::MalformedNumber, rollback);
                }
                s = rest;
                i.value_index_end = (src_len - s.len()) as isize;
                i.value_type = ValueType::Number;
                invoke_callback!();
            }
            b'n' => {
                if s.len() < 4 || &s[..4] != b"null" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                i.value_type = ValueType::Null;
                i.value_index = src_len - s.len();
                i.value_index_end = (i.value_index + 4) as isize;
                s = &s[4..];
                invoke_callback!();
            }
            b'f' => {
                if s.len() < 5 || &s[..5] != b"false" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                i.value_type = ValueType::False;
                i.value_index = src_len - s.len();
                i.value_index_end = (i.value_index + 5) as isize;
                s = &s[5..];
                invoke_callback!();
            }
            b't' => {
                if s.len() < 4 || &s[..4] != b"true" {
                    err!(ErrorCode::UnexpectedToken, s);
                }
                i.value_type = ValueType::True;
                i.value_index = src_len - s.len();
                i.value_index_end = (i.value_index + 4) as isize;
                s = &s[4..];
                invoke_callback!();
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
            match continuations.pop() {
                None => {
                    // Done - top level value parsed
                    return Ok(s);
                }
                Some(Cont::ObjectMember) => {
                    skip_ws!(s);
                    match s[0] {
                        b',' => {
                            s = &s[1..];
                            // Parse next key
                            skip_ws!(s);
                            if s[0] != b'"' {
                                if s[0] < 0x20 {
                                    err!(ErrorCode::IllegalControlChar, s);
                                }
                                err!(ErrorCode::UnexpectedToken, s);
                            }
                            s = &s[1..];
                            i.value_index = src_len - s.len() - 1;
                            s = scan_string_body(s, src_len).map_err(|e| (s, e))?;
                            i.key_index = i.value_index as isize;
                            i.key_index_end = (src_len - s.len()) as isize;
                            skip_ws!(s);
                            if s[0] != b':' {
                                if s[0] < 0x20 {
                                    err!(ErrorCode::IllegalControlChar, s);
                                }
                                err!(ErrorCode::UnexpectedToken, s);
                            }
                            s = &s[1..];
                            continuations.push(Cont::ObjectMember);
                            continue 'outer; // parse next member value
                        }
                        b'}' => {
                            s = &s[1..];
                            i.stack.pop();
                            i.key_index = -1;
                            i.key_index_end = -1;
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
                Some(Cont::ArrayElement) => {
                    skip_ws!(s);
                    match s[0] {
                        b',' => {
                            s = &s[1..];
                            continuations.push(Cont::ArrayElement);
                            continue 'outer; // parse next element
                        }
                        b']' => {
                            s = &s[1..];
                            i.stack.pop();
                            i.key_index = -1;
                            i.key_index_end = -1;
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
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueType;
    use alloc::string::ToString;
    use alloc::vec;

    #[test]
    fn scan_simple_values() {
        for (input, expected_type) in &[
            ("null", ValueType::Null),
            ("true", ValueType::True),
            ("false", ValueType::False),
            ("42", ValueType::Number),
            (r#""hello""#, ValueType::String),
        ] {
            let mut seen = Vec::new();
            let err = scan(input.as_bytes(), |iter| {
                seen.push(iter.value_type());
                false
            });
            assert!(err.is_none(), "error scanning {}: {:?}", input, err);
            assert_eq!(seen, vec![*expected_type], "for input {}", input);
        }
    }

    #[test]
    fn scan_object() {
        let input = br#"{"a":1,"b":"two"}"#;
        let mut types = Vec::new();
        let mut pointers = Vec::new();
        let err = scan(input, |iter| {
            types.push(iter.value_type());
            pointers.push(iter.pointer());
            false
        });
        assert!(err.is_none());
        assert_eq!(
            types,
            vec![ValueType::Object, ValueType::Number, ValueType::String,]
        );
        assert_eq!(pointers, vec!["", "/a", "/b"]);
    }

    #[test]
    fn scan_array() {
        let input = br#"[1,true,"x"]"#;
        let mut indices = Vec::new();
        let err = scan(input, |iter| {
            indices.push(iter.array_index());
            false
        });
        assert!(err.is_none());
        assert_eq!(indices, vec![-1, 0, 1, 2]); // array itself has -1
    }

    #[test]
    fn scan_nested() {
        let input = br#"{"o":{"a":[1,2]}}"#;
        let mut pointers = Vec::new();
        let err = scan(input, |iter| {
            pointers.push(iter.pointer());
            false
        });
        assert!(err.is_none());
        assert_eq!(pointers, vec!["", "/o", "/o/a", "/o/a/0", "/o/a/1"]);
    }

    #[test]
    fn scan_callback_break() {
        let input = br#"[1,2,3]"#;
        let mut count = 0;
        let err = scan(input, |_iter| {
            count += 1;
            count >= 2 // break after 2nd callback
        });
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, ErrorCode::Callback);
        assert_eq!(count, 2);
    }

    #[test]
    fn scan_empty_containers() {
        let input = br#"{"e":{},"a":[]}"#;
        let mut pointers = Vec::new();
        let err = scan(input, |iter| {
            pointers.push(iter.pointer());
            false
        });
        assert!(err.is_none());
        assert_eq!(pointers, vec!["", "/e", "/a"]);
    }

    #[test]
    fn scan_one_multiple() {
        let input = b"123 \"hello\"";
        let (rest, err) = scan_one(input, |_| false);
        assert!(err.is_none());
        // rest should be ' "hello"'
        let trimmed = rest
            .iter()
            .copied()
            .skip_while(|b| *b == b' ')
            .collect::<Vec<_>>();
        assert_eq!(&trimmed, b"\"hello\"");

        let (rest, err) = scan_one(&rest[rest.len() - trimmed.len()..], |_| false);
        assert!(err.is_none());
        assert!(rest.is_empty());
    }

    #[test]
    fn scan_value_slices() {
        let input = br#"{"n":-9.5e2,"s":"val"}"#;
        let mut values: Vec<(String, String)> = Vec::new();
        let err = scan(input, |iter| {
            let v = core::str::from_utf8(iter.value()).unwrap_or("").to_string();
            values.push((iter.pointer(), v));
            false
        });
        assert!(err.is_none());
        assert_eq!(values[0], ("".into(), "".into())); // object
        assert_eq!(values[1], ("/n".into(), "-9.5e2".into()));
        assert_eq!(values[2], ("/s".into(), "\"val\"".into()));
    }

    #[test]
    fn scan_error_invalid() {
        let err = scan(b"{", |_| false);
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, ErrorCode::UnexpectedEOF);
    }

    #[test]
    fn scan_error_trailing() {
        let err = scan(b"1 2", |_| false);
        assert!(err.is_some());
        assert_eq!(err.unwrap().code, ErrorCode::UnexpectedToken);
    }

    #[test]
    fn test_go_example() {
        // Reproduce the Go ExampleScan output
        let j = br#"{
		"s": "value",
		"t": true,
		"f": false,
		"0": null,
		"n": -9.123e3,
		"o0": {},
		"a0": [],
		"o": {
			"k": "\"v\"",
			"a": [
				true,
				null,
				"item",
				-67.02e9,
				["foo"]
			]
		},
		"a3": [
			0,
			{
				"a3.a3":8
			}
		]
	}"#;

        let mut results: Vec<(String, ValueType, String, isize, String, usize)> = Vec::new();
        let err = scan(j, |iter| {
            let pointer = iter.pointer();
            let vtype = iter.value_type();
            let key = core::str::from_utf8(iter.key()).unwrap_or("").to_string();
            let ai = iter.array_index();
            let val = core::str::from_utf8(iter.value()).unwrap_or("").to_string();
            let level = iter.level();
            results.push((pointer, vtype, key, ai, val, level));
            false
        });
        assert!(err.is_none(), "scan error: {:?}", err);

        // Spot check key entries
        assert_eq!(results[0].0, "");
        assert_eq!(results[0].1, ValueType::Object);
        assert_eq!(results[0].5, 0);

        // "/s" -> string "value"
        assert_eq!(results[1].0, "/s");
        assert_eq!(results[1].1, ValueType::String);
        assert_eq!(results[1].5, 1);

        // "/n" -> number
        assert_eq!(results[5].0, "/n");
        assert_eq!(results[5].1, ValueType::Number);
        assert_eq!(results[5].4, "-9.123e3");

        // "/o/a" -> array at level 2
        assert_eq!(results[10].0, "/o/a");
        assert_eq!(results[10].1, ValueType::Array);
        assert_eq!(results[10].5, 2);

        // "/o/a/0" -> true at level 3
        assert_eq!(results[11].0, "/o/a/0");
        assert_eq!(results[11].1, ValueType::True);
        assert_eq!(results[11].3, 0); // array index 0
        assert_eq!(results[11].5, 3);

        // "/a3/1/a3.a3" -> number 8
        let last = results.last().unwrap();
        assert_eq!(last.0, "/a3/1/a3.a3");
        assert_eq!(last.1, ValueType::Number);
        assert_eq!(last.4, "8");
        assert_eq!(last.5, 3);
    }
}
