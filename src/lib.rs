//! High-performance zero-allocation JSON iterator and validator.
//!
//! A faithful Rust port of [romshark/jscan/v2](https://github.com/romshark/jscan).
//!
//! # Example
//!
//! ```
//! use jscan::{scan, scan_str, ValueType};
//!
//! // Works with &[u8]
//! let json = br#"{"name":"Alice","age":30}"#;
//! let err = scan(json, |iter| {
//!     println!("{}: {:?}", iter.pointer(), iter.value_type());
//!     false // continue scanning
//! });
//! assert!(err.is_none());
//!
//! // Also works with &str via scan_str
//! let json = r#"{"name":"Alice","age":30}"#;
//! let err = scan_str(json, |iter| {
//!     if iter.value_type() == ValueType::Number {
//!         println!("{} = {}", iter.pointer(), iter.value_str());
//!     }
//!     false
//! });
//! assert!(err.is_none());
//! ```

#![no_std]

extern crate alloc;

mod error;
mod jsonnum;
mod keyescape;
mod scanner;
mod strfind;
mod validator;

pub use error::{Error, ErrorCode};
pub use scanner::{scan, scan_one, Iterator, Parser};
pub use validator::{valid, validate, validate_one, Validator};

// Convenience re-exports for &str usage
/// Like [`scan`], but accepts `&str` directly.
pub fn scan_str<'a>(s: &'a str, f: impl FnMut(&Iterator<'a>) -> bool) -> Option<Error> {
    scan(s.as_bytes(), f)
}

/// Like [`valid`], but accepts `&str` directly.
pub fn valid_str(s: &str) -> bool {
    valid(s.as_bytes())
}

/// Like [`validate`], but accepts `&str` directly.
pub fn validate_str(s: &str) -> Option<Error> {
    validate(s.as_bytes())
}

/// JSON value type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum ValueType {
    Object = 1,
    Array = 2,
    Null = 3,
    False = 4,
    True = 5,
    String = 6,
    Number = 7,
}

impl core::fmt::Display for ValueType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ValueType::Object => "object",
            ValueType::Array => "array",
            ValueType::Null => "null",
            ValueType::False => "false",
            ValueType::True => "true",
            ValueType::String => "string",
            ValueType::Number => "number",
        })
    }
}
