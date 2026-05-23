use dashmap::DashMap;
use once_cell::sync::Lazy;
use crate::types::ExecutionResult;

pub static JOB_STORE: Lazy<DashMap<String, Option<ExecutionResult>>> = Lazy::new(|| {
    DashMap::new()
});

use tokio::sync::broadcast;

pub static BROADCASTS: Lazy<DashMap<String, broadcast::Sender<String>>> = Lazy::new(|| {
    DashMap::new()
});
