use dashmap::DashMap;
use once_cell::sync::Lazy;
use crate::types::ExecutionResult;

pub static JOB_STORE: Lazy<DashMap<String, Option<ExecutionResult>>> = Lazy::new(|| {
    DashMap::new()
});
