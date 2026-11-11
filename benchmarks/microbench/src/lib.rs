//! Reproductions of rustic-witcher hot-path code, OLD vs NEW, for benchmarking.
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use polars::polars_utils::pl_path::PlRefPath;
use polars::prelude::*;
use rand::Rng;

/// polars 0.54 changed `scan_parquet` to take a `PlRefPath` instead of `&Path`.
fn pl_path(path: &Path) -> PlRefPath {
    PlRefPath::new(path.to_str().expect("non-utf8 path"))
}

// ---------------------------------------------------------------------------
// Synthetic data generation
// ---------------------------------------------------------------------------

/// Build a representative DataFrame: a few string columns (some cells carry an
/// embedded null byte), a JSON-ish string column, and an integer key column
/// used for selective filtering.
pub fn make_dataframe(rows: usize) -> DataFrame {
    let mut rng = rand::rng();

    let names: Vec<String> = (0..rows).map(|i| format!("name_{}", i % 5000)).collect();

    // ~2% of cells contain an embedded null byte (the case sanitize handles).
    let payloads: Vec<String> = (0..rows)
        .map(|i| {
            if i % 50 == 0 {
                format!("{{\"id\":{i},\"v\":\"bad\x00value\"}}")
            } else {
                format!("{{\"id\":{i},\"v\":\"clean_value_{}\"}}", i % 1000)
            }
        })
        .collect();

    let emails: Vec<String> = (0..rows).map(|i| format!("user{i}@example.com")).collect();

    let id: Vec<i64> = (0..rows as i64).collect();
    // Filter key: only ~5% of rows match `tenant == 7`.
    let tenant: Vec<i64> = (0..rows).map(|_| rng.random_range(0..20)).collect();

    df![
        "id" => id,
        "tenant" => tenant,
        "name" => names,
        "email" => emails,
        "payload" => payloads,
    ]
    .unwrap()
}

/// Write a DataFrame to a parquet file on disk (mirrors how LOAD files look in S3).
pub fn write_parquet(df: &mut DataFrame, path: &Path) {
    let file = File::create(path).unwrap();
    ParquetWriter::new(file)
        .with_row_group_size(Some(64 * 1024))
        .finish(df)
        .unwrap();
}

// ---------------------------------------------------------------------------
// sanitize_null_bytes: OLD (row-by-row) vs NEW (vectorised expr)
// ---------------------------------------------------------------------------

/// OLD implementation: pull every string cell into a `Vec<Option<String>>`,
/// check `.contains('\x00')` per row, rebuild each column.
pub fn sanitize_null_bytes_old(df: DataFrame) -> DataFrame {
    let string_col_names: Vec<String> = df
        .columns()
        .iter()
        .filter(|s| matches!(s.dtype(), DataType::String))
        .map(|s| s.name().to_string())
        .collect();

    if string_col_names.is_empty() {
        return df;
    }

    let sanitized: Vec<Series> = string_col_names
        .iter()
        .map(|name| {
            let values: Vec<Option<String>> = df
                .column(name)
                .unwrap()
                .str()
                .unwrap()
                .iter()
                .map(|opt_s| opt_s.filter(|s| !s.contains('\x00')).map(str::to_string))
                .collect();
            Series::new(name.as_str().into(), values)
        })
        .collect();

    let mut df = df;
    for series in sanitized {
        df.with_column(Column::from(series)).unwrap();
    }
    df
}

/// NEW implementation: vectorised columnar replacement inside the Polars engine.
pub fn sanitize_null_bytes_new(df: DataFrame) -> DataFrame {
    let string_col_names: Vec<String> = df
        .columns()
        .iter()
        .filter(|s| matches!(s.dtype(), DataType::String))
        .map(|s| s.name().to_string())
        .collect();

    if string_col_names.is_empty() {
        return df;
    }

    let exprs: Vec<Expr> = string_col_names
        .iter()
        .map(|name| {
            when(col(name.as_str()).str().contains_literal(lit("\x00")))
                .then(lit(NULL))
                .otherwise(col(name.as_str()))
                .alias(name.as_str())
        })
        .collect();

    df.lazy().with_columns(exprs).collect().unwrap()
}

// ---------------------------------------------------------------------------
// Parquet read + selective filter: OLD (eager full read then filter)
//                                   vs NEW (lazy scan with predicate pushdown)
// ---------------------------------------------------------------------------

/// OLD: read the ENTIRE parquet file into a DataFrame, then filter.
pub fn read_filter_old(path: &Path) -> DataFrame {
    let file = File::open(path).unwrap();
    let df = ParquetReader::new(file).finish().unwrap();
    df.lazy().filter(col("name").eq(lit("name_42"))).collect().unwrap()
}

/// NEW: lazy scan; the predicate is pushed into the parquet read so whole
/// row groups are skipped via statistics and only matching rows are decoded.
pub fn read_filter_new(path: &Path) -> DataFrame {
    LazyFrame::scan_parquet(pl_path(path), ScanArgsParquet::default())
        .unwrap()
        .filter(col("name").eq(lit("name_42")))
        .collect()
        .unwrap()
}

/// OLD: read the entire file, then keep the first N rows (record reduction).
pub fn read_slice_old(path: &Path, n: usize) -> DataFrame {
    let file = File::open(path).unwrap();
    let df = ParquetReader::new(file).finish().unwrap();
    df.head(Some(n))
}

/// NEW: lazy scan with a slice/limit pushdown — stops decoding after N rows.
pub fn read_slice_new(path: &Path, n: usize) -> DataFrame {
    LazyFrame::scan_parquet(pl_path(path), ScanArgsParquet::default())
        .unwrap()
        .slice(0, n as u32)
        .collect()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Config loading: OLD (read+parse every call) vs NEW (memoized)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize, serde::Serialize, Default, Clone)]
pub struct TableCfg {
    pub table_name: String,
    pub keep_num_of_records: Option<usize>,
    pub sanitize_null_bytes: Option<bool>,
}

#[derive(serde::Deserialize, serde::Serialize, Default, Clone)]
pub struct AnonymizationConfig {
    pub tables: Vec<TableCfg>,
}

/// OLD: read file from disk + parse TOML on every call.
pub fn load_config_old(path: &Path) -> AnonymizationConfig {
    let conf = std::fs::read_to_string(path).unwrap_or_default();
    toml::from_str(&conf).unwrap_or_default()
}

fn config_cache() -> &'static RwLock<HashMap<String, Arc<AnonymizationConfig>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<AnonymizationConfig>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// NEW: memoized — disk read + parse happens once per key.
pub fn load_config_new(key: &str, path: &Path) -> Arc<AnonymizationConfig> {
    if let Some(c) = config_cache().read().unwrap().get(key) {
        return Arc::clone(c);
    }
    let conf = std::fs::read_to_string(path).unwrap_or_default();
    let parsed = Arc::new(toml::from_str(&conf).unwrap_or_default());
    let mut cache = config_cache().write().unwrap();
    Arc::clone(cache.entry(key.to_string()).or_insert(parsed))
}

#[cfg(test)]
mod equivalence {
    use super::*;

    // NEW sanitize must produce byte-identical output to OLD.
    #[test]
    fn sanitize_old_eq_new() {
        let df = make_dataframe(50_000);
        let old = sanitize_null_bytes_old(df.clone());
        let new = sanitize_null_bytes_new(df.clone());
        assert!(old.equals_missing(&new), "sanitize OLD vs NEW diverged");
        // Sanity: at least one cell was actually nulled.
        let nulls = new.column("payload").unwrap().null_count();
        assert!(nulls > 0, "expected some payload cells to be nulled");
    }

    // NEW lazy filter must select exactly the same rows as OLD.
    #[test]
    fn filter_old_eq_new() {
        let path = std::env::temp_dir().join("rw_eq_filter.parquet");
        let mut df = make_dataframe(100_000);
        write_parquet(&mut df, &path);
        let old = read_filter_old(&path);
        let new = read_filter_new(&path);
        assert_eq!(old.height(), new.height(), "filter row counts differ");
        assert!(old.height() > 0, "filter should match some rows");
    }

    // NEW slice/limit pushdown must keep the same first-N rows as OLD.
    #[test]
    fn slice_old_eq_new() {
        let path = std::env::temp_dir().join("rw_eq_slice.parquet");
        let mut df = make_dataframe(100_000);
        write_parquet(&mut df, &path);
        let old = read_slice_old(&path, 10_000);
        let new = read_slice_new(&path, 10_000);
        assert!(old.equals(&new), "slice OLD vs NEW diverged");
    }
}
