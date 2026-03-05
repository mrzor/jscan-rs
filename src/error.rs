use core::fmt;

/// Error code identifying the type of JSON error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum ErrorCode {
    /// Invalid escape sequence in a string.
    InvalidEscape = 1,
    /// Illegal control character (< 0x20) encountered.
    IllegalControlChar = 2,
    /// Unexpected end of input.
    UnexpectedEOF = 3,
    /// Unexpected token.
    UnexpectedToken = 4,
    /// Malformed number literal.
    MalformedNumber = 5,
    /// Callback returned `true` to stop scanning.
    Callback = 6,
}

/// A JSON syntax error with location information.
#[derive(Debug, Clone)]
pub struct Error {
    /// The error code.
    pub code: ErrorCode,
    /// Index in the source where the error occurred.
    pub index: usize,
}

impl Error {
    pub(crate) fn new(code: ErrorCode, index: usize) -> Self {
        Self { code, index }
    }

    /// Returns `true` if this represents an actual error.
    #[inline]
    pub fn is_err(&self) -> bool {
        true
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            ErrorCode::UnexpectedEOF => {
                write!(f, "error at index {}: unexpected EOF", self.index)
            }
            _ => {
                let msg = match self.code {
                    ErrorCode::InvalidEscape => "invalid escape",
                    ErrorCode::IllegalControlChar => "illegal control character",
                    ErrorCode::UnexpectedToken => "unexpected token",
                    ErrorCode::MalformedNumber => "malformed number",
                    ErrorCode::Callback => "callback error",
                    ErrorCode::UnexpectedEOF => unreachable!(),
                };
                write!(f, "error at index {}: {}", self.index, msg)
            }
        }
    }
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Compute error index from source length and remaining slice length.
#[inline]
pub(crate) fn error_index(src_len: usize, remaining_len: usize) -> usize {
    src_len - remaining_len
}
