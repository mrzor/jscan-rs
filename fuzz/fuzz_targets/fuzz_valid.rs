#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let jscan_result = jscan::valid(data);
    let jscan_str_result = std::str::from_utf8(data)
        .map(|s| jscan::valid_str(s))
        .unwrap_or(false);

    // Byte and str APIs must agree when input is valid UTF-8
    if let Ok(_) = std::str::from_utf8(data) {
        assert_eq!(
            jscan_result, jscan_str_result,
            "valid() and valid_str() disagree on {:?}",
            String::from_utf8_lossy(data),
        );
    }

    // Compare against serde_json: if serde accepts it, jscan must too.
    // jscan may accept inputs serde rejects (e.g. invalid UTF-8 in strings)
    // because jscan validates JSON syntax without requiring UTF-8 validity.
    let serde_result = serde_json::from_slice::<serde_json::Value>(data).is_ok();
    if serde_result {
        assert!(
            jscan_result,
            "serde_json accepts but jscan rejects: {:?}",
            String::from_utf8_lossy(data),
        );
    }

    // Must never panic — jscan should handle all inputs gracefully
    let _ = jscan_result;
});
