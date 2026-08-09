# Benchmarks

Performance evidence for the hot-path optimizations and the polars 0.54 upgrade.

All numbers below are medians from [Criterion](https://github.com/bheisler/criterion.rs),
2,000,000-row synthetic DataFrames, on a 24-core x86-64 Linux host. Reproduce with the
harness in [`benchmarks/microbench`](../benchmarks/microbench).

## Microbenchmarks: OLD vs NEW hot paths

Each benchmark runs the **exact OLD body** against the **exact NEW body** of a function we
changed, so the speedup reflects work removed — not work skipped. Equivalence tests
(`cargo test -p rustic-witcher-microbench`) assert NEW produces byte-identical output to OLD
(`equals_missing`, matching row counts, matching slices) before any timing is trusted.

| Hot path | OLD (median) | NEW (median) | Speedup | What changed |
|---|---|---|---|---|
| `sanitize_null_bytes` | 194 ms | 18.8 ms | **~10.3×** | row-by-row `Vec<Option<String>>` → vectorised `when/then/otherwise` |
| Parquet read + selective filter | 16.6 ms | 11.0 ms | **~1.5×** | eager full read + post-filter → lazy `scan_parquet` + predicate pushdown |
| Parquet read + `keep_num_of_records` | 15.7 ms | 3.93 ms | **~4.0×** | full read + `head(n)` → `scan_parquet` + slice/`n_rows` pushdown |
| Config load (per 64 Parquet files) | 7.42 ms | 1.55 µs | **~4,800×** | disk read + TOML parse per file → process-wide memoized `Arc` |

Notes:
- The read+filter speedup is conservative on local disk; against S3 the predicate also avoids
  transferring non-matching row groups over the network, so the real win is larger.
- These results hold on **both polars 0.48.1 and 0.54.4** (microbench currently pins 0.54.4).

## S3 scan path — integration test

[`benchmarks/s3-integration`](../benchmarks/s3-integration) validates the lazy `scan_parquet`
S3 path end-to-end against a local S3 API ([Floci](https://floci.io)/LocalStack-compatible),
using the same credential-provider wiring as the operator:

- predicate pushdown (`name == "name_42"`) returns exactly the matching rows;
- slice pushdown (`n_rows`) returns exactly N rows;
- `collect()` runs inside `spawn_blocking` on a multi-threaded runtime (polars cloud reads
  require it — a current-thread runtime panics with
  `can call blocking only when running on the multi-threaded runtime`).

## Running

```bash
# Microbenchmarks + correctness
cd benchmarks/microbench
cargo test          # equivalence: NEW == OLD
cargo bench         # timings (HTML report under target/criterion)

# S3 integration (needs Docker)
cd benchmarks/s3-integration
docker compose up -d
cargo test -- --nocapture
docker compose down
```
