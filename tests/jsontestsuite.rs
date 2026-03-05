use jscan::{scan, valid, validate, Parser, Validator};
use std::fs;
use std::path::Path;

#[test]
fn json_test_suite() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/jsontestsuite");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("testdata/jsontestsuite not found")
        .filter_map(|e| e.ok())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut y_pass = 0;
    let mut n_pass = 0;
    let mut i_count = 0;
    let mut failures = Vec::new();

    for entry in &entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let content = fs::read(entry.path()).unwrap();

        if name.starts_with("y_") {
            // Must accept
            let mut ok = true;

            if !valid(&content) {
                failures.push(format!("{}: valid() rejected", name));
                ok = false;
            }

            let mut v = Validator::new(1024);
            if v.validate(&content).is_some() {
                failures.push(format!("{}: Validator::validate() rejected", name));
                ok = false;
            }

            let scan_err = scan(&content, |_| false);
            if scan_err.is_some() {
                failures.push(format!("{}: scan() rejected: {}", name, scan_err.unwrap()));
                ok = false;
            }

            let mut p = Parser::new(1024);
            let scan_err = p.scan(&content, |_| false);
            if scan_err.is_some() {
                failures.push(format!(
                    "{}: Parser::scan() rejected: {}",
                    name,
                    scan_err.unwrap()
                ));
                ok = false;
            }

            if ok {
                y_pass += 1;
            }
        } else if name.starts_with("n_") {
            // Must reject
            let mut ok = true;

            if valid(&content) {
                failures.push(format!("{}: valid() accepted", name));
                ok = false;
            }

            let mut v = Validator::new(1024);
            if v.validate(&content).is_none() {
                failures.push(format!("{}: Validator::validate() accepted", name));
                ok = false;
            }

            let scan_err = scan(&content, |_| false);
            if scan_err.is_none() {
                failures.push(format!("{}: scan() accepted", name));
                ok = false;
            }

            let mut p = Parser::new(1024);
            let scan_err = p.scan(&content, |_| false);
            if scan_err.is_none() {
                failures.push(format!("{}: Parser::scan() accepted", name));
                ok = false;
            }

            if ok {
                n_pass += 1;
            }
        } else if name.starts_with("i_") {
            // Implementation-defined: just ensure no panic
            let _ = valid(&content);
            let _ = validate(&content);
            let _ = scan(&content, |_| false);
            i_count += 1;
        }
    }

    eprintln!(
        "JSONTestSuite: y_pass={}, n_pass={}, i_tested={}, failures={}",
        y_pass,
        n_pass,
        i_count,
        failures.len()
    );

    if !failures.is_empty() {
        for f in &failures {
            eprintln!("  FAIL: {}", f);
        }
        panic!("{} JSONTestSuite failures", failures.len());
    }
}
