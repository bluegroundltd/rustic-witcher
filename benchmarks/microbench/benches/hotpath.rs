use std::hint::black_box;
use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rw_bench::*;

const ROWS: usize = 2_000_000;

fn tmp(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    p
}

fn bench_sanitize(c: &mut Criterion) {
    let df = make_dataframe(ROWS);
    let mut g = c.benchmark_group("sanitize_null_bytes");
    g.throughput(Throughput::Elements(ROWS as u64));
    g.sample_size(20);

    g.bench_function(BenchmarkId::new("old_row_by_row", ROWS), |b| {
        b.iter(|| black_box(sanitize_null_bytes_old(black_box(df.clone()))))
    });
    g.bench_function(BenchmarkId::new("new_vectorised", ROWS), |b| {
        b.iter(|| black_box(sanitize_null_bytes_new(black_box(df.clone()))))
    });
    g.finish();
}

fn bench_read_filter(c: &mut Criterion) {
    let path = tmp("rw_bench_data.parquet");
    let mut df = make_dataframe(ROWS);
    write_parquet(&mut df, &path);

    let mut g = c.benchmark_group("read_then_filter");
    g.throughput(Throughput::Elements(ROWS as u64));
    g.sample_size(20);

    g.bench_function(BenchmarkId::new("old_eager_read_then_filter", ROWS), |b| {
        b.iter(|| black_box(read_filter_old(black_box(&path))))
    });
    g.bench_function(BenchmarkId::new("new_lazy_scan_pushdown", ROWS), |b| {
        b.iter(|| black_box(read_filter_new(black_box(&path))))
    });
    g.finish();
}

fn bench_read_slice(c: &mut Criterion) {
    let path = tmp("rw_bench_data.parquet");
    let mut df = make_dataframe(ROWS);
    write_parquet(&mut df, &path);
    let keep = 10_000usize;

    let mut g = c.benchmark_group("read_then_keep_n_records");
    g.sample_size(20);

    g.bench_function(BenchmarkId::new("old_eager_read_then_head", keep), |b| {
        b.iter(|| black_box(read_slice_old(black_box(&path), keep)))
    });
    g.bench_function(BenchmarkId::new("new_lazy_scan_slice_pushdown", keep), |b| {
        b.iter(|| black_box(read_slice_new(black_box(&path), keep)))
    });
    g.finish();
}

fn bench_config(c: &mut Criterion) {
    // Build a realistic config with many tables.
    let cfg = AnonymizationConfig {
        tables: (0..200)
            .map(|i| TableCfg {
                table_name: format!("table_{i}"),
                keep_num_of_records: Some(1000),
                sanitize_null_bytes: Some(true),
            })
            .collect(),
    };
    let path = tmp("rw_bench_config.toml");
    std::fs::write(&path, toml::to_string(&cfg).unwrap()).unwrap();

    let mut g = c.benchmark_group("load_config_per_parquet_file");
    // Simulate a table with 64 LOAD files -> 64 config lookups.
    let calls = 64u64;

    g.bench_function("old_read_parse_each_call", |b| {
        b.iter(|| {
            for _ in 0..calls {
                black_box(load_config_old(black_box(&path)));
            }
        })
    });
    g.bench_function("new_cached", |b| {
        b.iter(|| {
            for _ in 0..calls {
                black_box(load_config_new(black_box("db-public"), black_box(&path)));
            }
        })
    });
    g.finish();
}

criterion_group!(
    benches,
    bench_sanitize,
    bench_read_filter,
    bench_read_slice,
    bench_config
);
criterion_main!(benches);
