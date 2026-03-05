use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use flate2::read::GzDecoder;
use std::fs;
use std::io::Read;
use std::path::Path;

use jscan::{scan, valid, Parser, Validator, ValueType};

fn load_test_data(name: &str) -> Vec<u8> {
    let path = Path::new("testdata").join(name);
    if name.ends_with(".gz") {
        let compressed =
            fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut data = Vec::new();
        decoder.read_to_end(&mut data).unwrap();
        data
    } else {
        fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e))
    }
}

struct Stats {
    total_strings: usize,
    total_nulls: usize,
    total_booleans: usize,
    total_numbers: usize,
    total_objects: usize,
    total_arrays: usize,
    total_keys: usize,
    max_key_len: usize,
    max_depth: usize,
    max_array_len: usize,
}

fn calc_stats(parser: &mut Parser, data: &[u8]) -> Stats {
    let mut s = Stats {
        total_strings: 0,
        total_nulls: 0,
        total_booleans: 0,
        total_numbers: 0,
        total_objects: 0,
        total_arrays: 0,
        total_keys: 0,
        max_key_len: 0,
        max_depth: 0,
        max_array_len: 0,
    };
    let err = parser.scan(data, |iter| {
        if iter.key_index() >= 0 {
            let key_len = (iter.key_index_end() - iter.key_index() - 2) as usize;
            s.total_keys += 1;
            if key_len > s.max_key_len {
                s.max_key_len = key_len;
            }
        }
        match iter.value_type() {
            ValueType::Object => s.total_objects += 1,
            ValueType::Array => s.total_arrays += 1,
            ValueType::Null => s.total_nulls += 1,
            ValueType::True | ValueType::False => s.total_booleans += 1,
            ValueType::Number => s.total_numbers += 1,
            ValueType::String => s.total_strings += 1,
        }
        if iter.level() > s.max_depth {
            s.max_depth = iter.level();
        }
        let arr_len = (iter.array_index() + 1) as usize;
        if arr_len > s.max_array_len {
            s.max_array_len = arr_len;
        }
        false
    });
    assert!(err.is_none(), "scan error: {:?}", err);
    s
}

const FILE_BENCHMARKS: &[&str] = &[
    "miniscule_1b.json",
    "tiny_8b.json",
    "small_336b.json",
    "large_26m.json.gz",
    "nasa_SxSW_2016_125k.json.gz",
    "escaped_3k.json",
    "array_int_1024_12k.json",
    "array_dec_1024_10k.json",
    "array_nullbool_1024_5k.json",
    "array_str_1024_639k.json",
];

fn bench_valid(c: &mut Criterion) {
    let mut group = c.benchmark_group("valid");

    // Deep array: 1024 nested brackets
    let deep_array: Vec<u8> = {
        let mut v = Vec::with_capacity(2048);
        for _ in 0..1024 {
            v.push(b'[');
        }
        for _ in 0..1024 {
            v.push(b']');
        }
        v
    };
    group.throughput(Throughput::Bytes(deep_array.len() as u64));
    group.bench_function("deeparray", |b| {
        let mut v = Validator::default();
        b.iter(|| black_box(v.valid(black_box(&deep_array))))
    });

    // Unwind stack: 1024 opening brackets (invalid)
    let unwind: Vec<u8> = vec![b'['; 1024];
    group.throughput(Throughput::Bytes(unwind.len() as u64));
    group.bench_function("unwind_stack", |b| {
        let mut v = Validator::default();
        b.iter(|| black_box(v.valid(black_box(&unwind))))
    });

    for &name in FILE_BENCHMARKS {
        let data = load_test_data(name);
        let label = name.split('.').next().unwrap();
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_function(label, |b| {
            let mut v = Validator::default();
            b.iter(|| black_box(v.valid(black_box(&data))))
        });
    }

    group.finish();
}

fn bench_calc_stats(c: &mut Criterion) {
    let mut group = c.benchmark_group("calc_stats");

    for &name in FILE_BENCHMARKS {
        let data = load_test_data(name);
        let label = name.split('.').next().unwrap();
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_function(label, |b| {
            let mut parser = Parser::new(1024);
            b.iter(|| {
                let s = calc_stats(&mut parser, black_box(&data));
                black_box(&s);
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_valid, bench_calc_stats);
criterion_main!(benches);
