use dashmap::DashMap;
use once_cell::sync::Lazy;
use crate::types::ExecutionResult;

pub static JOB_STORE: Lazy<DashMap<String, Option<ExecutionResult>>> = Lazy::new(|| {
    DashMap::new()
});

pub static JOB_META: Lazy<DashMap<String, i64>> = Lazy::new(|| {
    DashMap::new()
});

use tokio::sync::broadcast;

pub static BROADCASTS: Lazy<DashMap<String, broadcast::Sender<String>>> = Lazy::new(|| {
    DashMap::new()
});

use tokio::process::Child;
use std::sync::Arc;
use tokio::sync::Mutex;

// track running child processes (wrapped in Arc<Mutex<..>> so we can kill them)
pub static RUNNING_CHILDREN: Lazy<DashMap<String, Arc<Mutex<Option<Child>>>>> = Lazy::new(|| {
    DashMap::new()
});
