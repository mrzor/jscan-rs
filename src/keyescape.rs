use alloc::vec::Vec;

/// Append `key` to `dest`, escaping `~` as `~0` and `/` as `~1`
/// per RFC 6901 (JSON Pointer).
pub(crate) fn append_escaped(dest: &mut Vec<u8>, key: &[u8]) {
    if !key.iter().any(|&b| b == b'~' || b == b'/') {
        dest.extend_from_slice(key);
        return;
    }
    for &b in key {
        match b {
            b'~' => dest.extend_from_slice(b"~0"),
            b'/' => dest.extend_from_slice(b"~1"),
            _ => dest.push(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_escape() {
        let mut buf = Vec::new();
        append_escaped(&mut buf, b"hello");
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_tilde_and_slash() {
        let mut buf = Vec::new();
        append_escaped(&mut buf, b"a/b~c");
        assert_eq!(&buf, b"a~1b~0c");
    }
}
