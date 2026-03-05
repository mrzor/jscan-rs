# jscan-rs

High-performance zero-allocation JSON iterator and validator for Rust.

A faithful port of [romshark/jscan/v2](https://github.com/romshark/jscan) (Go).

## What it does

jscan traverses JSON without deserializing it. Instead of building a DOM or mapping to structs, it calls your closure for every value it encounters — giving you the type, key, array index, raw value slice, nesting depth, and JSON Pointer path. Validation happens as a side effect.

This is useful when you need to:
- Extract a few fields from large JSON without parsing the whole thing
- Validate JSON syntax at high speed
- Stream-process JSON values with minimal allocations
- Build custom deserializers

## Usage

```rust
use jscan::{scan, ValueType};

let json = br#"{"name":"Alice","scores":[98,76,100]}"#;

scan(json, |iter| {
    match iter.value_type() {
        ValueType::Number => {
            println!("{} = {}", iter.pointer(), std::str::from_utf8(iter.value()).unwrap());
        }
        _ => {}
    }
    false // return true to stop early
});
// Output:
// /scores/0 = 98
// /scores/1 = 76
// /scores/2 = 100
```

### Validation only

```rust
use jscan::valid;

assert!(valid(br#"{"key": [1, 2, 3]}"#));
assert!(!valid(br#"{"trailing": 1,}"#));
```

### Scanning multiple concatenated values

```rust
use jscan::scan_one;

let input = br#"123"hello"null"#;
let (rest, err) = scan_one(input, |iter| { /* ... */ false });
// rest = b#""hello"null"#, err = None
```

### Reusable parser (avoids repeated allocation)

```rust
use jscan::Parser;

let mut parser = Parser::new(64);
for json_doc in &[br#""a""#.as_slice(), br#"[1,2]"#.as_slice()] {
    let err = parser.scan(json_doc, |iter| {
        // ...
        false
    });
}
```

## Iterator API

Inside the callback, the `Iterator` provides:

| Method | Returns | Description |
|---|---|---|
| `value_type()` | `ValueType` | Object, Array, String, Number, True, False, Null |
| `value()` | `&[u8]` | Raw value slice (empty for objects/arrays) |
| `key()` | `&[u8]` | Object member key including quotes (empty if not a member) |
| `pointer()` | `String` | RFC 6901 JSON Pointer path |
| `level()` | `usize` | Nesting depth (0 = root) |
| `array_index()` | `isize` | Element index in array, or -1 |
| `value_index()` | `usize` | Byte offset of value start in source |
| `value_index_end()` | `isize` | Byte offset of value end, or -1 for containers |
| `key_index()` | `isize` | Byte offset of key start, or -1 |
| `key_index_end()` | `isize` | Byte offset of key end, or -1 |
| `write_pointer(&mut buf)` | — | Write pointer to a reusable buffer (avoids alloc) |

## Status

Early stage. The core scan and validate logic is complete and tested (21 tests pass including a full reproduction of the Go library's example output). See [NEXT-STEPS.md](NEXT-STEPS.md) for the roadmap.

## License

MIT — same as the original Go library.
