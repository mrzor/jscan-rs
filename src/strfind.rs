/// Skip whitespace (space, tab, CR, LF) returning the remaining slice.
/// If an illegal control char (< 0x20 but not whitespace) is at the new position,
/// returns `(remaining, true)`.
#[inline]
pub(crate) fn end_of_whitespace_seq(s: &[u8]) -> (&[u8], bool) {
    let mut s = s;

    // Unrolled 16-byte scan
    while s.len() > 15 {
        macro_rules! check {
            ($i:expr) => {
                if !is_whitespace(s[$i]) {
                    s = &s[$i..];
                    return (s, s[0] < 0x20);
                }
            };
        }
        check!(0); check!(1); check!(2); check!(3);
        check!(4); check!(5); check!(6); check!(7);
        check!(8); check!(9); check!(10); check!(11);
        check!(12); check!(13); check!(14); check!(15);
        s = &s[16..];
    }

    // Tail
    while !s.is_empty() {
        if !is_whitespace(s[0]) {
            return (s, s[0] < 0x20);
        }
        s = &s[1..];
    }
    (s, false)
}

#[inline(always)]
fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}
