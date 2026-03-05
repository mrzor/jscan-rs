use jscan::{scan, scan_one, valid, ValueType, Parser, Validator, ErrorCode};

#[derive(Debug, Clone)]
struct Record {
    level: usize,
    value_type: ValueType,
    key: &'static str,
    value: &'static str,
    array_index: isize,
    pointer: &'static str,
}

impl Record {
    fn new(
        vt: ValueType, level: usize, key: &'static str, value: &'static str,
        ai: isize, pointer: &'static str,
    ) -> Self {
        Self { level, value_type: vt, key, value, array_index: ai, pointer }
    }
}

fn run_scan_test(name: &str, input: &[u8], expected: &[Record]) {
    // valid
    assert!(valid(input), "{}: valid() failed", name);

    // Validator
    let mut v = Validator::new(64);
    assert!(v.valid(input), "{}: Validator::valid() failed", name);

    // scan
    let mut j = 0;
    let err = scan(input, |iter| {
        if j >= expected.len() {
            panic!("{}: unexpected extra value at index {}", name, j);
        }
        let e = &expected[j];
        assert_eq!(e.value_type, iter.value_type(), "{}: ValueType at {}", name, j);
        assert_eq!(e.level, iter.level(), "{}: Level at {}", name, j);
        let val = std::str::from_utf8(iter.value()).unwrap_or("");
        assert_eq!(e.value, val, "{}: Value at {}", name, j);
        let key = std::str::from_utf8(iter.key()).unwrap_or("");
        assert_eq!(e.key, key, "{}: Key at {}", name, j);
        assert_eq!(e.array_index, iter.array_index(), "{}: ArrayIndex at {}", name, j);
        let ptr = iter.pointer();
        assert_eq!(e.pointer, ptr, "{}: Pointer at {}", name, j);
        j += 1;
        false
    });
    assert!(err.is_none(), "{}: scan error: {:?}", name, err);
    assert_eq!(j, expected.len(), "{}: expected {} records, got {}", name, expected.len(), j);

    // Parser::scan
    let mut p = Parser::new(64);
    j = 0;
    let err = p.scan(input, |iter| {
        let e = &expected[j];
        assert_eq!(e.value_type, iter.value_type(), "{}: Parser ValueType at {}", name, j);
        assert_eq!(e.level, iter.level(), "{}: Parser Level at {}", name, j);
        j += 1;
        false
    });
    assert!(err.is_none(), "{}: Parser::scan error: {:?}", name, err);
}

#[test]
fn scan_null() {
    run_scan_test("null", b"null", &[
        Record::new(ValueType::Null, 0, "", "null", -1, ""),
    ]);
}

#[test]
fn scan_bool_true() {
    run_scan_test("bool_true", b"true", &[
        Record::new(ValueType::True, 0, "", "true", -1, ""),
    ]);
}

#[test]
fn scan_bool_false() {
    run_scan_test("bool_false", b"false", &[
        Record::new(ValueType::False, 0, "", "false", -1, ""),
    ]);
}

#[test]
fn scan_number_int() {
    run_scan_test("number_int", b"42", &[
        Record::new(ValueType::Number, 0, "", "42", -1, ""),
    ]);
}

#[test]
fn scan_number_decimal() {
    run_scan_test("number_decimal", b"42.5", &[
        Record::new(ValueType::Number, 0, "", "42.5", -1, ""),
    ]);
}

#[test]
fn scan_number_negative() {
    run_scan_test("number_negative", b"-42.5", &[
        Record::new(ValueType::Number, 0, "", "-42.5", -1, ""),
    ]);
}

#[test]
fn scan_number_exponent() {
    run_scan_test("number_exponent", b"2.99792458e8", &[
        Record::new(ValueType::Number, 0, "", "2.99792458e8", -1, ""),
    ]);
}

#[test]
fn scan_string() {
    run_scan_test("string", br#""42""#, &[
        Record::new(ValueType::String, 0, "", r#""42""#, -1, ""),
    ]);
}

#[test]
fn scan_empty_array() {
    run_scan_test("empty_array", b"[]", &[
        Record::new(ValueType::Array, 0, "", "", -1, ""),
    ]);
}

#[test]
fn scan_empty_object() {
    run_scan_test("empty_object", b"{}", &[
        Record::new(ValueType::Object, 0, "", "", -1, ""),
    ]);
}

#[test]
fn scan_nested_array() {
    run_scan_test("nested_array", br#"[[null,[{"key":true}]],[]]"#, &[
        Record::new(ValueType::Array,  0, "",       "",     -1, ""),
        Record::new(ValueType::Array,  1, "",       "",      0, "/0"),
        Record::new(ValueType::Null,   2, "",       "null",  0, "/0/0"),
        Record::new(ValueType::Array,  2, "",       "",      1, "/0/1"),
        Record::new(ValueType::Object, 3, "",       "",      0, "/0/1/0"),
        Record::new(ValueType::True,   4, r#""key""#, "true", -1, "/0/1/0/key"),
        Record::new(ValueType::Array,  1, "",       "",      1, "/1"),
    ]);
}

#[test]
fn scan_escaped_pointer() {
    run_scan_test("escaped_pointer", br#"{"/":[{"~":null},0]}"#, &[
        Record::new(ValueType::Object, 0, "",       "",     -1, ""),
        Record::new(ValueType::Array,  1, r#""/""#, "",     -1, "/~1"),
        Record::new(ValueType::Object, 2, "",       "",      0, "/~1/0"),
        Record::new(ValueType::Null,   3, r#""~""#, "null", -1, "/~1/0/~0"),
        Record::new(ValueType::Number, 2, "",       "0",     1, "/~1/1"),
    ]);
}

#[test]
fn scan_nested_object() {
    let input = br#"{
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
                false,
                null,
                "item",
                -67.02e9,
                ["foo"]
            ]
        },
        "a3": [ 0, {"a3": 8} ]
    }"#;

    run_scan_test("nested_object", input, &[
        Record::new(ValueType::Object, 0, "",        "",          -1, ""),
        Record::new(ValueType::String, 1, r#""s""#,  r#""value""#, -1, "/s"),
        Record::new(ValueType::True,   1, r#""t""#,  "true",      -1, "/t"),
        Record::new(ValueType::False,  1, r#""f""#,  "false",     -1, "/f"),
        Record::new(ValueType::Null,   1, r#""0""#,  "null",      -1, "/0"),
        Record::new(ValueType::Number, 1, r#""n""#,  "-9.123e3",  -1, "/n"),
        Record::new(ValueType::Object, 1, r#""o0""#, "",          -1, "/o0"),
        Record::new(ValueType::Array,  1, r#""a0""#, "",          -1, "/a0"),
        Record::new(ValueType::Object, 1, r#""o""#,  "",          -1, "/o"),
        Record::new(ValueType::String, 2, r#""k""#,  r#""\"v\"""#, -1, "/o/k"),
        Record::new(ValueType::Array,  2, r#""a""#,  "",          -1, "/o/a"),
        Record::new(ValueType::True,   3, "",        "true",       0, "/o/a/0"),
        Record::new(ValueType::False,  3, "",        "false",      1, "/o/a/1"),
        Record::new(ValueType::Null,   3, "",        "null",       2, "/o/a/2"),
        Record::new(ValueType::String, 3, "",        r#""item""#,  3, "/o/a/3"),
        Record::new(ValueType::Number, 3, "",        "-67.02e9",   4, "/o/a/4"),
        Record::new(ValueType::Array,  3, "",        "",           5, "/o/a/5"),
        Record::new(ValueType::String, 4, "",        r#""foo""#,   0, "/o/a/5/0"),
        Record::new(ValueType::Array,  1, r#""a3""#, "",          -1, "/a3"),
        Record::new(ValueType::Number, 2, "",        "0",          0, "/a3/0"),
        Record::new(ValueType::Object, 2, "",        "",           1, "/a3/1"),
        Record::new(ValueType::Number, 3, r#""a3""#, "8",         -1, "/a3/1/a3"),
    ]);
}

#[test]
fn scan_trailing_whitespace() {
    for (name, input) in &[
        ("trailing_space", "null "),
        ("trailing_cr", "null\r"),
        ("trailing_tab", "null\t"),
        ("trailing_lf", "null\n"),
    ] {
        run_scan_test(name, input.as_bytes(), &[
            Record::new(ValueType::Null, 0, "", "null", -1, ""),
        ]);
    }
}

// === Error Tests ===

fn assert_scan_error(name: &str, input: &[u8], expected_code: ErrorCode, expected_index: usize) {
    assert!(!valid(input), "{}: valid() should reject", name);

    let mut v = Validator::new(64);
    assert!(!v.valid(input), "{}: Validator::valid() should reject", name);

    let err = scan(input, |_| false);
    assert!(err.is_some(), "{}: scan should error", name);
    let e = err.unwrap();
    assert_eq!(e.code, expected_code, "{}: wrong error code", name);
    assert_eq!(e.index, expected_index, "{}: wrong error index (got {}, want {})", name, e.index, expected_index);

    let mut p = Parser::new(64);
    let err = p.scan(input, |_| false);
    assert!(err.is_some(), "{}: Parser::scan should error", name);
    let e = err.unwrap();
    assert_eq!(e.code, expected_code, "{}: Parser wrong error code", name);
    assert_eq!(e.index, expected_index, "{}: Parser wrong error index", name);
}

#[test]
fn error_unexpected_eof() {
    let cases: &[(&str, &[u8], usize)] = &[
        ("empty", b"", 0),
        ("space", b" ", 1),
        ("trn_space", b"\t\r\n ", 4),
        ("open_curly", b"{", 1),
        ("open_curly_space", b"{ ", 2),
        ("after_key", b"{\"x\"", 4),
        ("after_key_space", b"{\"x\" ", 5),
        ("after_field_value", b"{\"x\":null", 9),
        ("after_field_value_space", b"{\"x\":null ", 10),
        ("after_field_comma", b"{\"x\":null,", 10),
        ("after_field_comma_space", b"{\"x\":null, ", 11),
        ("open_square", b"[", 1),
        ("open_square_space", b"[ ", 2),
        ("before_comma_array", b"[null", 5),
        ("before_comma_array_space", b"[null ", 6),
        ("after_array_comma", b"[null,", 6),
        ("after_array_comma_space", b"[null, ", 7),
        ("nested_open", b"[[null", 6),
        ("unclosed_string", b"\"string", 7),
        ("unclosed_escaped_string", b"\"string\\\"", 9),
        ("unclosed_double_escaped", b"\"string\\\\\\\"", 11),
        ("revsolidus_in_key", b"{\"key\\", 6),
        ("unclosed_key", b"{\"key", 5),
    ];

    for &(name, input, idx) in cases {
        assert_scan_error(name, input, ErrorCode::UnexpectedEOF, idx);
    }
}

#[test]
fn error_invalid_escape() {
    let cases: &[(&str, &[u8], usize)] = &[
        ("in_string", b"\"\\0\"", 1),
        ("unicode_in_string", b"\"\\u000m\"", 1),
        ("in_fieldname", b"{\"\\0\":true}", 2),
        ("unicode_in_fieldname", b"{\"\\u000m\":true}", 2),
    ];

    for &(name, input, idx) in cases {
        assert_scan_error(name, input, ErrorCode::InvalidEscape, idx);
    }
}

#[test]
fn error_unexpected_token() {
    let cases: &[(&str, &[u8], usize)] = &[
        ("invalid_null", b"nul", 0),
        ("invalid_false", b"fals", 0),
        ("invalid_true", b"tru", 0),
        ("invalid_number", b"e1", 0),
        ("key_closing_curly", b"{\"key\"}", 6),
        ("key_number", b"{\"key\"1 :}", 6),
        ("key_semicolon", b"{\"key\";1}", 6),
        ("colon_closing_curly", b"{\"okay\":}", 8),
        ("comma_empty_obj", b"{\"key\":12,{}}", 10),
        ("field_square", b"{\"f\":\"\"]", 7),
        ("elem_curly", b"[null}", 5),
        ("elem_trailing_comma", b"[\"okay\",]", 8),
        ("elem_square", b"[\"okay\"[", 7),
        ("elem_minus", b"[\"okay\"-12", 7),
        ("elem_zero", b"[\"okay\"0", 7),
        ("elem_string", b"[\"okay\"\"not okay\"]", 7),
        ("field_string", b"{\"foo\":\"bar\" \"baz\":\"fuz\"}", 13),
        ("elem_false", b"[null false]", 6),
        ("elem_true", b"[null true]", 6),
        ("leading_zero", b"01", 1),
        ("neg_leading_zero", b"-00", 2),
        ("after_str_comma", b"\"okay\",null", 6),
        ("str_space_str", b"\"str\" \"str\"", 6),
        ("zero_space_zero", b"0 0", 2),
        ("false_space_false", b"false false", 6),
        ("true_space_true", b"true true", 5),
        ("null_space_null", b"null null", 5),
        ("arr_space_arr", b"[] []", 3),
        ("obj_space_obj", b"{\"k\":0} {\"k\":0}", 8),
    ];

    for &(name, input, idx) in cases {
        assert_scan_error(name, input, ErrorCode::UnexpectedToken, idx);
    }
}

#[test]
fn error_malformed_number() {
    let cases: &[(&str, &[u8], usize)] = &[
        ("negative_only", b"-", 0),
        ("trailing_dot", b"0.", 0),
        ("trailing_e", b"0e", 0),
        ("trailing_e_minus", b"1e-", 0),
    ];

    for &(name, input, idx) in cases {
        assert_scan_error(name, input, ErrorCode::MalformedNumber, idx);
    }
}

#[test]
fn error_control_characters_in_string() {
    // Control chars at various offsets inside a string value
    for offset in 0..=24 {
        let mut input = Vec::new();
        input.push(b'"');
        for _ in 0..offset {
            input.push(b'x');
        }
        input.push(0x00); // control char
        input.extend_from_slice(b"1234567812345678\"");

        assert!(!valid(&input), "ctrl char at offset {} should be invalid", offset);
        let err = scan(&input, |_| false);
        assert!(err.is_some(), "ctrl char at offset {} should error", offset);
        let e = err.unwrap();
        assert_eq!(e.code, ErrorCode::IllegalControlChar,
            "ctrl char at offset {}: wrong code", offset);
        assert_eq!(e.index, offset + 1,
            "ctrl char at offset {}: wrong index (got {}, want {})", offset, e.index, offset + 1);
    }
}

#[test]
fn error_control_characters_in_fieldname() {
    for offset in 0..=24 {
        let mut input = Vec::new();
        input.extend_from_slice(b"{\"");
        for _ in 0..offset {
            input.push(b'x');
        }
        input.push(0x00);
        input.extend_from_slice(b"\":\"1234567812345678\"}");

        assert!(!valid(&input), "fieldname ctrl char at offset {} should be invalid", offset);
        let err = scan(&input, |_| false);
        assert!(err.is_some());
        let e = err.unwrap();
        assert_eq!(e.code, ErrorCode::IllegalControlChar);
        assert_eq!(e.index, offset + 2);
    }
}

#[test]
fn error_control_chars_various_positions() {
    // Test control chars (excluding \t \r \n) at various structural positions
    let ctrl_chars: Vec<u8> = (0..0x20u8)
        .filter(|&b| b != b'\t' && b != b'\r' && b != b'\n')
        .collect();

    for &c in &ctrl_chars {
        // Before key after space
        let input = [b'{', b' ', c, b':', b'n', b'u', b'l', b'l', b'}'];
        assert!(!valid(&input), "ctrl 0x{:02x} before key", c);

        // After value
        let input = [b'[', b'n', b'u', b'l', b'l', c, b',', b'n', b'u', b'l', b'l', b']'];
        assert!(!valid(&input), "ctrl 0x{:02x} after value", c);
    }
}

#[test]
fn scan_one_basic() {
    let input = b"123 \"hello\"";
    let (rest, err) = scan_one(input, |_| false);
    assert!(err.is_none());
    assert_eq!(rest, b" \"hello\"");
}
