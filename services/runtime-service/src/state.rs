use dashmap::DashMap;
use once_cell::sync::Lazy;
use crate::types::ExecutionResult;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy)]
pub struct MetricsSnapshot {
    pub total_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub running_jobs: u64,
    pub cancelled_jobs: u64,
    pub total_execution_time_ms: u64,
    pub min_execution_time_ms: u64,
    pub max_execution_time_ms: u64,
    pub webhook_delivered: u64,
    pub webhook_failed: u64,
}

pub struct Metrics {
    pub total_jobs: AtomicU64,
    pub completed_jobs: AtomicU64,
    pub failed_jobs: AtomicU64,
    pub cancelled_jobs: AtomicU64,
    pub total_execution_time_ms: AtomicU64,
    pub min_execution_time_ms: AtomicU64,
    pub max_execution_time_ms: AtomicU64,
    pub webhook_delivered: AtomicU64,
    pub webhook_failed: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            total_jobs: AtomicU64::new(0),
            completed_jobs: AtomicU64::new(0),
            failed_jobs: AtomicU64::new(0),
            cancelled_jobs: AtomicU64::new(0),
            total_execution_time_ms: AtomicU64::new(0),
            min_execution_time_ms: AtomicU64::new(u64::MAX),
            max_execution_time_ms: AtomicU64::new(0),
            webhook_delivered: AtomicU64::new(0),
            webhook_failed: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            total_jobs: self.total_jobs.load(Ordering::Relaxed),
            completed_jobs: self.completed_jobs.load(Ordering::Relaxed),
            failed_jobs: self.failed_jobs.load(Ordering::Relaxed),
            running_jobs: self.total_jobs.load(Ordering::Relaxed) - (self.completed_jobs.load(Ordering::Relaxed) + self.failed_jobs.load(Ordering::Relaxed) + self.cancelled_jobs.load(Ordering::Relaxed)),
            cancelled_jobs: self.cancelled_jobs.load(Ordering::Relaxed),
            total_execution_time_ms: self.total_execution_time_ms.load(Ordering::Relaxed),
            min_execution_time_ms: self.min_execution_time_ms.load(Ordering::Relaxed),
            max_execution_time_ms: self.max_execution_time_ms.load(Ordering::Relaxed),
            webhook_delivered: self.webhook_delivered.load(Ordering::Relaxed),
            webhook_failed: self.webhook_failed.load(Ordering::Relaxed),
        }
    }

    pub fn record_job_started(&self) {
        self.total_jobs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_job_completed(&self, execution_time_ms: u64) {
        self.completed_jobs.fetch_add(1, Ordering::Relaxed);
        self.total_execution_time_ms.fetch_add(execution_time_ms, Ordering::Relaxed);
        let mut min = self.min_execution_time_ms.load(Ordering::Relaxed);
        while execution_time_ms < min {
            match self.min_execution_time_ms.compare_exchange(min, execution_time_ms, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => min = actual,
            }
        }
        let mut max = self.max_execution_time_ms.load(Ordering::Relaxed);
        while execution_time_ms > max {
            match self.max_execution_time_ms.compare_exchange(max, execution_time_ms, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => max = actual,
            }
        }
    }

    pub fn record_job_failed(&self) {
        self.failed_jobs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_job_cancelled(&self) {
        self.cancelled_jobs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_webhook_success(&self) {
        self.webhook_delivered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_webhook_failure(&self) {
        self.webhook_failed.fetch_add(1, Ordering::Relaxed);
    }
}

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

pub static METRICS: Lazy<Metrics> = Lazy::new(|| Metrics::new());
