pub mod target_db_finalizer;
pub mod target_db_preparator;

use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod};
use std::{env, time::Duration};

pub fn prepare_db_config(db_url: String) -> Config {
    let mut cfg = Config::new();
    cfg.url = Some(db_url);
    cfg.connect_timeout = Some(connect_timeout());
    cfg.keepalives = Some(keep_alives());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_pool_size()));
    cfg
}

fn connect_timeout() -> Duration {
    env::var("DB_CONNECT_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(180))
}

fn max_pool_size() -> usize {
    env::var("DB_MAX_POOL_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(24)
}

fn keep_alives() -> bool {
    env::var("DB_KEEP_ALIVES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(false)
}
